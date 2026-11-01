//! `check-size`: stripped binary size gate. MB means MiB (1,048,576 bytes).
use std::path::PathBuf;
use std::process::ExitCode;

pub const MIB: f64 = 1_048_576.0;

pub struct SizeReport {
    pub bytes: u64,
    pub mb: f64,
    pub max_mb: Option<f64>,
    pub pass: bool,
}

pub fn evaluate(bytes: u64, max_mb: Option<f64>) -> SizeReport {
    let mb = bytes as f64 / MIB;
    let pass = max_mb.is_none_or(|m| mb <= m);
    SizeReport {
        bytes,
        mb,
        max_mb,
        pass,
    }
}

const HELP: &str = "cargo xtask check-size [--bin PATH] [--max-mb F] [--label S]\n  MB means MiB (1,048,576 bytes). Exits 1 when the binary exceeds --max-mb.";

type Parsed = (Option<PathBuf>, Option<f64>, String);

fn parse(args: &mut lexopt::Parser) -> Result<Option<Parsed>, lexopt::Error> {
    use lexopt::prelude::*;
    let mut bin = None;
    let mut max_mb = None;
    let mut label = "local".to_string();
    while let Some(a) = args.next()? {
        match a {
            Long("bin") => bin = Some(args.value()?.into()),
            Long("max-mb") => max_mb = Some(args.value()?.parse()?),
            Long("label") => label = args.value()?.string()?,
            Long("help") | Short('h') => return Ok(None),
            other => return Err(other.unexpected()),
        }
    }
    if !crate::valid_label(&label) {
        return Err("--label must match [A-Za-z0-9._-]+".into());
    }
    Ok(Some((bin, max_mb, label)))
}

pub fn run(mut args: lexopt::Parser) -> ExitCode {
    let (bin, max_mb, label) = match parse(&mut args) {
        Ok(Some(v)) => v,
        Ok(None) => {
            println!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: {e}\n{HELP}");
            return ExitCode::from(2);
        }
    };
    let bin = bin.unwrap_or_else(crate::default_bin);
    let bytes = match std::fs::metadata(&bin) {
        Ok(m) => m.len(),
        Err(_) => {
            eprintln!("error: binary not found, run cargo build --release first");
            return ExitCode::from(2);
        }
    };
    let r = evaluate(bytes, max_mb);
    println!("size_bytes={}", r.bytes);
    println!("size_mb={:.2}", r.mb);
    match r.max_mb {
        Some(m) => println!("max_mb={m}"),
        None => println!("max_mb=none"),
    }
    println!("result={}", if r.pass { "PASS" } else { "FAIL" });
    let j = serde_json::json!({
        "label": label, "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "bytes": r.bytes, "mb": r.mb, "max_mb": r.max_mb, "pass": r.pass,
    });
    let path = crate::measure_dir().join(format!("size-{label}.json"));
    if let Err(e) = std::fs::write(&path, serde_json::to_string_pretty(&j).unwrap()) {
        eprintln!("warning: could not write {}: {e}", path.display());
    }
    if r.pass {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_budget_fails() {
        assert!(!evaluate(11_000_000, Some(10.0)).pass);
    }

    #[test]
    fn under_budget_passes() {
        assert!(evaluate(5_000_000, Some(10.0)).pass);
    }

    #[test]
    fn report_only_always_passes() {
        assert!(evaluate(u64::MAX / 2, None).pass);
        assert!((evaluate(1_048_576, None).mb - 1.0).abs() < 1e-9);
    }
}
