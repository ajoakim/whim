//! Startup clock and the bench stdout contract parsed by `cargo xtask measure`.

use eframe::egui;
use std::io::Write;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub struct StartupClock {
    t0: Instant,
    bench: bool,
    frames: u32,
    startup_ms: Option<u64>,
}

impl StartupClock {
    pub fn new(t0: Instant, bench: bool) -> Self {
        Self {
            t0,
            bench,
            frames: 0,
            startup_ms: None,
        }
    }

    /// Call at the top of every `App::ui`. Requests exactly one repaint (frame 1).
    pub fn on_frame(&mut self, ctx: &egui::Context) {
        self.frames = self.frames.saturating_add(1);
        match self.frames {
            1 => ctx.request_repaint(),
            2 => {
                let ms = self.t0.elapsed().as_millis() as u64;
                self.startup_ms = Some(ms);
                let us = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_micros())
                    .unwrap_or(0);
                if self.bench {
                    let mut out = std::io::stdout().lock();
                    let _ = out.write_all(format_lines(crate::backend::BACKEND, ms, us).as_bytes());
                    let _ = out.flush();
                }
            }
            _ => {}
        }
    }

    pub fn startup_ms(&self) -> Option<u64> {
        self.startup_ms
    }
}

pub fn format_lines(backend: &str, startup_ms: u64, first_frame_unix_us: u128) -> String {
    format!(
        "backend={backend}\nstartup_ms={startup_ms}\nfirst_frame_unix_us={first_frame_unix_us}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_format() {
        assert_eq!(
            format_lines("glow", 142, 1761955200123456),
            "backend=glow\nstartup_ms=142\nfirst_frame_unix_us=1761955200123456\n"
        );
    }
}
