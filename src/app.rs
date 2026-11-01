//! Minimal stub; replaced by the full app shell in Task 2.

use crate::bench::StartupClock;
use crate::file::{FileError, MappedFile};
use eframe::egui;

pub struct WhimApp {
    _file: Option<Result<MappedFile, FileError>>,
    clock: StartupClock,
}

impl WhimApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        file: Option<Result<MappedFile, FileError>>,
        clock: StartupClock,
        ppp: Option<f32>,
    ) -> Self {
        crate::theme::apply(&cc.egui_ctx);
        if let Some(p) = ppp {
            cc.egui_ctx.set_pixels_per_point(p);
        }
        Self { _file: file, clock }
    }
}

impl eframe::App for WhimApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.clock.on_frame(ui.ctx());
    }
}
