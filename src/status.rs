//! Status bar formatting, segment collapse order, and rendering (D-16, UI-01).

use crate::backend::BACKEND;
use crate::goto::thousands;
use crate::lineindex::Exactness;
use crate::source::EolKind;
use crate::theme;
use eframe::egui::{self, Align, Layout, RichText, Sense, pos2, vec2};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct TransientMsg {
    pub text: String,
    pub destructive: bool,
    pub expires: Option<Instant>,
}

#[derive(Default)]
pub struct Status {
    pub transient: Option<TransientMsg>,
}

impl Status {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show(&mut self, text: impl Into<String>, ttl: Option<Duration>) {
        self.transient = Some(TransientMsg {
            text: text.into(),
            destructive: false,
            expires: ttl.map(|d| Instant::now() + d),
        });
    }

    pub fn show_error(&mut self, text: impl Into<String>) {
        self.transient = Some(TransientMsg {
            text: text.into(),
            destructive: true,
            expires: None,
        });
    }

    pub fn clear(&mut self) {
        self.transient = None;
    }

    pub fn poll(&mut self) {
        if let Some(t) = &self.transient
            && let Some(exp) = t.expires
            && Instant::now() >= exp
        {
            self.transient = None;
        }
    }
}

/// Human-readable size with one decimal place (base 1024, UI-SPEC).
pub fn copy_size_text(n: u64) -> String {
    if n < 1024 {
        return format!("{n} B");
    }
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    let mut v = n as f64 / 1024.0;
    let mut u = 0;
    while v >= 1024.0 && u + 1 < UNITS.len() {
        v /= 1024.0;
        u += 1;
    }
    format!("{v:.1} {}", UNITS[u])
}

/// Capitalized EOL label for the status bar.
pub fn eol_label(k: EolKind) -> &'static str {
    match k {
        EolKind::Lf => "LF",
        EolKind::Crlf => "CRLF",
        EolKind::Mixed => "Mixed",
        EolKind::Unknown => "Unknown",
    }
}

pub fn indexing_text(pct: u8) -> String {
    format!("Indexing {pct}%")
}

pub fn notice_text(notice: Option<&'static str>) -> Option<String> {
    notice.map(str::to_owned)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    Line,
    Col,
    Offset,
    Size,
    Encoding,
    Eol,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub kind: SegmentKind,
    pub text: String,
    pub est_width: u32,
}

/// Collapse order (D-16, UI-SPEC): drop EOL -> Encoding -> Size -> Col -> Offset.
/// Line never drops.
pub fn fit_segments(mut segments: Vec<Segment>, available_width: f32) -> Vec<Segment> {
    const DROP_ORDER: [SegmentKind; 5] = [
        SegmentKind::Eol,
        SegmentKind::Encoding,
        SegmentKind::Size,
        SegmentKind::Col,
        SegmentKind::Offset,
    ];
    let avail = available_width.max(0.0) as u32;
    let total = |segs: &[Segment]| -> u32 { segs.iter().map(|s| s.est_width + 17).sum::<u32>() };
    for kind in DROP_ORDER {
        if total(&segments) <= avail {
            break;
        }
        segments.retain(|s| s.kind != kind);
    }
    segments
}

pub struct StatusCtx<'a> {
    pub has_doc: bool,
    pub line1: u64,
    pub line_exact: Exactness,
    pub col1: u64,
    pub offset: u64,
    pub len: u64,
    pub encoding: &'a str,
    pub eol: EolKind,
    pub indexing_pct: Option<u8>,
    pub copying_text: Option<&'a str>,
    pub transient: Option<&'a TransientMsg>,
    pub bench: bool,
    pub startup_ms: Option<u64>,
}

pub struct StatusAction {
    pub cancel_indexing: bool,
    pub cancel_copy: bool,
}

fn small(text: impl Into<String>, color: egui::Color32) -> RichText {
    RichText::new(text.into()).size(12.0).color(color)
}

fn divider(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(1.0, 14.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, theme::SEPARATOR);
}

pub fn render_status_bar(ui: &mut egui::Ui, cx: &StatusCtx<'_>) -> StatusAction {
    let mut action = StatusAction {
        cancel_indexing: false,
        cancel_copy: false,
    };

    ui.horizontal_centered(|ui| {
        if let Some(t) = cx.transient {
            let color = if t.destructive {
                theme::DESTRUCTIVE
            } else {
                theme::TEXT
            };
            ui.label(small(&t.text, color));
        } else if cx.has_doc {
            let ln_text = match cx.line_exact {
                Exactness::Exact => format!("Ln {}", thousands(cx.line1)),
                Exactness::Estimated => format!("Ln {} (~)", thousands(cx.line1)),
            };
            let all = vec![
                Segment {
                    kind: SegmentKind::Line,
                    est_width: (ln_text.len() as u32) * 7,
                    text: ln_text,
                },
                Segment {
                    kind: SegmentKind::Col,
                    text: format!("Col {}", thousands(cx.col1)),
                    est_width: 56,
                },
                Segment {
                    kind: SegmentKind::Offset,
                    text: format!("0x{:X}", cx.offset),
                    est_width: 84,
                },
                Segment {
                    kind: SegmentKind::Size,
                    text: copy_size_text(cx.len),
                    est_width: 64,
                },
                Segment {
                    kind: SegmentKind::Encoding,
                    text: cx.encoding.to_owned(),
                    est_width: 48,
                },
                Segment {
                    kind: SegmentKind::Eol,
                    text: eol_label(cx.eol).to_owned(),
                    est_width: 42,
                },
            ];
            let right_reserve = if cx.indexing_pct.is_some() || cx.copying_text.is_some() {
                180.0
            } else if cx.bench {
                160.0
            } else {
                16.0
            };
            let visible = fit_segments(all, (ui.available_width() - right_reserve).max(80.0));
            for (i, seg) in visible.iter().enumerate() {
                if i > 0 {
                    divider(ui);
                }
                ui.label(small(&seg.text, theme::MUTED));
            }
        }

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if cx.bench {
                ui.label(small(BACKEND, theme::MUTED));
                divider(ui);
                let s = match cx.startup_ms {
                    Some(ms) => format!("Startup {ms} ms"),
                    None => "Startup \u{2014} ms".to_owned(),
                };
                ui.label(small(s, theme::MUTED));
            }
            if let Some(pct) = cx.indexing_pct {
                if ui
                    .add(egui::Button::new(small("\u{2715}", theme::MUTED)).frame(false))
                    .on_hover_text("Cancel indexing (Esc)")
                    .clicked()
                {
                    action.cancel_indexing = true;
                }
                let (bar_rect, _) = ui.allocate_exact_size(vec2(48.0, 2.0), Sense::hover());
                ui.painter().rect_filled(bar_rect, 0.0, theme::SEPARATOR);
                let fill_w = bar_rect.width() * (f32::from(pct.min(100)) / 100.0);
                let fill = egui::Rect::from_min_size(bar_rect.min, vec2(fill_w, 2.0));
                ui.painter().rect_filled(fill, 0.0, theme::ACCENT);
                ui.label(small(indexing_text(pct), theme::MUTED));
            } else if let Some(copy_lbl) = cx.copying_text {
                if ui
                    .add(egui::Button::new(small("\u{2715}", theme::MUTED)).frame(false))
                    .on_hover_text("Cancel copy (Esc)")
                    .clicked()
                {
                    action.cancel_copy = true;
                }
                let _ = pos2(0.0, 0.0);
                ui.label(small(copy_lbl, theme::MUTED));
            }
        });
    });

    action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_segments() -> Vec<Segment> {
        vec![
            Segment {
                kind: SegmentKind::Line,
                text: "Ln 1,234".into(),
                est_width: 60,
            },
            Segment {
                kind: SegmentKind::Col,
                text: "Col 5".into(),
                est_width: 45,
            },
            Segment {
                kind: SegmentKind::Offset,
                text: "0x100".into(),
                est_width: 60,
            },
            Segment {
                kind: SegmentKind::Size,
                text: "10.0 KB".into(),
                est_width: 55,
            },
            Segment {
                kind: SegmentKind::Encoding,
                text: "UTF-8".into(),
                est_width: 45,
            },
            Segment {
                kind: SegmentKind::Eol,
                text: "LF".into(),
                est_width: 30,
            },
        ]
    }

    #[test]
    fn segments_order_and_collapse() {
        let all = fit_segments(sample_segments(), 1000.0);
        let kinds: Vec<_> = all.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            vec![
                SegmentKind::Line,
                SegmentKind::Col,
                SegmentKind::Offset,
                SegmentKind::Size,
                SegmentKind::Encoding,
                SegmentKind::Eol
            ]
        );

        // Tight width drops EOL first, then Encoding, Size, Col, Offset, keeping Line.
        let one = fit_segments(sample_segments(), 10.0);
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].kind, SegmentKind::Line);

        // Drop EOL only
        let no_eol = fit_segments(sample_segments(), 360.0);
        assert!(!no_eol.iter().any(|s| s.kind == SegmentKind::Eol));
        assert!(no_eol.iter().any(|s| s.kind == SegmentKind::Encoding));
    }

    #[test]
    fn eol_labels() {
        assert_eq!(eol_label(EolKind::Unknown), "Unknown");
        assert_eq!(eol_label(EolKind::Lf), "LF");
        assert_eq!(eol_label(EolKind::Crlf), "CRLF");
        assert_eq!(eol_label(EolKind::Mixed), "Mixed");
    }

    #[test]
    fn copy_size_formatting() {
        assert_eq!(copy_size_text(100), "100 B");
        assert_eq!(copy_size_text(16 * 1024 * 1024), "16.0 MB");
        assert_eq!(copy_size_text(300 * 1024 * 1024), "300.0 MB");
    }

    #[test]
    fn indexing_label() {
        assert_eq!(indexing_text(42), "Indexing 42%");
    }
}
