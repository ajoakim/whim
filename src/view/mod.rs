pub mod rows;
pub mod snap;

use crate::theme;
use eframe::egui::{self, Color32, FontId, Key, Sense};

const SCROLLBAR_W: f32 = 12.0;
const LEFT_INSET: f32 = 16.0;

/// Virtualized byte-offset view: lays out only the visible rows each frame.
pub struct ViewState {
    pub top: u64,
    scroll_accum: f32,
    visible_rows: usize,
}

impl Default for ViewState {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewState {
    pub fn new() -> Self {
        Self { top: 0, scroll_accum: 0.0, visible_rows: 1 }
    }

    pub fn top(&self) -> u64 {
        self.top
    }

    pub fn show(&mut self, ui: &mut egui::Ui, data: &[u8]) -> egui::Response {
        let len = data.len() as u64;
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::DOMINANT);

        let track = egui::Rect::from_min_max(egui::pos2(rect.right() - SCROLLBAR_W, rect.top()), rect.max);
        let text_rect = egui::Rect::from_min_max(rect.min, egui::pos2(track.left(), rect.bottom()));

        // Read every frame; never cache (scale-factor changes are handled by egui-winit).
        let ppp = ui.pixels_per_point();
        let n = rows::rows_for_height(text_rect.height());
        self.visible_rows = n;

        // Input.
        if response.hovered() || response.has_focus() || ui.input(|i| i.pointer.hover_pos().is_some_and(|p| rect.contains(p))) {
            let dy = ui.input(|i| i.smooth_scroll_delta.y);
            self.scroll_accum += dy;
            let whole = (self.scroll_accum / rows::ROW_HEIGHT).trunc();
            if whole != 0.0 {
                self.scroll_accum -= whole * rows::ROW_HEIGHT;
                self.top = rows::scroll_rows(data, self.top, -(whole as i64));
            }
            let page = (n as i64 - 2).max(1);
            let (mut delta, mut home, mut end) = (0i64, false, false);
            ui.input(|i| {
                if i.key_pressed(Key::ArrowDown) {
                    delta += 1;
                }
                if i.key_pressed(Key::ArrowUp) {
                    delta -= 1;
                }
                if i.key_pressed(Key::PageDown) {
                    delta += page;
                }
                if i.key_pressed(Key::PageUp) {
                    delta -= page;
                }
                home = i.key_pressed(Key::Home);
                end = i.key_pressed(Key::End);
            });
            if delta != 0 {
                self.top = rows::scroll_rows(data, self.top, delta);
            }
            if home {
                self.top = 0;
            }
            if end {
                self.top = rows::clamp_offset(len.saturating_sub((n as u64) * rows::BYTES_PER_ROW), len);
            }
        }

        // Scrollbar interaction.
        let sb = ui.interact(track, response.id.with("sb"), Sense::click_and_drag());
        let visible_bytes = n as u64 * rows::BYTES_PER_ROW;
        if (sb.dragged() || sb.clicked())
            && let Some(p) = sb.interact_pointer_pos()
        {
            let (_, h) = rows::thumb_geometry(self.top, len, visible_bytes, track.height());
            self.top = rows::offset_for_thumb(p.y - track.top() - h / 2.0, track.height(), h, len);
        }

        // Paint visible rows only.
        let font = FontId::monospace(14.0);
        for (i, range) in rows::visible_rows(data, self.top, n).into_iter().enumerate() {
            let y = snap::row_y(text_rect.top(), i, rows::ROW_HEIGHT, ppp);
            let (Ok(s), Ok(e)) = (usize::try_from(range.start), usize::try_from(range.end)) else {
                continue;
            };
            let gx = snap::snap(text_rect.left() + LEFT_INSET, ppp);
            let gutter = painter.layout_no_wrap(rows::format_gutter(range.start), font.clone(), theme::MUTED);
            let tx = snap::snap(gx + gutter.size().x + LEFT_INSET, ppp);
            painter.galley(egui::pos2(gx, y), gutter, theme::MUTED);
            let text = painter.layout_no_wrap(rows::display_text(&data[s..e]), font.clone(), theme::TEXT);
            painter.galley(egui::pos2(tx, y), text, theme::TEXT);
        }

        // Scrollbar paint.
        painter.rect_filled(track, 0.0, theme::SECONDARY);
        let (ty, th) = rows::thumb_geometry(self.top, len, visible_bytes, track.height());
        let thumb = egui::Rect::from_min_size(
            egui::pos2(track.left(), track.top() + ty),
            egui::vec2(SCROLLBAR_W, th.max(rows::MIN_THUMB)),
        );
        painter.rect_filled(thumb, 0.0, Color32::from_rgb(0x8B, 0x8F, 0x98));

        response
    }
}
