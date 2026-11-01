#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use whim::args::{USAGE, parse_args};
use whim::backend::BACKEND;
use whim::bench::StartupClock;
use whim::file::{FileError, MappedFile, open_mapped};

/// Release builds use the GUI subsystem, so `--help`/`--version`/usage errors would print
/// nowhere. Attach to the parent console (if any) and point unset std handles at it.
#[cfg(windows)]
fn attach_parent_console() {
    use std::os::windows::io::IntoRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
        SetStdHandle,
    };
    // SAFETY: plain Win32 console calls; handles passed to SetStdHandle are owned raw
    // handles that are intentionally leaked for the life of the process.
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        for which in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
            let h = GetStdHandle(which);
            // Keep redirections (pipes/files) that the parent already set up.
            if !h.is_null() && h != INVALID_HANDLE_VALUE {
                continue;
            }
            if let Ok(f) = std::fs::OpenOptions::new().write(true).open("CONOUT$") {
                SetStdHandle(which, f.into_raw_handle() as _);
            }
        }
    }
}

#[cfg(not(windows))]
fn attach_parent_console() {}

fn main() -> eframe::Result {
    let t0 = std::time::Instant::now();
    let args = match parse_args(std::env::args_os().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            attach_parent_console();
            eprintln!("{e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    if args.help {
        attach_parent_console();
        println!("{USAGE}");
        return Ok(());
    }
    if args.version {
        attach_parent_console();
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
            Ok(Box::new(whim::app::WhimApp::new(
                cc,
                opened,
                StartupClock::new(t0, bench),
                ppp,
            )))
        }),
    )
}
