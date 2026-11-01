//! App shell: menu bar, status bar, central view, empty/error states.

use crate::backend::BACKEND;
use crate::bench::StartupClock;
use crate::file::{FileError, MappedFile};
use crate::theme;
use crate::view::{ViewState, rows};
use eframe::egui::{self, Align, Key, KeyboardShortcut, Layout, Modifiers, RichText};

const SOON: &str = "Coming in a later phase";

pub struct WhimApp {
    file: Option<Result<MappedFile, FileError>>,
    view: ViewState,
    clock: StartupClock,
    show_about: bool,
}

impl WhimApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        file: Option<Result<MappedFile, FileError>>,
        clock: StartupClock,
        ppp: Option<f32>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        if let Some(p) = ppp {
            cc.egui_ctx.set_pixels_per_point(p);
        }
        Self { file, view: ViewState::new(), clock, show_about: false }
    }
}

fn small(text: impl Into<String>, color: egui::Color32) -> RichText {
    RichText::new(text.into()).size(12.0).color(color)
}

fn divider(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(egui::vec2(1.0, 14.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 0.0, theme::SEPARATOR);
}

impl eframe::App for WhimApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.clock.on_frame(ui.ctx());

        let quit = KeyboardShortcut::new(Modifiers::COMMAND, Key::Q);
        let open = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
        if ui.input_mut(|i| i.consume_shortcut(&quit)) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let quit_text = ui.ctx().format_shortcut(&quit);
        let open_text = ui.ctx().format_shortcut(&open);

        egui::Panel::top("menu")
            .exact_size(24.0)
            .frame(egui::Frame::NONE.fill(theme::SECONDARY).inner_margin(egui::Margin::symmetric(8, 0)))
            .show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        ui.add_enabled(false, egui::Button::new("Open...").shortcut_text(open_text))
                            .on_disabled_hover_text(SOON);
                        if ui.add(egui::Button::new("Quit").shortcut_text(quit_text)).clicked() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button("View", |ui| {
                        ui.add_enabled(false, egui::Button::new("Wrap lines")).on_disabled_hover_text(SOON);
                        ui.add_enabled(false, egui::Button::new("Line numbers")).on_disabled_hover_text(SOON);
                    });
                    ui.menu_button("Help", |ui| {
                        if ui.button("About Whim").clicked() {
                            self.show_about = true;
                            ui.close();
                        }
                    });
                });
            });

        let top = self.view.top();
        let startup = self.clock.startup_ms();
        let file = &self.file;
        egui::Panel::bottom("status")
            .exact_size(24.0)
            .frame(egui::Frame::NONE.fill(theme::SECONDARY).inner_margin(egui::Margin::symmetric(8, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    match file {
                        Some(Err(e)) => {
                            ui.label(small(e.message(), theme::DESTRUCTIVE));
                        }
                        Some(Ok(f)) => {
                            ui.label(small(format!("Offset {}", rows::format_offset(top)), theme::MUTED));
                            divider(ui);
                            ui.label(small(format!("Size {}", rows::format_size(f.len)), theme::MUTED));
                        }
                        None => {}
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(small(BACKEND, theme::MUTED));
                        divider(ui);
                        let s = match startup {
                            Some(ms) => format!("Startup {ms} ms"),
                            None => "Startup \u{2014} ms".to_owned(),
                        };
                        ui.label(small(s, theme::MUTED));
                    });
                });
            });

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| match &self.file {
            Some(Ok(f)) => {
                self.view.show(ui, f.data());
            }
            Some(Err(e)) => {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new(e.message()).size(14.0).color(theme::DESTRUCTIVE));
                });
            }
            None => {
                ui.vertical_centered(|ui| {
                    ui.add_space((ui.available_height() / 2.0 - 20.0).max(0.0));
                    ui.label(RichText::new("No file open").size(14.0).color(theme::TEXT));
                    ui.label(
                        RichText::new("Run `whim <path>` or choose File > Open to view a file.")
                            .size(14.0)
                            .color(theme::MUTED),
                    );
                });
            }
        });

        if self.show_about {
            let mut open = true;
            egui::Window::new("About Whim").open(&mut open).collapsible(false).resizable(false).show(
                ui.ctx(),
                |ui| {
                    ui.label(format!(
                        "Whim {} \u{2014} toolkit: egui 0.37 ({})",
                        env!("CARGO_PKG_VERSION"),
                        BACKEND
                    ));
                },
            );
            self.show_about = open;
        }
    }
}
