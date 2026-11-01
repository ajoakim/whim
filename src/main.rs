#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use whim::args::{USAGE, parse_args};
use whim::backend::BACKEND;
use whim::bench::StartupClock;
use whim::file::{FileError, MappedFile, open_mapped};

fn main() -> eframe::Result {
    let t0 = std::time::Instant::now();
    let args = match parse_args(std::env::args_os().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    if args.help {
        println!("{USAGE}");
        return Ok(());
    }
    if args.version {
        println!("whim {} ({BACKEND})", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if let Some(secs) = args.bench_exit_after {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(secs));
            std::process::exit(0);
        });
    }
    let opened: Option<Result<MappedFile, FileError>> = args.path.as_deref().map(open_mapped);
    let options = eframe::NativeOptions {
        renderer: whim::backend::renderer(),
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Whim")
            .with_inner_size([1024.0, 720.0])
            .with_min_inner_size([480.0, 320.0]),
        ..Default::default()
    };
    let bench = args.bench;
    let ppp = args.ppp;
    eframe::run_native(
        "whim",
        options,
        Box::new(move |cc| {
            Ok(Box::new(whim::app::WhimApp::new(cc, opened, StartupClock::new(t0, bench), ppp)))
        }),
    )
}
