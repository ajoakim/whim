//! App shell: menu bar, tab strip, binary banner, Go-to box, dialogs, status bar and central view.

use crate::backend::BACKEND;
use crate::bench::StartupClock;
use crate::copy::{self, CopyKind, CopyMsg, CopyTier, spawn_copy};
use crate::dialogs::{CopyDialog, CopyResult, confirm_copy_ui, open_dialog_worker};
use crate::doc::{Doc, LARGE_WRAP_LIMIT, PendingJump, ViewMode};
use crate::goto;
use crate::lineindex::Exactness;
use crate::open::{self, OpenResult};
use crate::source::EolKind;
use crate::status::{self, Status, StatusCtx, copy_size_text};
use crate::theme;
use crate::view::textview::TextViewCtx;
use eframe::egui::{
    self, Align, Align2, Key, KeyboardShortcut, Layout, Modifiers, RichText, Stroke, vec2,
};
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::Duration;

const NOTICE_TTL: Duration = Duration::from_secs(6);

pub struct WhimApp {
    doc: Option<Doc>,
    open_error: Option<String>,
    last_dir: Option<PathBuf>,
    pending_open: Option<Receiver<OpenResult>>,
    pending_dialog: Option<Receiver<Option<PathBuf>>>,
    open_gen: u64,
    copy_gen: u64,
    copying_label: Option<String>,
    copy_dialog: CopyDialog,
    status: Status,
    goto_visible: bool,
    goto_input: String,
    goto_just_opened: bool,
    clock: StartupClock,
    bench: bool,
    show_about: bool,
}

impl WhimApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        initial_path: Option<PathBuf>,
        clock: StartupClock,
        bench: bool,
        ppp: Option<f32>,
    ) -> Self {
        theme::apply(&cc.egui_ctx);
        if let Some(p) = ppp {
            cc.egui_ctx.set_pixels_per_point(p);
        }
        let mut app = Self {
            doc: None,
            open_error: None,
            last_dir: None,
            pending_open: None,
            pending_dialog: None,
            open_gen: 0,
            copy_gen: 0,
            copying_label: None,
            copy_dialog: CopyDialog::default(),
            status: Status::new(),
            goto_visible: false,
            goto_input: String::new(),
            goto_just_opened: false,
            clock,
            bench,
            show_about: false,
        };
        if let Some(path) = initial_path {
            app.request_open(&cc.egui_ctx, path);
        }
        app
    }

    /// Starts opening `path` on a background worker thread (D-19). Any in-flight open or
    /// previous document's indexer is cancelled / superseded by `open_gen`.
    fn request_open(&mut self, ctx: &egui::Context, path: PathBuf) {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            self.last_dir = Some(parent.to_path_buf());
        }
        self.open_gen = self.open_gen.wrapping_add(1);
        let generation = self.open_gen;
        let ctx_c = ctx.clone();
        self.pending_open = Some(open::spawn_open(
            generation,
            path,
            Box::new(move || ctx_c.request_repaint()),
        ));
    }

    fn start_copy(&mut self, ctx: &egui::Context, kind: CopyKind) {
        let Some(doc) = self.doc.as_mut() else {
            return;
        };
        if doc.sel.is_empty() {
            return;
        }
        let range = doc.sel.range();
        let Ok(scan_src) = doc.source.reopen() else {
            doc.frozen = true;
            self.status.show_error("file changed on disk");
            return;
        };
        self.copy_gen = self.copy_gen.wrapping_add(1);
        let generation = self.copy_gen;
        let ctx_c = ctx.clone();
        doc.copy_job = Some(spawn_copy(
            generation,
            Box::new(scan_src),
            range,
            kind,
            Box::new(move || ctx_c.request_repaint()),
        ));
    }

    fn trigger_copy(&mut self, ctx: &egui::Context) {
        let Some(doc) = self.doc.as_ref() else {
            return;
        };
        if doc.sel.is_empty() || doc.frozen {
            return;
        }
        let kind = match doc.mode {
            ViewMode::Text => CopyKind::Text,
            ViewMode::Hex => CopyKind::Hex,
        };
        let bytes = doc.sel.len();
        let out_bytes = copy::output_size(kind, bytes);
        match copy::tier(kind, bytes) {
            CopyTier::Immediate => self.start_copy(ctx, kind),
            CopyTier::Confirm => {
                self.copy_dialog = CopyDialog::new(CopyTier::Confirm, copy_size_text(out_bytes));
            }
            CopyTier::OverCap => {
                self.copy_dialog = CopyDialog::new(CopyTier::OverCap, copy_size_text(out_bytes));
            }
        }
    }

    fn drain_workers(&mut self, ctx: &egui::Context) {
        // 1. Native File > Open dialog result.
        if let Some(rx) = &self.pending_dialog
            && let Ok(picked) = rx.try_recv()
        {
            self.pending_dialog = None;
            if let Some(path) = picked {
                self.request_open(ctx, path);
            }
        }

        // 2. Open worker result.
        if let Some(rx) = &self.pending_open
            && let Ok((generation, _path, result)) = rx.try_recv()
        {
            self.pending_open = None;
            if generation == self.open_gen {
                match result {
                    Ok(opened) => {
                        self.open_error = None;
                        self.status.clear();
                        let ctx_c = ctx.clone();
                        let doc = Doc::new(
                            opened,
                            generation,
                            Box::new(move || ctx_c.request_repaint()),
                        );
                        if doc.wrap_auto_disabled {
                            self.status
                                .show("Word wrap off for large files", Some(NOTICE_TTL));
                            ctx.request_repaint_after(NOTICE_TTL);
                        }
                        self.doc = Some(doc);
                    }
                    Err(e) => {
                        if self.doc.is_some() {
                            self.status.show_error(e.short_message());
                        } else {
                            self.open_error = Some(e.message());
                        }
                    }
                }
            }
        }

        // 3. Background line indexer messages.
        if let Some(doc) = self.doc.as_mut() {
            let mut msgs = Vec::new();
            if let Some(job) = &doc.index_job {
                while let Ok(m) = job.rx.try_recv() {
                    msgs.push(m);
                }
            }
            for m in msgs {
                if let Some(err) = doc.handle_index_msg(m) {
                    doc.frozen = true;
                    self.status.show_error(err);
                }
            }

            // 4. Copy worker messages.
            let mut cmsgs = Vec::new();
            if let Some(job) = &doc.copy_job {
                while let Ok(m) = job.rx.try_recv() {
                    cmsgs.push(m);
                }
            }
            for m in cmsgs {
                if let CopyMsg::Progress { done, total, .. } = &m {
                    self.copying_label = Some(format!(
                        "Copying {} of {}",
                        copy_size_text(*done),
                        copy_size_text(*total)
                    ));
                    continue;
                }
                if let Some(res) = doc.handle_copy_msg(m) {
                    self.copying_label = None;
                    match res {
                        Ok(text) => ctx.copy_text(text),
                        Err(e) => self.status.show_error(e),
                    }
                } else {
                    self.copying_label = None;
                }
            }
        }
    }
}

fn truncate_middle(name: &str, max_chars: usize) -> String {
    let count = name.chars().count();
    if count <= max_chars || max_chars < 5 {
        return name.to_owned();
    }
    let keep = max_chars - 1;
    let left = keep / 2;
    let right = keep - left;
    let head: String = name.chars().take(left).collect();
    let tail: String = name.chars().skip(count - right).collect();
    format!("{head}\u{2026}{tail}")
}

impl eframe::App for WhimApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.clock.on_frame(&ctx);
        self.status.poll();
        self.drain_workers(&ctx);

        // Drag & drop (OPEN-02, D-15): take path() only, never .bytes(); first file only.
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .collect()
        });
        if let Some(first) = dropped.into_iter().next() {
            self.request_open(&ctx, first);
        }

        // Global shortcuts.
        let quit_sc = KeyboardShortcut::new(Modifiers::COMMAND, Key::Q);
        let open_sc = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
        let copy_sc = KeyboardShortcut::new(Modifiers::COMMAND, Key::C);
        let goto_sc = KeyboardShortcut::new(Modifiers::COMMAND, Key::G);
        let hex_sc = KeyboardShortcut::new(Modifiers::COMMAND, Key::H);
        let wrap_sc = KeyboardShortcut::new(Modifiers::ALT, Key::Z);

        if ui.input_mut(|i| i.consume_shortcut(&quit_sc)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ui.input_mut(|i| i.consume_shortcut(&open_sc)) && self.pending_dialog.is_none() {
            self.pending_dialog = Some(open_dialog_worker(ctx.clone(), self.last_dir.clone()));
        }
        if self.doc.is_some() {
            if ui.input_mut(|i| i.consume_shortcut(&goto_sc)) {
                self.goto_visible = true;
                self.goto_just_opened = true;
            }
            if ui.input_mut(|i| i.consume_shortcut(&hex_sc))
                && let Some(doc) = self.doc.as_mut()
            {
                doc.toggle_mode();
            }
            if ui.input_mut(|i| i.consume_shortcut(&wrap_sc))
                && let Some(doc) = self.doc.as_mut()
            {
                doc.text_view.wrap = !doc.text_view.wrap;
                if doc.text_view.wrap && doc.len() > LARGE_WRAP_LIMIT {
                    self.status
                        .show("Word wrap off for large files", Some(NOTICE_TTL));
                    ctx.request_repaint_after(NOTICE_TTL);
                }
            }
            if !self.goto_visible
                && !self.copy_dialog.shown()
                && !self.show_about
                && ui.input_mut(|i| i.consume_shortcut(&copy_sc))
            {
                self.trigger_copy(&ctx);
            }
        }

        // Esc priority (D-12): Go-to box > copy dialog > About > cancel copy > cancel indexing.
        if !self.copy_dialog.shown() && ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.goto_visible {
                self.goto_visible = false;
            } else if self.show_about {
                self.show_about = false;
            } else if let Some(doc) = self.doc.as_mut() {
                if doc.copy_job.is_some() {
                    doc.cancel_copy();
                    self.copying_label = None;
                } else if doc.index_job.is_some() {
                    doc.cancel_index();
                }
            }
        }

        let open_text = ctx.format_shortcut(&open_sc);
        let quit_text = ctx.format_shortcut(&quit_sc);
        let copy_text_sc = ctx.format_shortcut(&copy_sc);
        let goto_text = ctx.format_shortcut(&goto_sc);
        let hex_text = ctx.format_shortcut(&hex_sc);
        let wrap_text = ctx.format_shortcut(&wrap_sc);

        // Menu bar (24 pt).
        egui::Panel::top("menu")
            .exact_size(24.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::SECONDARY)
                    .inner_margin(egui::Margin::symmetric(8, 0)),
            )
            .show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui
                            .add(egui::Button::new("Open...").shortcut_text(open_text))
                            .clicked()
                        {
                            if self.pending_dialog.is_none() {
                                self.pending_dialog =
                                    Some(open_dialog_worker(ctx.clone(), self.last_dir.clone()));
                            }
                            ui.close();
                        }
                        if ui
                            .add(egui::Button::new("Quit").shortcut_text(quit_text))
                            .clicked()
                        {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button("Edit", |ui| {
                        let can_copy = self
                            .doc
                            .as_ref()
                            .is_some_and(|d| !d.sel.is_empty() && !d.frozen);
                        if ui
                            .add_enabled(
                                can_copy,
                                egui::Button::new("Copy").shortcut_text(copy_text_sc),
                            )
                            .clicked()
                        {
                            self.trigger_copy(&ctx);
                            ui.close();
                        }
                    });
                    ui.menu_button("View", |ui| {
                        let has_doc = self.doc.is_some();
                        let (wrap_on, ln_on, hex_on) = self
                            .doc
                            .as_ref()
                            .map(|d| {
                                (
                                    d.text_view.wrap,
                                    d.text_view.line_numbers,
                                    d.mode == ViewMode::Hex,
                                )
                            })
                            .unwrap_or((true, true, false));
                        let wrap_lbl = if wrap_on {
                            "\u{2713} Word wrap"
                        } else {
                            "   Word wrap"
                        };
                        if ui
                            .add_enabled(
                                has_doc && !hex_on,
                                egui::Button::new(wrap_lbl).shortcut_text(wrap_text),
                            )
                            .clicked()
                        {
                            if let Some(doc) = self.doc.as_mut() {
                                doc.text_view.wrap = !doc.text_view.wrap;
                            }
                            ui.close();
                        }
                        let ln_lbl = if ln_on {
                            "\u{2713} Line numbers"
                        } else {
                            "   Line numbers"
                        };
                        if ui
                            .add_enabled(has_doc && !hex_on, egui::Button::new(ln_lbl))
                            .clicked()
                        {
                            if let Some(doc) = self.doc.as_mut() {
                                doc.text_view.line_numbers = !doc.text_view.line_numbers;
                            }
                            ui.close();
                        }
                        let hex_lbl = if hex_on {
                            "\u{2713} Hex view"
                        } else {
                            "   Hex view"
                        };
                        if ui
                            .add_enabled(
                                has_doc,
                                egui::Button::new(hex_lbl).shortcut_text(hex_text),
                            )
                            .clicked()
                        {
                            if let Some(doc) = self.doc.as_mut() {
                                doc.toggle_mode();
                            }
                            ui.close();
                        }
                        ui.separator();
                        if ui
                            .add_enabled(
                                has_doc,
                                egui::Button::new("Go to...").shortcut_text(goto_text),
                            )
                            .clicked()
                        {
                            self.goto_visible = true;
                            self.goto_just_opened = true;
                            ui.close();
                        }
                    });
                    ui.menu_button("Help", |ui| {
                        if ui.button("About Whim").clicked() {
                            self.show_about = true;
                            ui.close();
                        }
                    });
                });
            });

        // Tab strip (24 pt, UI-SPEC D-08): visible only when a file is open.
        if let Some(doc) = &self.doc {
            let tab_title = truncate_middle(&doc.name, 30);
            let full_path = doc.path.display().to_string();
            egui::Panel::top("tabs")
                .exact_size(theme::TAB_H)
                .frame(
                    egui::Frame::NONE
                        .fill(theme::SECONDARY)
                        .inner_margin(egui::Margin::symmetric(8, 0)),
                )
                .show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        let resp = ui
                            .add(
                                egui::Label::new(
                                    RichText::new(tab_title).size(12.0).color(theme::TEXT),
                                )
                                .sense(egui::Sense::hover()),
                            )
                            .on_hover_text(full_path);
                        let r = resp.rect;
                        ui.painter().line_segment(
                            [
                                egui::pos2(r.left(), r.bottom() + 2.0),
                                egui::pos2(r.right(), r.bottom() + 2.0),
                            ],
                            Stroke::new(2.0, theme::ACCENT),
                        );
                    });
                });
        }

        // Binary-file banner (24 pt, D-10, HEX-05): never auto-switches.
        let show_binary_banner = self
            .doc
            .as_ref()
            .is_some_and(|d| d.binary && !d.binary_banner_dismissed && d.mode == ViewMode::Text);
        if show_binary_banner {
            let mut switch_hex = false;
            let mut dismiss = false;
            egui::Panel::top("binary_banner")
                .exact_size(theme::BANNER_H)
                .frame(
                    egui::Frame::NONE
                        .fill(theme::SECONDARY)
                        .inner_margin(egui::Margin::symmetric(12, 0)),
                )
                .show(ui, |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.label(
                            RichText::new("This looks like a binary file.")
                                .size(12.0)
                                .color(theme::TEXT),
                        );
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("View as hex").size(12.0).color(theme::ACCENT),
                                )
                                .frame(false),
                            )
                            .clicked()
                        {
                            switch_hex = true;
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new("\u{2715}").size(12.0).color(theme::MUTED),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                dismiss = true;
                            }
                        });
                    });
                });
            if let Some(doc) = self.doc.as_mut() {
                if switch_hex {
                    doc.toggle_mode();
                    doc.binary_banner_dismissed = true;
                } else if dismiss {
                    doc.binary_banner_dismissed = true;
                }
            }
        }

        // Status bar (24 pt, D-16).
        let (has_doc, line1, line_exact, col1, offset, len, encoding, eol, indexing_pct) =
            if let Some(doc) = &self.doc {
                let off = doc.current_top();
                let (ln0, ex) = doc
                    .index
                    .line_of(&doc.source, off)
                    .unwrap_or((0, Exactness::Estimated));
                let pct = if doc.index_job.is_some() && !doc.index.is_complete() {
                    Some(doc.index.progress_pct())
                } else {
                    None
                };
                let enc = if doc.utf16_bom { "UTF-16" } else { "UTF-8" };
                (
                    true,
                    ln0.saturating_add(1),
                    ex,
                    1u64,
                    off,
                    doc.len(),
                    enc,
                    doc.eol,
                    pct,
                )
            } else {
                (
                    false,
                    1,
                    Exactness::Exact,
                    1,
                    0,
                    0,
                    "UTF-8",
                    EolKind::Unknown,
                    None,
                )
            };

        let status_action = egui::Panel::bottom("status")
            .exact_size(24.0)
            .frame(
                egui::Frame::NONE
                    .fill(theme::SECONDARY)
                    .inner_margin(egui::Margin::symmetric(8, 0)),
            )
            .show(ui, |ui| {
                let scx = StatusCtx {
                    has_doc,
                    line1,
                    line_exact,
                    col1,
                    offset,
                    len,
                    encoding,
                    eol,
                    indexing_pct,
                    copying_text: self.copying_label.as_deref(),
                    transient: self.status.transient.as_ref(),
                    bench: self.bench,
                    startup_ms: self.clock.startup_ms(),
                };
                status::render_status_bar(ui, &scx)
            })
            .inner;

        if let Some(doc) = self.doc.as_mut() {
            if status_action.cancel_indexing {
                doc.cancel_index();
            }
            if status_action.cancel_copy {
                doc.cancel_copy();
                self.copying_label = None;
            }
        }

        // Central panel: TextView / HexView / error / empty state.
        let keys_blocked = self.show_about || self.goto_visible || self.copy_dialog.shown();
        let mut view_error: Option<String> = None;
        let mut font_notice: Option<&'static str> = None;

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if let Some(doc) = self.doc.as_mut() {
                    match doc.mode {
                        ViewMode::Text => {
                            let cx = TextViewCtx {
                                source: &doc.source,
                                index: &doc.index,
                                frozen: doc.frozen,
                                keys_blocked,
                            };
                            let out = doc.text_view.show(ui, &cx, &mut doc.sel);
                            if let Some(err) = out.error {
                                view_error = Some(err.to_string());
                            }
                            if let Some(n) = out.notice {
                                font_notice = Some(n);
                            }
                        }
                        ViewMode::Hex => {
                            doc.hex_view.frozen = doc.frozen;
                            doc.hex_view.keys_blocked = keys_blocked;
                            let _ = doc.hex_view.show(ui, &doc.source, &mut doc.sel);
                            if let Some(err) = doc.hex_view.last_error.take() {
                                view_error = Some(err.to_string());
                            }
                        }
                    }
                } else if let Some(err) = &self.open_error {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new(err).size(14.0).color(theme::DESTRUCTIVE));
                    });
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() / 2.0 - 20.0).max(0.0));
                        ui.label(RichText::new("No file open").size(14.0).color(theme::TEXT));
                        ui.label(
                            RichText::new(
                                "Drop a file here, or choose File > Open to view a file.",
                            )
                            .size(14.0)
                            .color(theme::MUTED),
                        );
                    });
                }
            });

        if let Some(err) = view_error {
            if let Some(doc) = self.doc.as_mut() {
                doc.frozen = true;
            }
            self.status.show_error(err);
        }
        if let Some(n) = font_notice {
            self.status.show(n, Some(NOTICE_TTL));
            ctx.request_repaint_after(NOTICE_TTL);
        }

        // Go-to box (Ctrl+G, D-11).
        if self.goto_visible
            && let Some(doc) = self.doc.as_mut()
        {
            let hex_mode = doc.mode == ViewMode::Hex;
            let total_lines = doc.index.total_lines();
            let doc_len = doc.len();
            let mut commit_jump = false;
            let mut close_goto = false;

            egui::Window::new("Go to")
                .title_bar(false)
                .collapsible(false)
                .resizable(false)
                .fixed_size(vec2(320.0, 0.0))
                .anchor(Align2::CENTER_TOP, vec2(0.0, 56.0))
                .frame(
                    egui::Frame::window(&ctx.global_style())
                        .fill(theme::SECONDARY)
                        .stroke(Stroke::new(1.0, theme::SEPARATOR))
                        .inner_margin(egui::Margin::same(10)),
                )
                .show(&ctx, |ui| {
                    let resp = ui.add_sized(
                        vec2(300.0, 24.0),
                        egui::TextEdit::singleline(&mut self.goto_input)
                            .font(egui::TextStyle::Monospace)
                            .hint_text(goto::hint(hex_mode)),
                    );
                    if self.goto_just_opened {
                        resp.request_focus();
                        self.goto_just_opened = false;
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        commit_jump = true;
                    }
                    let prev = goto::preview(&self.goto_input, hex_mode, doc_len, total_lines);
                    let color = if prev.valid || self.goto_input.trim().is_empty() {
                        theme::MUTED
                    } else {
                        theme::DESTRUCTIVE
                    };
                    let lbl = if self.goto_input.trim().is_empty() {
                        goto::hint(hex_mode).to_owned()
                    } else {
                        prev.text
                    };
                    ui.label(RichText::new(lbl).size(12.0).color(color));
                });

            if commit_jump && let Ok(target) = goto::parse_goto(&self.goto_input, hex_mode) {
                if let Ok(resolved) = goto::resolve(target, &doc.source, &doc.index) {
                    match doc.mode {
                        ViewMode::Text => {
                            let _ = doc.text_view.jump_to(&doc.source, resolved.offset);
                        }
                        ViewMode::Hex => {
                            doc.hex_view.jump_to(resolved.offset);
                        }
                    }
                    // Q8: if line jump happened before indexing completed, auto-re-anchor on Done.
                    if matches!(target, goto::GotoTarget::Line(_)) && !doc.index.is_complete() {
                        doc.pending_jump = Some(PendingJump {
                            target,
                            saved_scroll_epoch: doc.text_view.scroll_epoch,
                        });
                    } else {
                        doc.pending_jump = None;
                    }
                }
                close_goto = true;
            }
            if close_goto {
                self.goto_visible = false;
            }
        }

        // Copy-size confirmation / hard-cap dialog (D-14).
        if let Some(res) = confirm_copy_ui(&ctx, &mut self.copy_dialog)
            && res == CopyResult::Copy
            && let Some(doc) = &self.doc
        {
            let kind = match doc.mode {
                ViewMode::Text => CopyKind::Text,
                ViewMode::Hex => CopyKind::Hex,
            };
            self.start_copy(&ctx, kind);
        }

        // About dialog.
        if self.show_about {
            let mut open = true;
            egui::Window::new("About Whim")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .show(&ctx, |ui| {
                    ui.label(format!(
                        "Whim {} \u{2014} toolkit: egui 0.37 ({})",
                        env!("CARGO_PKG_VERSION"),
                        BACKEND
                    ));
                });
            self.show_about = open;
        }
    }
}
