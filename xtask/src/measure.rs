//! `measure`: spawn whim N times, report startup median/max, settled idle RSS and idle CPU.
use std::io::{BufRead, BufReader, Read};
use std::path::PathBuf;
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sysinfo::{Pid, ProcessesToUpdate, System};

const MIB: f64 = 1_048_576.0;
/// Idle sampling starts this many seconds after launch (startup transient is gone by then).
const SETTLE_START_SECS: u64 = 20;

/// Minimum of the settled-window samples (the gate metric); 0 when empty.
pub fn window_min(v: &[f64]) -> f64 {
    v.iter().cloned().reduce(f64::min).unwrap_or(0.0)
}

#[derive(Default, Clone, Debug)]
pub struct Budgets {
    pub max_startup_ms: Option<f64>,
    pub max_rss_mb: Option<f64>,
    pub max_idle_cpu: Option<f64>,
}

#[derive(Default, Clone, Debug)]
pub struct Results {
    pub startup_median_ms: f64,
    pub idle_rss_mb: f64,
    pub idle_cpu_pct: f64,
}

pub fn median(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

pub fn parse_bench_line(line: &str) -> Option<(&str, &str)> {
    line.trim().split_once('=')
}

/// Microsecond stamps to milliseconds.
pub fn startup_total_ms(first_frame_unix_us: u128, spawn_unix_us: u128) -> f64 {
    first_frame_unix_us.saturating_sub(spawn_unix_us) as f64 / 1000.0
}

pub fn evaluate(r: &Results, b: &Budgets) -> Vec<String> {
    let mut v = Vec::new();
    if let Some(m) = b.max_startup_ms
        && r.startup_median_ms > m
    {
        v.push(format!(
            "median startup {:.1} ms > max {m} ms",
            r.startup_median_ms
        ));
    }
    if let Some(m) = b.max_rss_mb
        && r.idle_rss_mb > m
    {
        v.push(format!("idle RSS {:.1} MB > max {m} MB", r.idle_rss_mb));
    }
    if let Some(m) = b.max_idle_cpu
        && r.idle_cpu_pct > m
    {
        v.push(format!("idle CPU {:.2} % > max {m} %", r.idle_cpu_pct));
    }
    v
}

struct Opts {
    bin: Option<PathBuf>,
    file: Option<PathBuf>,
    runs: u32,
    idle_secs: u64,
    timeout_secs: u64,
    budgets: Budgets,
    label: String,
    extra: Vec<String>,
}

const HELP: &str = "cargo xtask measure [--bin PATH] [--file PATH] [--runs N=10] [--idle-secs S=10] [--timeout-secs T=60]
    [--max-startup-ms F] [--max-rss-mb F] [--max-idle-cpu F] [--label S=local] [--extra-arg ARG]...
  Exit codes: 0 pass, 1 budget exceeded, 2 launch failure / usage error.";

fn parse(args: &mut lexopt::Parser) -> Result<Option<Opts>, lexopt::Error> {
    use lexopt::prelude::*;
    let mut o = Opts {
        bin: None,
        file: None,
        runs: 10,
        idle_secs: 10,
        timeout_secs: 60,
        budgets: Budgets::default(),
        label: "local".into(),
        extra: Vec::new(),
    };
    while let Some(a) = args.next()? {
        match a {
            Long("bin") => o.bin = Some(args.value()?.into()),
            Long("file") => o.file = Some(args.value()?.into()),
            Long("runs") => o.runs = args.value()?.parse()?,
            Long("idle-secs") => o.idle_secs = args.value()?.parse()?,
            Long("timeout-secs") => o.timeout_secs = args.value()?.parse()?,
            Long("max-startup-ms") => o.budgets.max_startup_ms = Some(args.value()?.parse()?),
            Long("max-rss-mb") => o.budgets.max_rss_mb = Some(args.value()?.parse()?),
            Long("max-idle-cpu") => o.budgets.max_idle_cpu = Some(args.value()?.parse()?),
            Long("label") => o.label = args.value()?.string()?,
            Long("extra-arg") => o.extra.push(args.value()?.string()?),
            Long("help") | Short('h') => return Ok(None),
            other => return Err(other.unexpected()),
        }
    }
    if o.runs == 0 {
        return Err("--runs must be at least 1".into());
    }
    Ok(Some(o))
}

fn now_us() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0)
}

#[derive(Default)]
struct Sample {
    total_ms: f64,
    inapp_ms: Option<f64>,
    backend: Option<String>,
}

/// Spawn one run; Err(msg) on launch failure. The child is returned still alive.
fn launch(o: &Opts, bin: &PathBuf, file: &PathBuf) -> Result<(Child, Sample), String> {
    let exit_after = (o.idle_secs + o.timeout_secs + 5).to_string();
    let spawn_us = now_us();
    let mut child = Command::new(bin)
        .arg(file)
        .args(["--bench", "--bench-exit-after", &exit_after])
        .args(&o.extra)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not spawn {}: {e}", bin.display()))?;
    let (tx, rx) = mpsc::channel::<String>();
    let stdout = child.stdout.take().expect("piped stdout");
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut stderr = child.stderr.take().expect("piped stderr");
    let err_buf = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let err_buf2 = err_buf.clone();
    std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        if let Ok(mut g) = err_buf2.lock() {
            *g = s;
        }
    });

    let deadline = Instant::now() + Duration::from_secs(o.timeout_secs);
    let mut s = Sample::default();
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(line) => {
                if let Some((k, v)) = parse_bench_line(&line) {
                    match k {
                        "backend" => s.backend = Some(v.to_string()),
                        "startup_ms" => s.inapp_ms = v.parse().ok(),
                        // Contract line: `first_frame_unix_us=<u128>`
                        "first_frame_unix_us" => {
                            let ff: u128 = v
                                .parse()
                                .map_err(|_| format!("bad first_frame_unix_us: {v}"))?;
                            s.total_ms = startup_total_ms(ff, spawn_us);
                            return Ok((child, s));
                        }
                        _ => {}
                    }
                }
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                std::thread::sleep(Duration::from_millis(100));
                let stderr_txt = err_buf.lock().map(|g| g.clone()).unwrap_or_default();
                let why = match e {
                    RecvTimeoutError::Timeout => "timed out",
                    RecvTimeoutError::Disconnected => "child exited early",
                };
                return Err(format!("{why}\n{stderr_txt}"));
            }
        }
    }
}

fn mem_cpu(sys: &mut System, pid: Pid) -> Option<(f64, f64, u64)> {
    sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let p = sys.process(pid)?;
    Some((
        p.memory() as f64 / MIB,
        p.virtual_memory() as f64 / MIB,
        p.accumulated_cpu_time(),
    ))
}

pub fn run(mut args: lexopt::Parser) -> ExitCode {
    let o = match parse(&mut args) {
        Ok(Some(o)) => o,
        Ok(None) => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}\n{HELP}");
            return ExitCode::from(2);
        }
    };
    let bin = o.bin.clone().unwrap_or_else(crate::default_bin);

    let mut temp_file: Option<PathBuf> = None;
    let file = match &o.file {
        Some(f) => f.clone(),
        None => {
            let p = std::env::temp_dir().join("whim-measure-20g.sparse.bin");
            if let Err(e) = crate::sparse::create(&p, 20u64 << 30, true) {
                eprintln!("error: could not create sparse file: {e}");
                return ExitCode::from(2);
            }
            temp_file = Some(p.clone());
            p
        }
    };
    let file_len = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);

    let code = run_inner(&o, &bin, &file, file_len);

    if let Some(p) = temp_file {
        let _ = std::fs::remove_file(p);
    }
    code
}

fn run_inner(o: &Opts, bin: &PathBuf, file: &PathBuf, file_len: u64) -> ExitCode {
    let mut totals = Vec::new();
    let mut inapp = Vec::new();
    let mut backend = String::from("unknown");
    let (mut idle_rss, mut idle_virt, mut idle_cpu) = (0.0f64, 0.0f64, 0.0f64);
    let mut idle_rss_peak = 0.0f64;

    for i in 0..o.runs {
        let (mut child, s) = match launch(o, bin, file) {
            Ok(v) => v,
            Err(msg) => {
                eprintln!("{msg}");
                eprintln!("error: whim did not present a first frame (launch/GL failure)");
                return ExitCode::from(2);
            }
        };
        totals.push(s.total_ms);
        if let Some(m) = s.inapp_ms {
            inapp.push(m);
        }
        if let Some(b) = s.backend {
            backend = b;
        }
        if i + 1 == o.runs {
            let pid = Pid::from_u32(child.id());
            let Ok(mut sys) = System::new() else {
                eprintln!("error: sysinfo could not initialise");
                let _ = child.kill();
                let _ = child.wait();
                return ExitCode::from(2);
            };
            // Settled window: wait until +SETTLE_START_SECS after launch, then sample
            // once a second for SETTLE_WINDOW_SECS. The gate metric is the MIN working
            // set of the window; the max over all samples (incl. startup) is reported only.
            let launched = Instant::now() - Duration::from_secs_f64(s.total_ms / 1000.0);
            let mut all_rss: Vec<f64> = Vec::new();
            let mut window: Vec<f64> = Vec::new();
            let mut virt: Vec<f64> = Vec::new();
            let mut cpu_first: Option<u64> = None;
            let mut cpu_last: Option<(u64, Instant)> = None;
            let mut cpu_t0 = Instant::now();
            let start = Duration::from_secs(SETTLE_START_SECS);
            let end = start + Duration::from_secs(o.idle_secs);
            // One early sample so the reported peak includes the startup transient.
            std::thread::sleep(Duration::from_secs(2));
            if let Some(m) = mem_cpu(&mut sys, pid) {
                all_rss.push(m.0);
            }
            loop {
                let el = launched.elapsed();
                if el > end {
                    break;
                }
                if el >= start {
                    match mem_cpu(&mut sys, pid) {
                        Some(m) => {
                            all_rss.push(m.0);
                            window.push(m.0);
                            virt.push(m.1);
                            if cpu_first.is_none() {
                                cpu_first = Some(m.2);
                                cpu_t0 = Instant::now();
                            }
                            cpu_last = Some((m.2, Instant::now()));
                        }
                        None => {
                            eprintln!(
                                "warning: could not sample process (exited during idle window?)"
                            );
                            break;
                        }
                    }
                }
                std::thread::sleep(Duration::from_secs(1));
            }
            idle_rss = window_min(&window);
            idle_rss_peak = all_rss.iter().cloned().fold(0.0, f64::max);
            idle_virt = virt.iter().cloned().fold(0.0, f64::max);
            if let (Some(f), Some((l, t))) = (cpu_first, cpu_last) {
                let secs = t.duration_since(cpu_t0).as_secs_f64().max(1.0);
                idle_cpu = l.saturating_sub(f) as f64 / (secs * 1000.0) * 100.0;
            }
        }
        let _ = child.kill();
        let _ = child.wait();
    }

    let first_run = totals[0];
    let mut sorted = totals.clone();
    let startup_median = median(&mut sorted);
    let startup_max = totals.iter().cloned().fold(0.0, f64::max);
    let inapp_median = median(&mut inapp.clone());

    let results = Results {
        startup_median_ms: startup_median,
        idle_rss_mb: idle_rss,
        idle_cpu_pct: idle_cpu,
    };
    let violations = evaluate(&results, &o.budgets);
    let pass = violations.is_empty();

    println!("backend={backend}");
    println!("runs={}", o.runs);
    println!("first_run_ms={first_run:.1}");
    println!("startup_median_ms={startup_median:.1}");
    println!("startup_max_ms={startup_max:.1}");
    println!("inapp_startup_median_ms={inapp_median:.1}");
    println!("idle_rss_mb={idle_rss:.1}");
    println!("idle_rss_peak_mb={idle_rss_peak:.1}");
    println!("idle_virtual_mb={idle_virt:.1}");
    println!("idle_cpu_pct={idle_cpu:.2}");

    let j = serde_json::json!({
        "label": o.label, "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "backend": backend, "runs": o.runs, "file": file.display().to_string(), "file_len": file_len,
        "startup_total_ms": totals, "startup_median_ms": startup_median, "startup_max_ms": startup_max,
        "first_run_ms": first_run, "inapp_startup_median_ms": inapp_median,
        "idle_secs": o.idle_secs, "idle_rss_mb": idle_rss, "idle_rss_peak_mb": idle_rss_peak, "idle_virtual_mb": idle_virt,
        "idle_cpu_pct": idle_cpu,
        "budgets": {
            "max_startup_ms": o.budgets.max_startup_ms,
            "max_rss_mb": o.budgets.max_rss_mb,
            "max_idle_cpu": o.budgets.max_idle_cpu,
        },
        "violations": violations, "pass": pass,
    });
    let path = crate::measure_dir().join(format!("measure-{}.json", o.label));
    if let Err(e) = std::fs::write(&path, serde_json::to_string_pretty(&j).unwrap()) {
        eprintln!("warning: could not write {}: {e}", path.display());
    }
    for v in &violations {
        println!("BUDGET FAIL: {v}");
    }
    if pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_odd_even() {
        assert_eq!(median(&mut [5.0, 1.0, 3.0]), 3.0);
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), 2.5);
    }

    #[test]
    fn bench_line_parsing() {
        assert_eq!(
            parse_bench_line("startup_ms=142"),
            Some(("startup_ms", "142"))
        );
        assert_eq!(parse_bench_line("garbage"), None);
    }

    #[test]
    fn budgets_evaluate() {
        let b = Budgets {
            max_startup_ms: Some(300.0),
            max_rss_mb: Some(70.0),
            max_idle_cpu: Some(1.0),
        };
        let ok = Results {
            startup_median_ms: 250.0,
            idle_rss_mb: 50.0,
            idle_cpu_pct: 0.5,
        };
        assert!(evaluate(&ok, &b).is_empty());
        assert_eq!(
            evaluate(
                &Results {
                    startup_median_ms: 350.0,
                    ..ok.clone()
                },
                &b
            )
            .len(),
            1
        );
        assert_eq!(
            evaluate(
                &Results {
                    idle_rss_mb: 80.0,
                    ..ok.clone()
                },
                &b
            )
            .len(),
            1
        );
        assert_eq!(
            evaluate(
                &Results {
                    idle_cpu_pct: 1.5,
                    ..ok.clone()
                },
                &b
            )
            .len(),
            1
        );
        let huge = Results {
            startup_median_ms: 1e9,
            idle_rss_mb: 1e9,
            idle_cpu_pct: 1e9,
        };
        assert!(evaluate(&huge, &Budgets::default()).is_empty());
    }

    #[test]
    fn window_min_is_minimum() {
        assert_eq!(window_min(&[60.0, 58.0, 59.5]), 58.0);
        assert_eq!(window_min(&[]), 0.0);
    }

    #[test]
    fn startup_from_stamps() {
        assert_eq!(
            startup_total_ms(1_700_000_250_000, 1_700_000_000_000),
            250.0
        );
    }
}
