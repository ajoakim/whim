//! Virtualized real-line text view (CORE-03, CORE-06, EDIT-05, EDIT-06, UI-05).
//!
//! Lays out only segments overlapping the visible viewport (+1), supports soft wrap and
//! horizontal scroll (D-05, D-20), right-aligned exact/estimated line-number gutter (D-06),
//! lazy system font fallback (D-07), frozen dimmed rendering on read errors (D-01), and
//! Phase 1's IME composition probe. Selection/caret interaction is extended in plan 02-09.

use crate::fonts::FontFallback;
use crate::ime::ImeProbe;
use crate::lineindex::{Exactness, LineIndex};
use crate::scroll::{self, Segment};
use crate::selection::{self, Selection};
use crate::source::{SourceError, TextSource, read_clamped};
use crate::text::{self, Decoded, decode_segment};
use crate::theme;
use crate::view::scrollbar::{self, Axis, HBAR_H, VBAR_W};
use crate::view::snap;
use eframe::egui::text::{LayoutJob, TextFormat, TextWrapping};
use eframe::egui::{self, Align2, FontId, Galley, Key, Rect, RichText, Sense, Stroke, pos2, vec2};
use std::collections::HashMap;
use std::sync::Arc;

const MAX_CACHE_ENTRIES: usize = 128;
const GUTTER_LEFT_PAD: f32 = 16.0;
const GUTTER_RIGHT_PAD: f32 = 8.0;
const TEXT_LEFT_PAD_WITH_GUTTER: f32 = 8.0;
const TEXT_LEFT_PAD_NO_GUTTER: f32 = 16.0;

// ---------------------------------------------------------------------------
// Pure row model (egui-free, unit-tested independently)
// ---------------------------------------------------------------------------

/// One visual row inside a decoded segment: display-char range `[first_char, first_char + chars)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RowSpan {
    pub first_char: usize,
    pub chars: usize,
}

pub(crate) trait RowLayouter {
    /// Returns at least one `RowSpan` (an empty segment yields one empty row).
    fn rows(&mut self, seg: &Segment, decoded: &Decoded) -> Vec<RowSpan>;
}

#[cfg(test)]
pub(crate) struct MonoLayouter {
    pub wrap_at: Option<usize>,
}

#[cfg(test)]
impl RowLayouter for MonoLayouter {
    fn rows(&mut self, _seg: &Segment, decoded: &Decoded) -> Vec<RowSpan> {
        let n = decoded.char_count();
        let Some(w) = self.wrap_at else {
            return vec![RowSpan {
                first_char: 0,
                chars: n,
            }];
        };
        let w = w.max(1);
        if n == 0 {
            return vec![RowSpan {
                first_char: 0,
                chars: 0,
            }];
        }
        let mut out = Vec::with_capacity(n.div_ceil(w));
        let mut i = 0;
        while i < n {
            let take = (n - i).min(w);
            out.push(RowSpan {
                first_char: i,
                chars: take,
            });
            i += take;
        }
        out
    }
}

pub(crate) fn decode_seg_from_source(
    src: &dyn TextSource,
    seg: &Segment,
) -> Result<Decoded, SourceError> {
    let seg_len =
        usize::try_from(seg.end.saturating_sub(seg.start)).map_err(|_| SourceError::Changed)?;
    let bytes = read_clamped(src, seg.start, seg_len)?;
    let next_is_newline = if !seg.newline_end && seg.end < src.len() {
        let mut next = [0u8; 1];
        src.read_range(seg.end, &mut next)?;
        next[0] == b'\n'
    } else {
        false
    };
    Ok(decode_segment(&bytes, next_is_newline))
}

fn row_byte(seg: &Segment, decoded: &Decoded, row: RowSpan) -> u64 {
    seg.start
        .saturating_add(u64::from(decoded.byte_for_char(row.first_char)))
}

fn row_index_for_byte(seg: &Segment, decoded: &Decoded, rows: &[RowSpan], off: u64) -> usize {
    let mut best = 0usize;
    for (i, &r) in rows.iter().enumerate() {
        if row_byte(seg, decoded, r) <= off {
            best = i;
        } else {
            break;
        }
    }
    best
}

/// Start byte of the visual row containing `off`. Idempotent.
pub(crate) fn row_start_of(
    src: &dyn TextSource,
    lay: &mut dyn RowLayouter,
    off: u64,
) -> Result<u64, SourceError> {
    let len = src.len();
    if len == 0 {
        return Ok(0);
    }
    let clamped = off.min(len.saturating_sub(1));
    let seg_s = scroll::segment_start(src, clamped)?;
    let seg = scroll::segment_at(src, seg_s)?;
    let dec = decode_seg_from_source(src, &seg)?;
    let rows = lay.rows(&seg, &dec);
    let idx = row_index_for_byte(&seg, &dec, &rows, clamped);
    Ok(row_byte(&seg, &dec, rows[idx]))
}

/// Steps `delta` visual rows from `anchor` (positive = down, negative = up), clamping to `[0, last_row_start]`.
pub(crate) fn step_rows(
    src: &dyn TextSource,
    lay: &mut dyn RowLayouter,
    anchor: u64,
    delta: i64,
) -> Result<u64, SourceError> {
    let len = src.len();
    if len == 0 || delta == 0 {
        return Ok(if len == 0 { 0 } else { anchor.min(len - 1) });
    }

    let clamped = anchor.min(len - 1);
    let mut seg = scroll::segment_at(src, scroll::segment_start(src, clamped)?)?;
    let mut dec = decode_seg_from_source(src, &seg)?;
    let mut rows = lay.rows(&seg, &dec);
    let mut row_idx = row_index_for_byte(&seg, &dec, &rows, clamped);

    if delta > 0 {
        let mut rem = delta as u64;
        while rem > 0 {
            let avail_in_seg = (rows.len() - 1 - row_idx) as u64;
            if rem <= avail_in_seg {
                row_idx += rem as usize;
                break;
            }
            rem -= avail_in_seg;
            if seg.end >= len {
                row_idx = rows.len() - 1;
                break;
            }
            seg = scroll::segment_at(src, seg.end)?;
            dec = decode_seg_from_source(src, &seg)?;
            rows = lay.rows(&seg, &dec);
            row_idx = 0;
            rem -= 1;
        }
    } else {
        let mut rem = delta.unsigned_abs();
        while rem > 0 {
            if rem <= row_idx as u64 {
                row_idx -= rem as usize;
                break;
            }
            rem -= row_idx as u64;
            let Some(prev_s) = scroll::prev_segment_start(src, seg.start)? else {
                row_idx = 0;
                break;
            };
            seg = scroll::segment_at(src, prev_s)?;
            dec = decode_seg_from_source(src, &seg)?;
            rows = lay.rows(&seg, &dec);
            row_idx = rows.len() - 1;
            rem -= 1;
        }
    }

    Ok(row_byte(&seg, &dec, rows[row_idx]))
}

/// Anchor that places the last visual row of the file on the last visible line (or 0 for short files).
pub(crate) fn anchor_for_end(
    src: &dyn TextSource,
    lay: &mut dyn RowLayouter,
    visible_rows: usize,
) -> Result<u64, SourceError> {
    let len = src.len();
    if len == 0 {
        return Ok(0);
    }
    let last_seg_s = scroll::last_segment_start(src)?;
    let seg = scroll::segment_at(src, last_seg_s)?;
    let dec = decode_seg_from_source(src, &seg)?;
    let rows = lay.rows(&seg, &dec);
    let last_row_off = row_byte(&seg, &dec, rows[rows.len() - 1]);
    let back = visible_rows.saturating_sub(1) as i64;
    step_rows(src, lay, last_row_off, -back)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GutterMark {
    Number(u64, Exactness),
    Continuation,
    Blank,
}

fn decimal_digits(mut n: u64) -> usize {
    let mut d = 1;
    while n >= 10 {
        n /= 10;
        d += 1;
    }
    d
}

/// Monotone gutter cell width: at least 4 digits, plus 1 cell for `~` when estimated.
pub(crate) fn gutter_cells(prev: usize, total: (u64, Exactness)) -> usize {
    let (count, ex) = total;
    let base = decimal_digits(count.max(1)).max(4);
    let need = match ex {
        Exactness::Exact => base,
        Exactness::Estimated => base + 1,
    };
    prev.max(need)
}

pub(crate) fn gutter_mark_for_row(
    seg: &Segment,
    row_in_seg: usize,
    line1: u64,
    index: &LineIndex,
) -> GutterMark {
    if row_in_seg > 0 {
        return GutterMark::Blank;
    }
    if seg.hard_continuation {
        return GutterMark::Continuation;
    }
    let ex = if index.is_complete() || seg.start <= index.covered() {
        Exactness::Exact
    } else {
        Exactness::Estimated
    };
    GutterMark::Number(line1, ex)
}

// ---------------------------------------------------------------------------
// egui TextView
// ---------------------------------------------------------------------------

pub struct TextViewCtx<'a> {
    pub source: &'a dyn TextSource,
    pub index: &'a LineIndex,
    pub frozen: bool,
    pub keys_blocked: bool,
}

pub struct TextViewOutput {
    pub response: egui::Response,
    pub error: Option<SourceError>,
    pub top: u64,
    pub visible_bytes: u64,
    pub notice: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LayoutKey {
    content_w_bits: u32,
    wrap: bool,
    ppp_bits: u32,
    version: u64,
    len: u64,
}

#[derive(Clone)]
pub(crate) struct ParaLayout {
    pub decoded: Arc<Decoded>,
    pub galley: Arc<Galley>,
    pub rows: Vec<RowSpan>,
}

#[derive(Clone)]
pub(crate) struct VisibleRow {
    pub seg: Segment,
    pub decoded: Arc<Decoded>,
    pub galley: Arc<Galley>,
    pub row_in_seg: usize,
    pub mark: GutterMark,
}

pub struct TextView {
    pub wrap: bool,
    pub line_numbers: bool,
    pub scroll_epoch: u64,

    anchor: u64,
    sub_px: f32,
    hscroll_px: f32,
    max_row_w: f32,
    gutter_cells: usize,
    layout_key: Option<LayoutKey>,
    cache: HashMap<u64, ParaLayout>,
    cache_order: Vec<u64>,
    last_good: Vec<VisibleRow>,
    fonts: FontFallback,
    ime: ImeProbe,
    focus_requested: bool,
    visible_rows: usize,
}

impl Default for TextView {
    fn default() -> Self {
        Self::new()
    }
}

impl TextView {
    pub fn new() -> Self {
        Self {
            wrap: true,
            line_numbers: true,
            scroll_epoch: 0,
            anchor: 0,
            sub_px: 0.0,
            hscroll_px: 0.0,
            max_row_w: 0.0,
            gutter_cells: 4,
            layout_key: None,
            cache: HashMap::new(),
            cache_order: Vec::new(),
            last_good: Vec::new(),
            fonts: FontFallback::new(),
            ime: ImeProbe::new(),
            focus_requested: false,
            visible_rows: 1,
        }
    }

    pub fn reset(&mut self) {
        self.anchor = 0;
        self.sub_px = 0.0;
        self.hscroll_px = 0.0;
        self.max_row_w = 0.0;
        self.gutter_cells = 4;
        self.layout_key = None;
        self.cache.clear();
        self.cache_order.clear();
        self.last_good.clear();
        self.focus_requested = false;
    }

    pub fn top(&self) -> u64 {
        self.anchor
    }

    /// Jumps to the segment containing `off`. Does NOT bump `scroll_epoch` (Q8).
    pub fn jump_to(&mut self, src: &dyn TextSource, off: u64) -> Result<(), SourceError> {
        let len = src.len();
        self.anchor = if len == 0 {
            0
        } else {
            scroll::segment_start(src, off.min(len - 1))?
        };
        self.sub_px = 0.0;
        Ok(())
    }

    fn bump_scroll(&mut self) {
        self.increment_scroll_epoch();
    }

    /// Increment `scroll_epoch` once per user-initiated movement so 02-11 can detect
    /// post-jump scrolling (Q8).
    pub(crate) fn increment_scroll_epoch(&mut self) {
        self.scroll_epoch = self.scroll_epoch.wrapping_add(1);
    }

    /// Mouse click / press at `(x, y)` inside `text_rect`. Returns the absolute byte offset
    /// of the character nearest `(x, y)` using `Galley::cursor_from_pos` + `byte_of_char`.
    pub(crate) fn handle_click(
        &mut self,
        cx: &TextViewCtx<'_>,
        text_rect: Rect,
        pos: egui::Pos2,
    ) -> u64 {
        let len = cx.source.len();
        if len == 0 || self.last_good.is_empty() {
            return 0;
        }
        let row_idx = ((pos.y - text_rect.top()).max(0.0) / theme::ROW_HEIGHT).floor() as usize;
        let Some(row) = self.last_good.get(row_idx) else {
            return len;
        };
        let local_x = (pos.x - (text_rect.left() - self.hscroll_px)).max(0.0);
        let local_y = (row.row_in_seg as f32 + 0.5) * theme::ROW_HEIGHT;
        let cursor = row.galley.cursor_from_pos(vec2(local_x, local_y));
        let char_idx: usize = cursor.index.into();
        let rel_byte = row.decoded.byte_for_char(char_idx);
        row.seg.start.saturating_add(u64::from(rel_byte)).min(len)
    }

    /// Double-click word selection at `off`: bounded 256-byte scan via `selection::word_bounds`.
    pub(crate) fn handle_word_select(
        &mut self,
        src: &dyn TextSource,
        off: u64,
    ) -> Result<std::ops::Range<u64>, SourceError> {
        selection::word_bounds(src, off)
    }

    /// Triple-click paragraph (segment) selection containing `off`:
    /// `[segment_start(off), next_boundary(segment_start(off)))`.
    pub(crate) fn handle_triple_select(
        &mut self,
        src: &dyn TextSource,
        off: u64,
    ) -> Result<std::ops::Range<u64>, SourceError> {
        let len = src.len();
        if len == 0 {
            return Ok(0..0);
        }
        let s = scroll::segment_start(src, off.min(len - 1))?;
        let e = scroll::next_boundary(src, s)?;
        Ok(s..e)
    }

    fn invalidate_cache_if_needed(&mut self, key: LayoutKey) {
        if self.layout_key != Some(key) {
            self.cache.clear();
            self.cache_order.clear();
            self.layout_key = Some(key);
        }
    }

    fn get_or_layout(
        &mut self,
        ui: &egui::Ui,
        src: &dyn TextSource,
        seg: Segment,
        content_w: f32,
        notice: &mut Option<&'static str>,
    ) -> Result<ParaLayout, SourceError> {
        if let Some(hit) = self.cache.get(&seg.start) {
            return Ok(hit.clone());
        }

        let decoded = Arc::new(decode_seg_from_source(src, &seg)?);
        let font = FontId::monospace(theme::MONO_SIZE);
        if !decoded.text.is_ascii()
            && let Some(msg) = self.fonts.check(ui.ctx(), &font, &decoded.text)
        {
            *notice = Some(msg);
        }

        let job = build_layout_job(&decoded, font, self.wrap, content_w);
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let rows = spans_from_galley(&galley, decoded.char_count());
        let row_w = galley.size().x;
        if row_w > self.max_row_w {
            self.max_row_w = row_w;
        }

        let entry = ParaLayout {
            decoded,
            galley,
            rows,
        };
        if self.cache.len() >= MAX_CACHE_ENTRIES
            && let Some(old) = self.cache_order.first().copied()
        {
            self.cache_order.remove(0);
            self.cache.remove(&old);
        }
        self.cache.insert(seg.start, entry.clone());
        self.cache_order.push(seg.start);
        Ok(entry)
    }

    #[allow(clippy::too_many_arguments)]
    fn handle_keys(
        &mut self,
        ui: &egui::Ui,
        src: &dyn TextSource,
        content_w: f32,
        visible_rows: usize,
        composing: bool,
        sel: &mut Selection,
        notice: &mut Option<&'static str>,
    ) -> Result<(), SourceError> {
        if composing {
            return Ok(());
        }
        let len = src.len();
        let page = (visible_rows as i64 - 1).max(1);
        let (
            mut delta,
            mut ctrl_home,
            mut ctrl_end,
            mut hdelta,
            mut select_all,
            mut shift_held,
            mut home_line,
            mut end_line,
            mut move_char,
        ) = (0i64, false, false, 0.0f32, false, false, false, false, 0i64);
        ui.input(|i| {
            let cmd = i.modifiers.command;
            let shift = i.modifiers.shift;
            shift_held = shift;
            if cmd && i.key_pressed(Key::A) {
                select_all = true;
                return;
            }
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
            if i.key_pressed(Key::Home) {
                if cmd {
                    ctrl_home = true;
                } else {
                    home_line = true;
                }
            }
            if i.key_pressed(Key::End) {
                if cmd {
                    ctrl_end = true;
                } else {
                    end_line = true;
                }
            }
            if i.key_pressed(Key::ArrowRight) {
                if shift {
                    move_char += 1;
                } else if !self.wrap {
                    hdelta += 40.0;
                } else {
                    move_char += 1;
                }
            }
            if i.key_pressed(Key::ArrowLeft) {
                if shift {
                    move_char -= 1;
                } else if !self.wrap {
                    hdelta -= 40.0;
                } else {
                    move_char -= 1;
                }
            }
        });

        if select_all {
            *sel = Selection::select_all(len);
            return Ok(());
        }
        if move_char != 0 && len > 0 {
            let next = if move_char > 0 {
                sel.head.saturating_add(1).min(len)
            } else {
                sel.head.saturating_sub(1)
            };
            if shift_held {
                sel.extend_to(next);
            } else {
                *sel = Selection::caret(next);
            }
        }
        if home_line && len > 0 {
            let seg_s = scroll::segment_start(src, sel.head.min(len - 1))?;
            if shift_held {
                sel.extend_to(seg_s);
            } else {
                *sel = Selection::caret(seg_s);
            }
            if !self.wrap {
                self.hscroll_px = 0.0;
            }
        }
        if end_line && len > 0 {
            let seg_s = scroll::segment_start(src, sel.head.min(len - 1))?;
            let seg_e = scroll::next_boundary(src, seg_s)?;
            if shift_held {
                sel.extend_to(seg_e);
            } else {
                *sel = Selection::caret(seg_e);
            }
        }
        if delta != 0 {
            let cur_anchor = self.anchor;
            let mut lay = EguiLayouter {
                view: self,
                ui,
                content_w,
                notice,
            };
            let next = step_rows(src, &mut lay, cur_anchor, delta)?;
            if next != cur_anchor {
                lay.view.anchor = next;
                lay.view.sub_px = 0.0;
                lay.view.bump_scroll();
            }
            if shift_held {
                sel.extend_to(next);
            }
        }
        if ctrl_home {
            if self.anchor != 0 {
                self.anchor = 0;
                self.sub_px = 0.0;
                self.bump_scroll();
            }
            if shift_held {
                sel.extend_to(0);
            } else {
                *sel = Selection::caret(0);
            }
        }
        if ctrl_end {
            let mut lay = EguiLayouter {
                view: self,
                ui,
                content_w,
                notice,
            };
            let end_a = anchor_for_end(src, &mut lay, visible_rows)?;
            if end_a != lay.view.anchor {
                lay.view.anchor = end_a;
                lay.view.sub_px = 0.0;
                lay.view.bump_scroll();
            }
            if shift_held {
                sel.extend_to(len);
            } else {
                *sel = Selection::caret(len);
            }
        }
        if hdelta != 0.0 && !self.wrap {
            let max_h = (self.max_row_w - content_w).max(0.0);
            self.hscroll_px = (self.hscroll_px + hdelta).clamp(0.0, max_h);
        }
        Ok(())
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        cx: &TextViewCtx<'_>,
        sel: &mut Selection,
    ) -> TextViewOutput {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, theme::DOMINANT);

        let ppp = ui.pixels_per_point();
        let font = FontId::monospace(theme::MONO_SIZE);
        let cell_w = ui.fonts_mut(|f| f.glyph_width(&font, '0')).max(1.0);

        if self.wrap {
            self.hscroll_px = 0.0;
        }

        let len = cx.source.len();
        if len == 0 && !cx.frozen {
            self.anchor = 0;
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "This file is empty.",
                FontId::proportional(theme::MONO_SIZE),
                theme::MUTED,
            );
            return TextViewOutput {
                response,
                error: None,
                top: 0,
                visible_bytes: 0,
                notice: None,
            };
        }

        // Update monotone gutter width from index total lines.
        self.gutter_cells = gutter_cells(self.gutter_cells, cx.index.total_lines());
        let gutter_w = if self.line_numbers {
            GUTTER_LEFT_PAD + self.gutter_cells as f32 * cell_w + GUTTER_RIGHT_PAD
        } else {
            0.0
        };
        let text_left = if self.line_numbers {
            rect.left() + gutter_w + 1.0 + TEXT_LEFT_PAD_WITH_GUTTER
        } else {
            rect.left() + TEXT_LEFT_PAD_NO_GUTTER
        };

        let prelim_content_w = (rect.right() - VBAR_W - text_left).max(16.0);
        let show_hbar = !self.wrap && self.max_row_w > prelim_content_w;
        let vbar_bottom = if show_hbar {
            rect.bottom() - HBAR_H
        } else {
            rect.bottom()
        };
        let vtrack = Rect::from_min_max(
            pos2(rect.right() - VBAR_W, rect.top()),
            pos2(rect.right(), vbar_bottom),
        );
        let text_rect = Rect::from_min_max(
            pos2(text_left, rect.top()),
            pos2(vtrack.left(), vbar_bottom),
        );
        let content_w = text_rect.width().max(16.0);
        let visible_rows =
            ((text_rect.height().max(0.0) / theme::ROW_HEIGHT).ceil() as usize).max(1);
        self.visible_rows = visible_rows;

        // Frozen state: paint last good rows dimmed to 40%, no I/O, no input.
        if cx.frozen {
            let mut dim_painter = ui.painter_at(rect);
            dim_painter.set_opacity(0.4);
            paint_rows(
                &dim_painter,
                rect,
                text_rect,
                gutter_w,
                self.line_numbers,
                self.hscroll_px,
                ppp,
                &font,
                &self.last_good,
                sel,
                false,
                ui,
            );
            return TextViewOutput {
                response,
                error: None,
                top: self.anchor,
                visible_bytes: 0,
                notice: None,
            };
        }

        self.invalidate_cache_if_needed(LayoutKey {
            content_w_bits: content_w.to_bits(),
            wrap: self.wrap,
            ppp_bits: ppp.to_bits(),
            version: cx.source.version(),
            len,
        });

        // Focus & IME probe (Phase 1 contract).
        if response.clicked() || !self.focus_requested {
            response.request_focus();
            self.focus_requested = true;
        }
        let focused = response.has_focus();
        if focused {
            let ime = &mut self.ime;
            ui.input(|i| {
                for ev in &i.events {
                    if let egui::Event::Ime(ime_ev) = ev {
                        ime.apply(ime_ev);
                    }
                }
            });
            if !self.ime.is_composing() && ui.input(|i| i.key_pressed(Key::Escape)) {
                self.ime.escape();
            }
        }
        let composing = self.ime.is_composing();

        let mut notice: Option<&'static str> = None;
        let mut read_err: Option<SourceError> = None;

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
            if !self.wrap {
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
            }
            if !(shift && !self.wrap) && dy != 0.0 {
                self.sub_px += dy;
                let whole = (self.sub_px / theme::ROW_HEIGHT).trunc();
                if whole != 0.0 {
                    self.sub_px -= whole * theme::ROW_HEIGHT;
                    let cur_anchor = self.anchor;
                    let mut lay = EguiLayouter {
                        view: self,
                        ui,
                        content_w,
                        notice: &mut notice,
                    };
                    match step_rows(cx.source, &mut lay, cur_anchor, -(whole as i64)) {
                        Ok(next) => {
                            if next != cur_anchor {
                                lay.view.anchor = next;
                                lay.view.bump_scroll();
                            }
                        }
                        Err(e) => read_err = Some(e),
                    }
                }
            }
        }

        // Keyboard navigation & selection.
        if read_err.is_none()
            && focused
            && !cx.keys_blocked
            && !ui.ctx().any_popup_open()
            && let Err(e) = self.handle_keys(
                ui,
                cx.source,
                content_w,
                visible_rows,
                composing,
                sel,
                &mut notice,
            )
        {
            read_err = Some(e);
        }

        // Build visible rows starting at `self.anchor` (+1 row for partial bottom line).
        let mut built: Vec<VisibleRow> = Vec::with_capacity(visible_rows + 1);
        let mut visible_bytes = 0u64;
        if read_err.is_none() {
            match self.build_visible(ui, cx, content_w, visible_rows + 1, &mut notice) {
                Ok((rows, vis_b)) => {
                    built = rows;
                    visible_bytes = vis_b;
                }
                Err(e) => read_err = Some(e),
            }
        }

        if read_err.is_none() {
            self.last_good = built;
        }

        // Mouse click / drag / double-click word / triple-click paragraph selection.
        if read_err.is_none()
            && let Some(pos) = response.interact_pointer_pos()
            && text_rect.contains(pos)
        {
            let off = self.handle_click(cx, text_rect, pos);
            let shift = ui.input(|i| i.modifiers.shift);
            if response.triple_clicked() {
                match self.handle_triple_select(cx.source, off) {
                    Ok(r) => {
                        sel.anchor = r.start;
                        sel.head = r.end;
                        self.increment_scroll_epoch();
                    }
                    Err(e) => read_err = Some(e),
                }
            } else if response.double_clicked() {
                match self.handle_word_select(cx.source, off) {
                    Ok(r) => {
                        sel.anchor = r.start;
                        sel.head = r.end;
                        self.increment_scroll_epoch();
                    }
                    Err(e) => read_err = Some(e),
                }
            } else if response.drag_started() || response.clicked() {
                if shift {
                    sel.extend_to(off);
                } else {
                    *sel = Selection::caret(off);
                }
                self.increment_scroll_epoch();
            } else if response.dragged() {
                sel.extend_to(off);
                self.increment_scroll_epoch();
            }
        }

        // Paint visible rows (or last_good on error) with selection highlights.
        paint_rows(
            &painter,
            rect,
            text_rect,
            gutter_w,
            self.line_numbers,
            self.hscroll_px,
            ppp,
            &font,
            &self.last_good,
            sel,
            focused,
            ui,
        );

        // IME probe strip + caret (Phase 1 behaviour).
        if focused {
            let ox = snap::snap(text_rect.left(), ppp);
            let oy = snap::snap(text_rect.top(), ppp);
            let mut caret_x = ox;
            if composing || !self.ime.committed().is_empty() {
                let strip = Rect::from_min_size(
                    pos2(text_rect.left(), text_rect.top()),
                    vec2(text_rect.width(), 2.0 * theme::ROW_HEIGHT),
                );
                painter.rect_filled(strip, 0.0, theme::SECONDARY);
                let pre = painter.layout_no_wrap(
                    self.ime.preedit().to_owned(),
                    font.clone(),
                    theme::TEXT,
                );
                let w = pre.size().x;
                let underline_y = oy + theme::ROW_HEIGHT - 3.0;
                painter.galley(pos2(ox, oy), pre, theme::TEXT);
                if composing {
                    painter.line_segment(
                        [pos2(ox, underline_y), pos2(ox + w, underline_y)],
                        Stroke::new(1.0, theme::ACCENT),
                    );
                }
                caret_x = snap::snap(ox + w, ppp);
                let done = painter.layout_no_wrap(
                    self.ime.committed().to_owned(),
                    font.clone(),
                    theme::TEXT,
                );
                painter.galley(pos2(ox, oy + theme::ROW_HEIGHT), done, theme::TEXT);
            }
            let caret_rect = Rect::from_min_size(pos2(caret_x, oy), vec2(2.0, theme::ROW_HEIGHT));
            painter.rect_filled(caret_rect, 0.0, theme::ACCENT);
            ui.ctx().output_mut(|o| {
                o.ime = Some(egui::output::IMEOutput {
                    purpose: egui::IMEPurpose::Normal,
                    rect: text_rect,
                    cursor_rect: caret_rect,
                    should_interrupt_composition: false,
                });
            });
        }

        // Vertical scrollbar (f64 fractions).
        let start_frac = if len > 0 {
            self.anchor as f64 / len as f64
        } else {
            0.0
        };
        let vis_frac = if len > 0 {
            (visible_bytes.max(1) as f64 / len as f64).min(1.0)
        } else {
            1.0
        };
        if let Some(new_frac) = scrollbar::scrollbar(
            ui,
            vtrack,
            response.id.with("vsb"),
            Axis::Vertical,
            start_frac,
            vis_frac,
        ) && read_err.is_none()
        {
            let target = ((new_frac * (len.saturating_sub(1) as f64)).round() as u64)
                .min(len.saturating_sub(1));
            let mut lay = EguiLayouter {
                view: self,
                ui,
                content_w,
                notice: &mut notice,
            };
            match row_start_of(cx.source, &mut lay, target) {
                Ok(snapped) => {
                    if snapped != lay.view.anchor {
                        lay.view.anchor = snapped;
                        lay.view.sub_px = 0.0;
                        lay.view.bump_scroll();
                    }
                }
                Err(e) => read_err = Some(e),
            }
        }

        // Horizontal scrollbar (wrap off only, when max_row_w > content_w).
        if show_hbar {
            let htrack = Rect::from_min_max(
                pos2(text_rect.left(), vbar_bottom),
                pos2(vtrack.left(), rect.bottom()),
            );
            let max_h = (self.max_row_w - content_w).max(1.0);
            let h_start = f64::from(self.hscroll_px / max_h);
            let h_vis = f64::from((content_w / self.max_row_w).min(1.0));
            if let Some(new_h) = scrollbar::scrollbar(
                ui,
                htrack,
                response.id.with("hsb"),
                Axis::Horizontal,
                h_start,
                h_vis,
            ) {
                self.hscroll_px = (new_h as f32 * max_h).clamp(0.0, max_h);
            }
        }

        TextViewOutput {
            response,
            error: read_err,
            top: self.anchor,
            visible_bytes,
            notice,
        }
    }

    fn build_visible(
        &mut self,
        ui: &egui::Ui,
        cx: &TextViewCtx<'_>,
        content_w: f32,
        max_rows: usize,
        notice: &mut Option<&'static str>,
    ) -> Result<(Vec<VisibleRow>, u64), SourceError> {
        let len = cx.source.len();
        if len == 0 {
            return Ok((Vec::new(), 0));
        }

        let anchor = self.anchor.min(len - 1);
        let first_seg_s = scroll::segment_start(cx.source, anchor)?;
        let (first_line0, _) = cx.index.line_of(cx.source, first_seg_s)?;
        let mut line1 = first_line0.saturating_add(1);

        let mut seg = scroll::segment_at(cx.source, first_seg_s)?;
        let mut para = self.get_or_layout(ui, cx.source, seg, content_w, notice)?;
        let mut row_in_seg = row_index_for_byte(&seg, &para.decoded, &para.rows, anchor);

        let mut out = Vec::with_capacity(max_rows);
        let mut last_end = anchor;

        while out.len() < max_rows {
            let mark = gutter_mark_for_row(&seg, row_in_seg, line1, cx.index);
            let row_end_byte = if row_in_seg + 1 < para.rows.len() {
                row_byte(&seg, &para.decoded, para.rows[row_in_seg + 1])
            } else {
                seg.end
            };
            last_end = row_end_byte;
            out.push(VisibleRow {
                seg,
                decoded: Arc::clone(&para.decoded),
                galley: Arc::clone(&para.galley),
                row_in_seg,
                mark,
            });

            if row_in_seg + 1 < para.rows.len() {
                row_in_seg += 1;
            } else {
                if seg.end >= len {
                    break;
                }
                if seg.newline_end {
                    line1 = line1.saturating_add(1);
                }
                seg = scroll::segment_at(cx.source, seg.end)?;
                para = self.get_or_layout(ui, cx.source, seg, content_w, notice)?;
                row_in_seg = 0;
            }
        }

        Ok((out, last_end.saturating_sub(anchor)))
    }
}

struct EguiLayouter<'a> {
    view: &'a mut TextView,
    ui: &'a egui::Ui,
    content_w: f32,
    notice: &'a mut Option<&'static str>,
}

impl RowLayouter for EguiLayouter<'_> {
    fn rows(&mut self, seg: &Segment, decoded: &Decoded) -> Vec<RowSpan> {
        if let Some(hit) = self.view.cache.get(&seg.start) {
            return hit.rows.clone();
        }
        let font = FontId::monospace(theme::MONO_SIZE);
        if !decoded.text.is_ascii()
            && let Some(msg) = self.view.fonts.check(self.ui.ctx(), &font, &decoded.text)
        {
            *self.notice = Some(msg);
        }
        let job = build_layout_job(decoded, font, self.view.wrap, self.content_w);
        let galley = self.ui.fonts_mut(|f| f.layout_job(job));
        let rows = spans_from_galley(&galley, decoded.char_count());
        let row_w = galley.size().x;
        if row_w > self.view.max_row_w {
            self.view.max_row_w = row_w;
        }
        rows
    }
}

fn build_layout_job(decoded: &Decoded, font: FontId, wrap: bool, content_w: f32) -> LayoutJob {
    let mut job = LayoutJob {
        wrap: if wrap {
            TextWrapping::wrap_at_width(content_w)
        } else {
            TextWrapping::no_max_width()
        },
        ..Default::default()
    };

    let fmt = |color| TextFormat {
        font_id: font.clone(),
        color,
        line_height: Some(theme::ROW_HEIGHT),
        ..Default::default()
    };

    if decoded.dim.is_empty() {
        job.append(&decoded.text, 0.0, fmt(theme::TEXT));
        return job;
    }

    // Build byte boundaries in `decoded.text` corresponding to char indices in `decoded.dim`.
    let char_bytes: Vec<usize> = decoded
        .text
        .char_indices()
        .map(|(b, _)| b)
        .chain(std::iter::once(decoded.text.len()))
        .collect();
    let total_chars = char_bytes.len() - 1;
    let mut cur = 0usize;
    for r in &decoded.dim {
        let s = r.start.min(total_chars);
        let e = r.end.min(total_chars);
        if s > cur {
            job.append(
                &decoded.text[char_bytes[cur]..char_bytes[s]],
                0.0,
                fmt(theme::TEXT),
            );
        }
        if e > s {
            job.append(
                &decoded.text[char_bytes[s]..char_bytes[e]],
                0.0,
                fmt(theme::MUTED_DIM),
            );
        }
        cur = e;
    }
    if cur < total_chars {
        job.append(&decoded.text[char_bytes[cur]..], 0.0, fmt(theme::TEXT));
    }
    job
}

fn spans_from_galley(galley: &Galley, total_chars: usize) -> Vec<RowSpan> {
    if galley.rows.is_empty() {
        return vec![RowSpan {
            first_char: 0,
            chars: total_chars,
        }];
    }
    let mut out = Vec::with_capacity(galley.rows.len());
    let mut first = 0usize;
    for placed in &galley.rows {
        let c: usize = placed.row.char_count_excluding_newline().into();
        out.push(RowSpan {
            first_char: first,
            chars: c,
        });
        first += c;
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn paint_rows(
    painter: &egui::Painter,
    rect: Rect,
    text_rect: Rect,
    gutter_w: f32,
    line_numbers: bool,
    hscroll_px: f32,
    ppp: f32,
    font: &FontId,
    rows: &[VisibleRow],
    sel: &Selection,
    focused: bool,
    ui: &egui::Ui,
) {
    if line_numbers {
        let sep_x = snap::snap(rect.left() + gutter_w + 0.5, ppp);
        painter.line_segment(
            [pos2(sep_x, rect.top()), pos2(sep_x, text_rect.bottom())],
            Stroke::new(1.0, theme::SEPARATOR),
        );
    }

    let text_painter = painter.with_clip_rect(text_rect);
    let gutter_right = rect.left() + gutter_w - GUTTER_RIGHT_PAD;
    let hover_pos = ui.input(|i| i.pointer.hover_pos());
    let sel_range = sel.range();
    let sel_fill = if focused {
        theme::SELECTION
    } else {
        theme::SELECTION_INACTIVE
    };

    let mut idx = 0usize;
    while idx < rows.len() {
        let first = &rows[idx];
        let y = snap::row_y(text_rect.top(), idx, theme::ROW_HEIGHT, ppp);

        // Paint gutter marks and per-row selection rects for each visible row of this segment.
        let seg_start_idx = idx;
        let seg_start_row_in_seg = first.row_in_seg;
        while idx < rows.len() && rows[idx].seg.start == first.seg.start {
            let r = &rows[idx];
            let ry = snap::row_y(text_rect.top(), idx, theme::ROW_HEIGHT, ppp);
            if !sel_range.is_empty()
                && sel_range.start < r.seg.end
                && sel_range.end > r.seg.start
                && let Some(placed) = r.galley.rows.get(r.row_in_seg)
            {
                let rel_s = sel_range.start.saturating_sub(r.seg.start) as u32;
                let rel_e = sel_range.end.min(r.seg.end).saturating_sub(r.seg.start) as u32;
                let c_start = text::char_for_byte(&r.decoded, rel_s);
                let c_end = text::char_for_byte(&r.decoded, rel_e);
                let mut row_first_c = 0usize;
                for prev in r.galley.rows.iter().take(r.row_in_seg) {
                    let c: usize = prev.row.char_count_excluding_newline().into();
                    row_first_c += c;
                }
                let row_len: usize = placed.row.char_count_excluding_newline().into();
                let row_end_c = row_first_c + row_len;
                let lo = c_start.max(row_first_c).min(row_end_c) - row_first_c;
                let hi = c_end.max(row_first_c).min(row_end_c) - row_first_c;
                let x0 = text_rect.left() - hscroll_px + placed.row.x_offset(lo.into());
                let x1 = if sel_range.end >= r.seg.end && r.row_in_seg + 1 == r.galley.rows.len() {
                    text_rect.left() - hscroll_px + placed.row.x_offset(hi.into()) + 6.0
                } else {
                    text_rect.left() - hscroll_px + placed.row.x_offset(hi.into())
                };
                if x1 > x0 {
                    let srect = Rect::from_min_max(pos2(x0, ry), pos2(x1, ry + theme::ROW_HEIGHT));
                    text_painter.rect_filled(srect, 0.0, sel_fill);
                }
            }
            if line_numbers {
                match r.mark {
                    GutterMark::Number(n, ex) => {
                        let (label, color) = match ex {
                            Exactness::Exact => (n.to_string(), theme::MUTED),
                            Exactness::Estimated => (format!("~{n}"), theme::MUTED_DIM),
                        };
                        painter.text(
                            pos2(snap::snap(gutter_right, ppp), ry + theme::ROW_HEIGHT * 0.5),
                            Align2::RIGHT_CENTER,
                            label,
                            font.clone(),
                            color,
                        );
                    }
                    GutterMark::Continuation => {
                        painter.text(
                            pos2(snap::snap(gutter_right, ppp), ry + theme::ROW_HEIGHT * 0.5),
                            Align2::RIGHT_CENTER,
                            "\u{203A}",
                            font.clone(),
                            theme::MUTED,
                        );
                        let cell_rect = Rect::from_min_size(
                            pos2(rect.left(), ry),
                            vec2(gutter_w, theme::ROW_HEIGHT),
                        );
                        if hover_pos.is_some_and(|p| cell_rect.contains(p)) {
                            let chars = r.decoded.char_count();
                            egui::Tooltip::always_open(
                                ui.ctx().clone(),
                                ui.layer_id(),
                                ui.scope_id().with(("cont_tip", r.seg.start)),
                                egui::PopupAnchor::Pointer,
                            )
                            .show(|ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "Long line continued ({chars} chars/row)"
                                    ))
                                    .size(12.0),
                                );
                            });
                        }
                    }
                    GutterMark::Blank => {}
                }
            }
            idx += 1;
        }

        // Paint the segment's galley once, shifted up by `seg_start_row_in_seg * ROW_HEIGHT`
        // and clipped vertically to the rows of this segment that are in view.
        let vis_count = idx - seg_start_idx;
        let seg_clip = Rect::from_min_max(
            pos2(text_rect.left(), y),
            pos2(
                text_rect.right(),
                (y + vis_count as f32 * theme::ROW_HEIGHT).min(text_rect.bottom()),
            ),
        );
        let seg_painter = text_painter.with_clip_rect(seg_clip);
        let gx = snap::snap(text_rect.left() - hscroll_px, ppp);
        let gy = snap::snap(y - seg_start_row_in_seg as f32 * theme::ROW_HEIGHT, ppp);
        seg_painter.galley(pos2(gx, gy), Arc::clone(&first.galley), theme::TEXT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lineindex::{IndexMsg, spawn_indexer};
    use crate::source::MemSource;
    use crate::testutil::{XorShift, fixtures};
    use std::time::Duration;

    fn complete_index(bytes: &[u8]) -> LineIndex {
        let mut idx = LineIndex::new(bytes.len() as u64, bytes);
        let job = spawn_indexer(1, Box::new(MemSource::new(bytes.to_vec())), Box::new(|| {}));
        while let Ok(m) = job.rx.recv_timeout(Duration::from_secs(5)) {
            let done = matches!(m, IndexMsg::Done { .. });
            idx.apply(&m);
            if done {
                break;
            }
        }
        idx
    }

    #[test]
    fn step_inverse() {
        let mut rng = XorShift::new(4242);
        let wraps = [Some(7usize), Some(80), None];
        for case in 0..50 {
            let data = if case == 0 {
                fixtures::long_line(60_000)
            } else {
                let len = 50 + rng.below(4_000) as usize;
                fixtures::random_text(&mut rng, len, (case % 10) as u32)
            };
            let src = MemSource::new(data);
            let mut lay = MonoLayouter {
                wrap_at: wraps[case % wraps.len()],
            };
            let last = anchor_for_end(&src, &mut lay, 1).unwrap();
            for _ in 0..10 {
                let off = rng.below(src.len().max(1));
                let a = row_start_of(&src, &mut lay, off).unwrap();
                let k = 1 + rng.below(40) as i64;
                let fwd = step_rows(&src, &mut lay, a, k).unwrap();
                if fwd < last {
                    let back = step_rows(&src, &mut lay, fwd, -k).unwrap();
                    assert_eq!(back, a, "case={case} a={a} k={k} fwd={fwd}");
                }
            }
        }
    }

    #[test]
    fn step_clamps() {
        let src = MemSource::new(b"aa\nbb\ncc".to_vec());
        let mut lay = MonoLayouter { wrap_at: None };
        assert_eq!(step_rows(&src, &mut lay, 0, -1).unwrap(), 0);
        assert_eq!(step_rows(&src, &mut lay, 0, 100).unwrap(), 6);
        assert_eq!(step_rows(&src, &mut lay, 6, 1).unwrap(), 6);
    }

    #[test]
    fn end_shows_last_row() {
        let data = fixtures::tiny_lines(30);
        let src = MemSource::new(data);
        let mut lay = MonoLayouter { wrap_at: None };
        let a = anchor_for_end(&src, &mut lay, 10).unwrap();
        let last = step_rows(&src, &mut lay, a, 9).unwrap();
        assert_eq!(step_rows(&src, &mut lay, last, 1).unwrap(), last);

        // Short file stays at 0.
        let short = MemSource::new(b"a\nb\nc".to_vec());
        assert_eq!(anchor_for_end(&short, &mut lay, 10).unwrap(), 0);
    }

    #[test]
    fn row_start_snapping() {
        let src = MemSource::new(b"abcdefghij\nklmnop".to_vec());
        let mut lay = MonoLayouter { wrap_at: Some(4) };
        for off in 0..src.len() {
            let r = row_start_of(&src, &mut lay, off).unwrap();
            assert!(r <= off);
            assert_eq!(row_start_of(&src, &mut lay, r).unwrap(), r);
        }
    }

    #[test]
    fn gutter_marks() {
        let mut data = b"a\nbb\n".to_vec();
        data.extend_from_slice(&vec![b'x'; 25_000]);
        data.extend_from_slice(b"\nz");
        let idx = complete_index(&data);
        let src = MemSource::new(data);
        let mut lay = MonoLayouter { wrap_at: Some(7) };

        let mut seg = scroll::segment_at(&src, 0).unwrap();
        let mut marks = Vec::new();
        let mut line1 = 1u64;
        loop {
            let dec = decode_seg_from_source(&src, &seg).unwrap();
            let rows = lay.rows(&seg, &dec);
            for (i, _) in rows.iter().enumerate() {
                marks.push(gutter_mark_for_row(&seg, i, line1, &idx));
            }
            if seg.end >= src.len() {
                break;
            }
            if seg.newline_end {
                line1 += 1;
            }
            seg = scroll::segment_at(&src, seg.end).unwrap();
        }
        assert_eq!(marks[0], GutterMark::Number(1, Exactness::Exact));
        assert_eq!(marks[1], GutterMark::Number(2, Exactness::Exact));
        assert_eq!(marks[2], GutterMark::Number(3, Exactness::Exact));
        assert_eq!(marks[3], GutterMark::Blank);
        assert!(marks.contains(&GutterMark::Continuation));
        assert_eq!(
            *marks.last().unwrap(),
            GutterMark::Number(4, Exactness::Exact)
        );
    }

    #[test]
    fn gutter_cells_monotone() {
        let c1 = gutter_cells(4, (999, Exactness::Estimated));
        assert_eq!(c1, 5);
        let c2 = gutter_cells(c1, (12, Exactness::Exact));
        assert_eq!(c2, 5);
        let c3 = gutter_cells(c2, (123_456, Exactness::Exact));
        assert_eq!(c3, 6);
    }

    #[test]
    fn egui_smoke() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0))),
            ..Default::default()
        };

        // (a) CJK lines
        let cjk = fixtures::cjk_lines(200);
        let idx = complete_index(&cjk);
        let src = MemSource::new(cjk);
        let mut view = TextView::new();
        let mut sel = Selection::caret(0);
        let mut out_top = 1;
        let mut out_vis = 0;
        for _ in 0..2 {
            let mut full = ctx.run_ui(input.clone(), |ui| {
                let cx = TextViewCtx {
                    source: &src,
                    index: &idx,
                    frozen: false,
                    keys_blocked: false,
                };
                let out = view.show(ui, &cx, &mut sel);
                assert!(out.error.is_none());
                out_top = out.top;
                out_vis = out.visible_bytes;
            });
            full.textures_delta.clear();
        }
        assert_eq!(out_top, 0);
        assert!(out_vis > 0);

        // (b) 3 MB single line with wrap off: reads < 1 MB in a frame
        let long = fixtures::long_line(3_000_000);
        let idx_l = LineIndex::new(long.len() as u64, &long[..65_536]);
        let src_l = MemSource::new(long);
        let mut view_l = TextView::new();
        view_l.wrap = false;
        for frame in 0..2 {
            if frame == 1 {
                src_l.reset_counter();
            }
            let mut full = ctx.run_ui(input.clone(), |ui| {
                let cx = TextViewCtx {
                    source: &src_l,
                    index: &idx_l,
                    frozen: false,
                    keys_blocked: false,
                };
                let out = view_l.show(ui, &cx, &mut sel);
                assert!(out.error.is_none());
                assert_eq!(out.top, 0);
                assert!(out.visible_bytes > 0);
            });
            full.textures_delta.clear();
        }
        // Second frame hits the layout cache (well under 1 MB).
        assert!(
            src_l.bytes_read() < 1024 * 1024,
            "read {} bytes",
            src_l.bytes_read()
        );

        // (c) Empty source
        let empty = MemSource::new(Vec::new());
        let idx_e = LineIndex::new(0, &[]);
        let mut view_e = TextView::new();
        for _ in 0..2 {
            let mut full = ctx.run_ui(input.clone(), |ui| {
                let cx = TextViewCtx {
                    source: &empty,
                    index: &idx_e,
                    frozen: false,
                    keys_blocked: false,
                };
                let out = view_e.show(ui, &cx, &mut sel);
                assert!(out.error.is_none());
                assert_eq!(out.top, 0);
                assert_eq!(out.visible_bytes, 0);
            });
            full.textures_delta.clear();
        }
    }

    #[test]
    fn word_and_triple_select_and_click_hit_test() {
        let data = b"hello world_123\nsecond line\n".to_vec();
        let idx = complete_index(&data);
        let src = MemSource::new(data);
        let mut view = TextView::new();

        // Word select on 'w' in "world_123"
        assert_eq!(view.handle_word_select(&src, 6).unwrap(), 6..15);
        // Triple-click selects the entire paragraph including '\n'
        assert_eq!(view.handle_triple_select(&src, 6).unwrap(), 0..16);
        assert_eq!(view.handle_triple_select(&src, 18).unwrap(), 16..28);

        // Render one frame so last_good is populated, then click hit-test.
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0))),
            ..Default::default()
        };
        let mut sel = Selection { anchor: 0, head: 5 };
        let mut full = ctx.run_ui(input, |ui| {
            let cx = TextViewCtx {
                source: &src,
                index: &idx,
                frozen: false,
                keys_blocked: false,
            };
            let _ = view.show(ui, &cx, &mut sel);
        });
        full.textures_delta.clear();

        let cx = TextViewCtx {
            source: &src,
            index: &idx,
            frozen: false,
            keys_blocked: false,
        };
        let text_rect = Rect::from_min_size(pos2(80.0, 0.0), vec2(700.0, 600.0));
        let b0 = view.handle_click(&cx, text_rect, pos2(80.0, 5.0));
        assert_eq!(b0, 0);
        let b_row2 = view.handle_click(&cx, text_rect, pos2(80.0, 25.0));
        assert_eq!(b_row2, 16);
        let b_past = view.handle_click(&cx, text_rect, pos2(80.0, 400.0));
        assert_eq!(b_past, src.len());

        let before = view.scroll_epoch;
        view.increment_scroll_epoch();
        assert_eq!(view.scroll_epoch, before + 1);
    }
}
