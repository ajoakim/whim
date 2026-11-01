//! Bare eframe baseline: one label, no file, no custom painting. Same renderer selection
//! and `--bench` stdout contract as the whim binary, so `cargo xtask measure --bin` can
//! report the toolkit's own memory floor next to whim's. Report-only; never gated.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use whim::args::{USAGE, parse_args};
use whim::bench::StartupClock;

struct Bare {
    clock: StartupClock,
}

impl eframe::App for Bare {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.clock.on_frame(ui.ctx());
        ui.label("bare eframe");
    }
}

fn main() -> eframe::Result {
    let t0 = std::time::Instant::now();
    let args = match parse_args(std::env::args_os().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    if let Some(secs) = args.bench_exit_after {
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(secs));
            std::process::exit(0);
        });
    }
    let options = eframe::NativeOptions {
        renderer: whim::backend::renderer(),
        viewport: egui::ViewportBuilder::default()
            .with_title("bare")
            .with_inner_size([1024.0, 720.0]),
        ..Default::default()
    };
    let bench = args.bench;
    eframe::run_native(
        "bare",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(Bare {
                clock: StartupClock::new(t0, bench),
            }))
        }),
    )
}
