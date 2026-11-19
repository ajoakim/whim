//! Read-only hex view: virtualized 16-byte rows with offset, hex and ASCII columns,
//! cell-arithmetic hit testing, selection painting and byte-preserving switch (HEX-01, HEX-03, D-09).

use crate::copy::{CopyKind, hex_text};
use crate::scroll;
use crate::selection::Selection;
use crate::source::{SourceError, TextSource, read_clamped};
use crate::text::NON_PRINTABLE;
use crate::theme;
use crate::view::scrollbar::{self, Axis, HBAR_H, VBAR_W};
use crate::view::snap;
use eframe::egui::{self, Align2, FontId, Key, Rect, Sense, Stroke, pos2, vec2};

pub const BYTES_PER_ROW: u64 = 16;
const LEFT_PAD: f32 = 16.0;
const OFFSET_HEX_GAP: f32 = 16.0;
const GROUP_GAP: f32 = 24.0;
const HEX_ASCII_GAP: f32 = 16.0;
const ASCII_LEFT_PAD: f32 = 12.0;
const RIGHT_PAD: f32 = 16.0;

#[derive(Clone, Copy, Debug)]
pub struct HexColumns {
    pub cell_w: f32,
    pub offset_x: f32,
    pub hex_g0_x: f32,
    pub hex_g1_x: f32,
    pub sep_x: f32,
    pub ascii_x: f32,
    pub total_w: f32,
}

impl HexColumns {
    pub fn compute(origin_x: f32, cell_w: f32) -> Self {
        let cell_w = cell_w.max(1.0);
        let offset_x = origin_x + LEFT_PAD;
        let offset_w = 16.0 * cell_w;
        let hex_g0_x = offset_x + offset_w + OFFSET_HEX_GAP;
        // Each group of 8 bytes: 8 * 2 hex digits + 7 single-cell spaces = 23 cells.
        let group_w = 23.0 * cell_w;
        let hex_g1_x = hex_g0_x + group_w + GROUP_GAP;
        let sep_x = hex_g1_x + group_w + HEX_ASCII_GAP;
        let ascii_x = sep_x + 1.0 + ASCII_LEFT_PAD;
        let total_w = (ascii_x - origin_x) + 16.0 * cell_w + RIGHT_PAD;
        Self {
            cell_w,
            offset_x,
            hex_g0_x,
            hex_g1_x,
            sep_x,
            ascii_x,
            total_w,
        }
    }

    /// X start of byte `col` (`0..16`) in the hex column.
    pub fn hex_byte_x(&self, col: usize) -> f32 {
        if col < 8 {
            self.hex_g0_x + (col as f32) * 3.0 * self.cell_w
        } else {
            self.hex_g1_x + ((col - 8) as f32) * 3.0 * self.cell_w
        }
    }

    /// X start of byte `col` (`0..16`) in the ASCII column.
    pub fn ascii_byte_x(&self, col: usize) -> f32 {
        self.ascii_x + (col as f32) * self.cell_w
    }

    /// Pure cell-arithmetic hit test: maps `(x, row_idx)` to a byte offset clamped to `row_len`.
    pub fn hit_test_byte(&self, x: f32, row_start: u64, row_len: usize) -> u64 {
        if row_len == 0 {
            return row_start;
        }
        let max_col = row_len - 1;
        let col = if x >= self.ascii_x {
            let c = ((x - self.ascii_x) / self.cell_w).floor() as isize;
            c.clamp(0, max_col as isize) as usize
        } else if x >= self.hex_g1_x - GROUP_GAP * 0.5 {
            let c = ((x - self.hex_g1_x) / (3.0 * self.cell_w)).floor() as isize;
            (8 + c.clamp(0, 7)) as usize
        } else if x >= self.hex_g0_x {
            let c = ((x - self.hex_g0_x) / (3.0 * self.cell_w)).floor() as isize;
            c.clamp(0, 7) as usize
        } else {
            0
        };
        row_start.saturating_add(col.min(max_col) as u64)
    }
}

/// Formats a 16-byte (or shorter) hex row into `(offset_16_hex, group0, group1, ascii_chars)`.
pub fn format_hex_row(row_start: u64, bytes: &[u8]) -> (String, String, String, Vec<(char, bool)>) {
    let offset = format!("{row_start:016X}");
    let g0_len = bytes.len().min(8);
    let mut g0 = String::with_capacity(23);
    hex_text(&bytes[..g0_len], false, &mut g0);
    let mut g1 = String::with_capacity(23);
    if bytes.len() > 8 {
        hex_text(&bytes[8..bytes.len().min(16)], false, &mut g1);
    }
    let mut ascii = Vec::with_capacity(bytes.len().min(16));
    for &b in bytes.iter().take(16) {
        if (0x20..=0x7E).contains(&b) {
            ascii.push((b as char, false));
        } else {
            ascii.push((NON_PRINTABLE, true));
        }
    }
    (offset, g0, g1, ascii)
}

pub fn align_row(off: u64) -> u64 {
    off & !15
}

pub struct HexView {
    anchor: u64,
    sub_px: f32,
    hscroll_px: f32,
    max_row_w: f32,
    scroll_epoch: u64,
    focus_requested: bool,
    last_good_anchor: u64,
    last_good_bytes: Vec<u8>,
    pub keys_blocked: bool,
    pub frozen: bool,
    pub last_error: Option<SourceError>,
}

impl Default for HexView {
    fn default() -> Self {
        Self::new()
    }
}

impl HexView {
    pub fn new() -> Self {
        Self {
            anchor: 0,
            sub_px: 0.0,
            hscroll_px: 0.0,
            max_row_w: 0.0,
            scroll_epoch: 0,
            focus_requested: false,
            last_good_anchor: 0,
            last_good_bytes: Vec::new(),
            keys_blocked: false,
            frozen: false,
            last_error: None,
        }
    }

    pub fn reset(&mut self) {
        self.anchor = 0;
        self.sub_px = 0.0;
        self.hscroll_px = 0.0;
        self.max_row_w = 0.0;
        self.focus_requested = false;
        self.last_good_anchor = 0;
        self.last_good_bytes.clear();
        self.last_error = None;
    }

    pub fn top(&self) -> u64 {
        self.anchor
    }

    /// Position-preserving switch from text view -> hex view (D-09).
    pub fn set_top_from_text(&mut self, text_top: u64) {
        self.anchor = align_row(text_top);
        debug_assert_eq!(self.anchor % 16, 0);
        self.sub_px = 0.0;
    }

    /// Byte position to pass back when switching hex view -> text view (D-09).
    pub fn top_to_text(&self) -> u64 {
        self.anchor
    }

    /// Snapped text segment start corresponding to this hex view's anchor (D-09).
    pub fn top_to_text_segment(&self, src: &dyn TextSource) -> Result<u64, SourceError> {
        let len = src.len();
        if len == 0 {
            return Ok(0);
        }
        scroll::segment_start(src, self.anchor.min(len - 1))
    }

    /// Jumps to `off` aligned down to a 16-byte row boundary; returns the snapped offset.
    pub fn jump_to(&mut self, off: u64) -> u64 {
        self.anchor = align_row(off);
        debug_assert_eq!(self.anchor % 16, 0);
        self.sub_px = 0.0;
        self.anchor
    }

    pub fn scroll_epoch(&self) -> u64 {
        self.scroll_epoch
    }

    /// Helper exposing the copy kind for callers wiring Ctrl+C.
    pub fn copy_kind() -> CopyKind {
        CopyKind::Hex
    }

    fn bump_scroll(&mut self) {
        self.scroll_epoch = self.scroll_epoch.wrapping_add(1);
    }

    fn clamp_anchor(&mut self, len: u64) {
        if len == 0 {
            self.anchor = 0;
        } else {
            let max_row = align_row(len - 1);
            self.anchor = align_row(self.anchor).min(max_row);
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        source: &dyn TextSource,
        sel: &mut Selection,
    ) -> egui::Response {
        self.last_error = None;
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::DOMINANT);

        let len = source.len();
        if len == 0 && !self.frozen {
            self.anchor = 0;
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "This file is empty.",
                FontId::proportional(theme::MONO_SIZE),
                theme::MUTED,
            );
            return response;
        }

        let ppp = ui.pixels_per_point();
        let font = FontId::monospace(theme::MONO_SIZE);
        let cell_w = ui.fonts_mut(|f| f.glyph_width(&font, '0')).max(1.0);
        let cols_base = HexColumns::compute(rect.left(), cell_w);
        if cols_base.total_w > self.max_row_w {
            self.max_row_w = cols_base.total_w;
        }

        let prelim_w = (rect.width() - VBAR_W).max(16.0);
        let show_hbar = self.max_row_w > prelim_w;
        let vbar_bottom = if show_hbar {
            rect.bottom() - HBAR_H
        } else {
            rect.bottom()
        };
        let vtrack = Rect::from_min_max(
            pos2(rect.right() - VBAR_W, rect.top()),
            pos2(rect.right(), vbar_bottom),
        );
        let content_rect = Rect::from_min_max(rect.min, pos2(vtrack.left(), vbar_bottom));
        let content_w = content_rect.width().max(16.0);
        let visible_rows =
            ((content_rect.height().max(0.0) / theme::ROW_HEIGHT).ceil() as usize).max(1);

        if self.frozen {
            let mut dim_painter = ui.painter_at(content_rect);
            dim_painter.set_opacity(0.4);
            let cols = HexColumns::compute(content_rect.left() - self.hscroll_px, cell_w);
            paint_hex_rows(
                &dim_painter,
                content_rect,
                &cols,
                self.last_good_anchor,
                &self.last_good_bytes,
                sel,
                false,
                ppp,
                &font,
            );
            return response;
        }

        self.clamp_anchor(len);

        if response.clicked() || !self.focus_requested {
            response.request_focus();
            self.focus_requested = true;
        }
        let focused = response.has_focus();

        // Wheel scroll.
        if response.hovered()
            || focused
            || ui.input(|i| i.pointer.hover_pos().is_some_and(|p| rect.contains(p)))
        {
            let (dx, dy, shift) = ui.input(|i| {
                (
                    i.smooth_scroll_delta.x,
                    i.smooth_scroll_delta.y,
                    i.modifiers.shift,
                )
            });
            let horiz = if dx != 0.0 {
                -dx
            } else if shift && dy != 0.0 {
                -dy
            } else {
                0.0
            };
            if horiz != 0.0 {
                let max_h = (self.max_row_w - content_w).max(0.0);
                self.hscroll_px = (self.hscroll_px + horiz).clamp(0.0, max_h);
            }
            if !shift && dy != 0.0 {
                self.sub_px += dy;
                let whole = (self.sub_px / theme::ROW_HEIGHT).trunc();
                if whole != 0.0 {
                    self.sub_px -= whole * theme::ROW_HEIGHT;
                    self.step_rows(-(whole as i64), len);
                }
            }
        }

        // Keyboard navigation & selection.
        if focused && !self.keys_blocked && !ui.ctx().any_popup_open() {
            let page = (visible_rows as i64 - 1).max(1);
            ui.input(|i| {
                let cmd = i.modifiers.command;
                let shift = i.modifiers.shift;
                if cmd && i.key_pressed(Key::A) {
                    *sel = Selection::select_all(len);
                    return;
                }
                if i.key_pressed(Key::ArrowDown) {
                    self.step_rows(1, len);
                }
                if i.key_pressed(Key::ArrowUp) {
                    self.step_rows(-1, len);
                }
                if i.key_pressed(Key::PageDown) {
                    self.step_rows(page, len);
                }
                if i.key_pressed(Key::PageUp) {
                    self.step_rows(-page, len);
                }
                if cmd && i.key_pressed(Key::Home) {
                    self.anchor = 0;
                    self.sub_px = 0.0;
                    self.bump_scroll();
                    if shift {
                        sel.extend_to(0);
                    } else {
                        *sel = Selection::caret(0);
                    }
                }
                if cmd && i.key_pressed(Key::End) {
                    let last_row = align_row(len - 1);
                    let back = (visible_rows.saturating_sub(1) as u64).saturating_mul(BYTES_PER_ROW);
                    self.anchor = last_row.saturating_sub(back);
                    self.sub_px = 0.0;
                    self.bump_scroll();
                    if shift {
                        sel.extend_to(len);
                    } else {
                        *sel = Selection::caret(len);
                    }
                }
                if i.key_pressed(Key::ArrowRight) {
                    let next = sel.head.saturating_add(1).min(len);
                    if shift {
                        sel.extend_to(next);
                    } else {
                        *sel = Selection::caret(next);
                    }
                }
                if i.key_pressed(Key::ArrowLeft) {
                    let prev = sel.head.saturating_sub(1);
                    if shift {
                        sel.extend_to(prev);
                    } else {
                        *sel = Selection::caret(prev);
                    }
                }
            });
        }

        // Read visible bytes (16 * (visible_rows + 1)).
        let want_bytes = (visible_rows + 1).saturating_mul(BYTES_PER_ROW as usize);
        match read_clamped(source, self.anchor, want_bytes) {
            Ok(bytes) => {
                self.last_good_anchor = self.anchor;
                self.last_good_bytes = bytes;
            }
            Err(e) => {
                self.last_error = Some(e);
            }
        }

        let cols = HexColumns::compute(content_rect.left() - self.hscroll_px, cell_w);

        // Pointer selection (single click, drag, Shift+click, double-click = 1 byte, triple-click = 16-byte row).
        if let Some(pos) = response.interact_pointer_pos()
            && content_rect.contains(pos)
        {
            let row_idx = ((pos.y - content_rect.top()).max(0.0) / theme::ROW_HEIGHT).floor() as u64;
            let row_start = self
                .last_good_anchor
                .saturating_add(row_idx.saturating_mul(BYTES_PER_ROW));
            if row_start < len {
                let row_len = (len - row_start).min(BYTES_PER_ROW) as usize;
                let byte_off = cols.hit_test_byte(pos.x, row_start, row_len);
                let shift = ui.input(|i| i.modifiers.shift);
                if response.triple_clicked() {
                    sel.anchor = row_start;
                    sel.head = row_start.saturating_add(row_len as u64);
                } else if response.double_clicked() {
                    sel.anchor = byte_off;
                    sel.head = byte_off.saturating_add(1).min(len);
                } else if response.drag_started() || response.clicked() {
                    if shift {
                        sel.extend_to(byte_off.saturating_add(1).min(len));
                    } else {
                        *sel = Selection::caret(byte_off);
                    }
                } else if response.dragged() {
                    let end = if byte_off >= sel.anchor {
                        byte_off.saturating_add(1).min(len)
                    } else {
                        byte_off
                    };
                    sel.extend_to(end);
                }
            }
        }

        // Paint rows.
        let content_painter = ui.painter_at(content_rect);
        paint_hex_rows(
            &content_painter,
            content_rect,
            &cols,
            self.last_good_anchor,
            &self.last_good_bytes,
            sel,
            focused,
            ppp,
            &font,
        );

        // Vertical scrollbar (exact proportional, f64).
        let visible_bytes = (visible_rows as u64).saturating_mul(BYTES_PER_ROW);
        let start_frac = (self.anchor as f64 / len as f64).clamp(0.0, 1.0);
        let vis_frac = (visible_bytes as f64 / len as f64).clamp(0.0, 1.0);
        if let Some(new_frac) = scrollbar::scrollbar(
            ui,
            vtrack,
            response.id.with("hex_vsb"),
            Axis::Vertical,
            start_frac,
            vis_frac,
        ) {
            let max_row = align_row(len - 1);
            let target = align_row((new_frac * max_row as f64).round() as u64).min(max_row);
            if target != self.anchor {
                self.anchor = target;
                self.sub_px = 0.0;
                self.bump_scroll();
            }
        }

        // Horizontal scrollbar if narrower than total columns.
        if show_hbar {
            let htrack = Rect::from_min_max(
                pos2(rect.left(), vbar_bottom),
                pos2(vtrack.left(), rect.bottom()),
            );
            let max_h = (self.max_row_w - content_w).max(1.0);
            let h_start = f64::from(self.hscroll_px / max_h);
            let h_vis = f64::from((content_w / self.max_row_w).min(1.0));
            if let Some(new_h) = scrollbar::scrollbar(
                ui,
                htrack,
                response.id.with("hex_hsb"),
                Axis::Horizontal,
                h_start,
                h_vis,
            ) {
                self.hscroll_px = (new_h as f32 * max_h).clamp(0.0, max_h);
            }
        }

        response
    }

    fn step_rows(&mut self, delta: i64, len: u64) {
        if len == 0 || delta == 0 {
            return;
        }
        let max_row = align_row(len - 1);
        let next = if delta > 0 {
            self.anchor
                .saturating_add((delta as u64).saturating_mul(BYTES_PER_ROW))
                .min(max_row)
        } else {
            self.anchor
                .saturating_sub(delta.unsigned_abs().saturating_mul(BYTES_PER_ROW))
        };
        let aligned = align_row(next);
        if aligned != self.anchor {
            self.anchor = aligned;
            self.bump_scroll();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_hex_rows(
    painter: &egui::Painter,
    content_rect: Rect,
    cols: &HexColumns,
    base_anchor: u64,
    bytes: &[u8],
    sel: &Selection,
    focused: bool,
    ppp: f32,
    font: &FontId,
) {
    // Vertical separator before ASCII column.
    let sep_x = snap::snap(cols.sep_x + 0.5, ppp);
    painter.line_segment(
        [
            pos2(sep_x, content_rect.top()),
            pos2(sep_x, content_rect.bottom()),
        ],
        Stroke::new(1.0, theme::SEPARATOR),
    );

    let sel_range = sel.range();
    let sel_fill = if focused {
        theme::SELECTION
    } else {
        theme::SELECTION_INACTIVE
    };
    let half_gap = cols.cell_w * 0.5;

    for (row_i, chunk) in bytes.chunks(BYTES_PER_ROW as usize).enumerate() {
        let y = snap::row_y(content_rect.top(), row_i, theme::ROW_HEIGHT, ppp);
        if y >= content_rect.bottom() {
            break;
        }
        let row_start = base_anchor.saturating_add((row_i as u64).saturating_mul(BYTES_PER_ROW));
        let row_end = row_start.saturating_add(chunk.len() as u64);

        // Paint selection highlights in both hex and ASCII columns before text.
        if !sel_range.is_empty() && sel_range.start < row_end && sel_range.end > row_start {
            let lo = (sel_range.start.max(row_start) - row_start) as usize;
            let hi = (sel_range.end.min(row_end) - row_start) as usize;
            for col in lo..hi {
                let hx = cols.hex_byte_x(col);
                let hrect = Rect::from_min_size(
                    pos2(hx - half_gap * 0.5, y),
                    vec2(2.0 * cols.cell_w + half_gap, theme::ROW_HEIGHT),
                );
                painter.rect_filled(hrect, 0.0, sel_fill);

                let ax = cols.ascii_byte_x(col);
                let arect = Rect::from_min_size(
                    pos2(ax, y),
                    vec2(cols.cell_w, theme::ROW_HEIGHT),
                );
                painter.rect_filled(arect, 0.0, sel_fill);
            }
        }

        let (off_s, g0, g1, ascii) = format_hex_row(row_start, chunk);
        let cy = y + theme::ROW_HEIGHT * 0.5;
        painter.text(
            pos2(snap::snap(cols.offset_x, ppp), cy),
            Align2::LEFT_CENTER,
            off_s,
            font.clone(),
            theme::MUTED,
        );
        if !g0.is_empty() {
            painter.text(
                pos2(snap::snap(cols.hex_g0_x, ppp), cy),
                Align2::LEFT_CENTER,
                g0,
                font.clone(),
                theme::TEXT,
            );
        }
        if !g1.is_empty() {
            painter.text(
                pos2(snap::snap(cols.hex_g1_x, ppp), cy),
                Align2::LEFT_CENTER,
                g1,
                font.clone(),
                theme::TEXT,
            );
        }
        for (i, (ch, is_dim)) in ascii.into_iter().enumerate() {
            let color = if is_dim { theme::MUTED_DIM } else { theme::TEXT };
            painter.text(
                pos2(snap::snap(cols.ascii_byte_x(i), ppp), cy),
                Align2::LEFT_CENTER,
                ch.to_string(),
                font.clone(),
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemSource;

    #[test]
    fn hex_row_formatting() {
        let bytes: Vec<u8> = (0x18..0x28).collect();
        let (off, g0, g1, ascii) = format_hex_row(0x100, &bytes);
        assert_eq!(off, "0000000000000100");
        assert_eq!(g0, "18 19 1A 1B 1C 1D 1E 1F");
        assert_eq!(g1, "20 21 22 23 24 25 26 27");
        assert_eq!(ascii.len(), 16);
        assert!(ascii[0].1); // 0x18 is non-printable
        assert_eq!(ascii[0].0, NON_PRINTABLE);
        assert!(!ascii[8].1); // 0x20 is space (printable)
        assert_eq!(ascii[8].0, ' ');
        assert!(!ascii[15].1);
        assert_eq!(ascii[15].0, '\'');
    }

    #[test]
    fn jump_to_and_switch_preserve_alignment() {
        let mut hv = HexView::new();
        assert_eq!(hv.jump_to(0x37), 0x30);
        assert_eq!(hv.top() % 16, 0);

        hv.set_top_from_text(0x49);
        assert_eq!(hv.top(), 0x40);
        assert_eq!(hv.top_to_text(), 0x40);

        let src = MemSource::new(b"hello\nworld\n0123456789abcdef\nmore".to_vec());
        hv.set_top_from_text(14);
        assert_eq!(hv.top(), 0);
        assert_eq!(hv.top_to_text_segment(&src).unwrap(), 0);

        hv.set_top_from_text(20);
        assert_eq!(hv.top(), 16);
        assert_eq!(hv.top_to_text_segment(&src).unwrap(), 12);
    }

    #[test]
    fn hit_test_cell_arithmetic() {
        let cols = HexColumns::compute(0.0, 8.0);
        // Group 0 byte 3
        let x3 = cols.hex_byte_x(3) + 4.0;
        assert_eq!(cols.hit_test_byte(x3, 0x20, 16), 0x23);
        // Group 1 byte 10
        let x10 = cols.hex_byte_x(10) + 4.0;
        assert_eq!(cols.hit_test_byte(x10, 0x20, 16), 0x2A);
        // ASCII cell 15
        let xa = cols.ascii_byte_x(15) + 2.0;
        assert_eq!(cols.hit_test_byte(xa, 0x20, 16), 0x2F);
        // Clamped on short final row (row_len = 5)
        assert_eq!(cols.hit_test_byte(xa, 0x20, 5), 0x24);
    }

    #[test]
    fn scrollbar_proportional_math() {
        let len = 16_000u64;
        let anchor = 8_000u64;
        let vis = 320u64;
        let (t_start, t_len) = scrollbar::thumb(anchor as f64 / len as f64, vis as f64 / len as f64, 400.0);
        let back = scrollbar::frac_for_thumb(t_start, t_len, 400.0);
        assert!((back - 0.5).abs() < 1e-4);
    }

    #[test]
    fn hex_copy_text_uppercase_pairs() {
        let mut s = String::new();
        hex_text(&[0x4C, 0x9A, 0xFF], false, &mut s);
        assert_eq!(s, "4C 9A FF");
        assert_eq!(HexView::copy_kind(), CopyKind::Hex);
    }

    #[test]
    fn egui_smoke_hex_and_empty() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(900.0, 600.0))),
            ..Default::default()
        };

        let src = MemSource::new((0..2048).map(|i| (i & 0xFF) as u8).collect());
        let mut hv = HexView::new();
        let mut sel = Selection { anchor: 4, head: 22 };
        for _ in 0..2 {
            let mut full = ctx.run_ui(input.clone(), |ui| {
                let _ = hv.show(ui, &src, &mut sel);
            });
            full.textures_delta.clear();
        }
        assert!(hv.last_error.is_none());
        assert_eq!(hv.top(), 0);

        let empty = MemSource::new(Vec::new());
        let mut hv_e = HexView::new();
        let mut sel_e = Selection::caret(0);
        let mut full = ctx.run_ui(input, |ui| {
            let _ = hv_e.show(ui, &empty, &mut sel_e);
        });
        full.textures_delta.clear();
        assert!(hv_e.last_error.is_none());
    }
}
