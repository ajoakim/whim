//! Dialogs and non-blocking native file picker worker (D-14, D-15, OPEN-09).

pub use crate::copy::CopyTier;
use crate::theme;
use eframe::egui::{self, Align, Align2, Key, Layout, RichText, vec2};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyResult {
    Copy,
    DontCopy,
}

#[derive(Debug)]
pub struct CopyDialog {
    pub tier: CopyTier,
    pub size_text: String,
    pub open: bool,
}

impl Default for CopyDialog {
    fn default() -> Self {
        Self {
            tier: CopyTier::Immediate,
            size_text: String::new(),
            open: false,
        }
    }
}

impl CopyDialog {
    pub fn new(tier: CopyTier, size_text: String) -> Self {
        Self {
            tier,
            size_text,
            open: true,
        }
    }

    pub fn shown(&self) -> bool {
        self.open
    }

    pub fn body_text(&self) -> String {
        match self.tier {
            CopyTier::OverCap => format!(
                "The selection is {}, which is over the 256 MB copy limit. Select a smaller range.",
                self.size_text
            ),
            CopyTier::Confirm | CopyTier::Immediate => {
                format!("The selection is {}.", self.size_text)
            }
        }
    }

    pub fn primary_button_label(&self) -> String {
        match self.tier {
            CopyTier::OverCap => "Close".to_owned(),
            CopyTier::Confirm | CopyTier::Immediate => format!("Copy {}", self.size_text),
        }
    }
}

/// Spawns `rfd::FileDialog::pick_file()` on a background thread so the UI keeps repainting
/// (Pattern 5, D-15).
pub fn open_dialog_worker(
    ctx: egui::Context,
    last_dir: Option<PathBuf>,
) -> Receiver<Option<PathBuf>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut dlg = rfd::FileDialog::new();
        if let Some(dir) = last_dir
            && dir.is_dir()
        {
            dlg = dlg.set_directory(dir);
        }
        let picked = dlg.pick_file();
        if tx.send(picked).is_ok() {
            ctx.request_repaint();
        }
    });
    rx
}

/// Renders the modal copy-size confirmation / over-cap dialog. Returns `Some(CopyResult)`
/// when the user makes a choice or presses Esc.
pub fn confirm_copy_ui(ctx: &egui::Context, dialog: &mut CopyDialog) -> Option<CopyResult> {
    if !dialog.open {
        return None;
    }

    // Dimmed scrim behind modal.
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::unique("copy_scrim"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (r, _) = ui.allocate_exact_size(screen.size(), egui::Sense::click());
            ui.painter().rect_filled(r, 0.0, theme::OVERLAY_SCRIM);
        });

    let mut result = None;
    if ctx.input(|i| i.key_pressed(Key::Escape)) {
        dialog.open = false;
        return Some(CopyResult::DontCopy);
    }

    let body = dialog.body_text();
    let primary = dialog.primary_button_label();
    let over_cap = dialog.tier == CopyTier::OverCap;

    egui::Window::new("Copy large selection?")
        .collapsible(false)
        .resizable(false)
        .fixed_size(vec2(400.0, 0.0))
        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            ui.label(RichText::new(body).size(14.0).color(theme::TEXT));
            ui.add_space(12.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if over_cap {
                    if ui.button(&primary).clicked() {
                        result = Some(CopyResult::DontCopy);
                    }
                } else {
                    if ui.button(&primary).clicked() {
                        result = Some(CopyResult::Copy);
                    }
                    if ui.button("Don't Copy").clicked() {
                        result = Some(CopyResult::DontCopy);
                    }
                }
            });
        });

    if result.is_some() {
        dialog.open = false;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::copy::{COPY_HARD_CAP, COPY_IMMEDIATE_MAX, CopyKind, tier};
    use crate::open::eol_of_sample;
    use crate::source::EolKind;

    #[test]
    fn copy_tier_text() {
        let t_confirm = tier(CopyKind::Text, COPY_IMMEDIATE_MAX + 1);
        assert_eq!(t_confirm, CopyTier::Confirm);
        let dlg = CopyDialog::new(t_confirm, "16.0 MB".into());
        assert!(dlg.shown());
        assert_eq!(dlg.body_text(), "The selection is 16.0 MB.");
        assert_eq!(dlg.primary_button_label(), "Copy 16.0 MB");

        let t_over = tier(CopyKind::Text, COPY_HARD_CAP + 1);
        assert_eq!(t_over, CopyTier::OverCap);
        let dlg2 = CopyDialog::new(t_over, "256.0 MB".into());
        assert!(dlg2.body_text().contains("over the 256 MB copy limit"));
        assert_eq!(dlg2.primary_button_label(), "Close");
    }

    #[test]
    fn eol_detected_with_binary_header() {
        let sample = b"\x00\x01hdr\r\nline2\r\n";
        assert_eq!(eol_of_sample(sample), EolKind::Crlf);
    }
}
