//! Reusable vertical/horizontal scrollbar widget for both views (UI-SPEC).

use crate::theme;
use eframe::egui::{self, Color32, Id, Rect, Sense, pos2, vec2};

pub const VBAR_W: f32 = 12.0;
pub const HBAR_H: f32 = 8.0;
pub const MIN_THUMB: f32 = 24.0;
pub const THUMB: Color32 = Color32::from_rgb(0x8B, 0x8F, 0x98);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Vertical,
    Horizontal,
}

/// Computes `(thumb_start, thumb_len)` along a track of length `track_len`.
///
/// Fractions are `f64` so 20 GiB files never lose precision (Pitfall 10).
pub fn thumb(start_frac: f64, visible_frac: f64, track_len: f32) -> (f32, f32) {
    let track = f64::from(track_len.max(0.0));
    if track <= 0.0 {
        return (0.0, 0.0);
    }
    let vis = visible_frac.clamp(0.0, 1.0);
    let len = (track * vis).max(f64::from(MIN_THUMB)).min(track);
    let travel = (track - len).max(0.0);
    let start = (start_frac.clamp(0.0, 1.0) * travel).clamp(0.0, travel);
    (start as f32, len as f32)
}

/// Inverse of [`thumb`]: start fraction in `0.0..=1.0` for a thumb position.
pub fn frac_for_thumb(thumb_start: f32, thumb_len: f32, track_len: f32) -> f64 {
    let travel = f64::from(track_len) - f64::from(thumb_len);
    if travel <= 0.0 {
        return 0.0;
    }
    (f64::from(thumb_start) / travel).clamp(0.0, 1.0)
}

/// Paints a scrollbar track + thumb and returns `Some(new_start_frac)` while clicked or dragged.
///
/// Drag preserves the pointer grab offset captured on drag start in egui memory under `id`.
pub fn scrollbar(
    ui: &egui::Ui,
    track: Rect,
    id: Id,
    axis: Axis,
    start_frac: f64,
    visible_frac: f64,
) -> Option<f64> {
    let (track_origin, track_len) = match axis {
        Axis::Vertical => (track.top(), track.height()),
        Axis::Horizontal => (track.left(), track.width()),
    };

    let (t_start, t_len) = thumb(start_frac, visible_frac, track_len);
    let thumb_rect = match axis {
        Axis::Vertical => Rect::from_min_size(
            pos2(track.left(), track_origin + t_start),
            vec2(track.width(), t_len),
        ),
        Axis::Horizontal => Rect::from_min_size(
            pos2(track_origin + t_start, track.top()),
            vec2(t_len, track.height()),
        ),
    };

    let resp = ui.interact(track, id, Sense::click_and_drag());
    let grab_id = id.with("grab");
    let mut new_frac = None;

    if resp.drag_started()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let coord = match axis {
            Axis::Vertical => p.y,
            Axis::Horizontal => p.x,
        };
        let rel = coord - track_origin;
        let grab = if rel >= t_start && rel <= t_start + t_len {
            rel - t_start
        } else {
            t_len * 0.5
        };
        ui.ctx().data_mut(|d| d.insert_temp(grab_id, grab));
    }

    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let coord = match axis {
            Axis::Vertical => p.y,
            Axis::Horizontal => p.x,
        };
        let grab = if resp.dragged() {
            ui.ctx()
                .data(|d| d.get_temp::<f32>(grab_id))
                .unwrap_or(t_len * 0.5)
        } else {
            t_len * 0.5
        };
        let desired = coord - track_origin - grab;
        new_frac = Some(frac_for_thumb(desired, t_len, track_len));
    }

    let painter = ui.painter_at(track);
    painter.rect_filled(track, 0.0, theme::SECONDARY);
    let fill = if resp.hovered() || resp.dragged() {
        THUMB
    } else {
        theme::MUTED_DIM
    };
    painter.rect_filled(thumb_rect, 0.0, fill);

    new_frac
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::XorShift;

    #[test]
    fn thumb_min() {
        let (s, l) = thumb(0.0, 1e-9, 400.0);
        assert_eq!(s, 0.0);
        assert_eq!(l, MIN_THUMB);

        let (s_end, l_end) = thumb(1.0, 1e-9, 400.0);
        assert!((s_end + l_end - 400.0).abs() < 1e-4);
    }

    #[test]
    fn roundtrip() {
        let mut rng = XorShift::new(42);
        for _ in 0..100 {
            let f = (rng.next_u64() % 1_000_000) as f64 / 1_000_000.0;
            let v = 0.05;
            let track = 600.0;
            let (s, l) = thumb(f, v, track);
            let back = frac_for_thumb(s, l, track);
            assert!((back - f).abs() < 1e-5, "f={f} back={back}");
        }
    }

    #[test]
    fn degenerate_track() {
        assert_eq!(thumb(0.5, 0.5, 0.0), (0.0, 0.0));
        assert_eq!(frac_for_thumb(5.0, 24.0, 20.0), 0.0);
    }
}
