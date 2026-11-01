//! Byte-offset row model for the virtualized view (pure, no egui).

use std::ops::Range;

pub const BYTES_PER_ROW: u64 = 128;
pub const ROW_HEIGHT: f32 = 20.0;
pub const MAX_ROW_CHARS: usize = 4096;
pub const MIN_THUMB: f32 = 24.0;
pub const NON_PRINTABLE: char = '\u{00B7}';

/// Clamp an offset to a valid byte index (`len == 0` gives 0).
pub fn clamp_offset(off: u64, len: u64) -> u64 {
    if len == 0 { 0 } else { off.min(len - 1) }
}

/// Exclusive end of the row starting at `start`. Never exceeds `data.len()`.
pub fn row_end(data: &[u8], start: u64) -> u64 {
    let len = data.len() as u64;
    if start >= len {
        return len;
    }
    let win_end = start.saturating_add(BYTES_PER_ROW).min(len);
    let window = &data[start as usize..win_end as usize];
    match window.iter().position(|&b| b == b'\n') {
        Some(k) => start + k as u64 + 1,
        None => win_end,
    }
}

/// Up to `count` row ranges starting at `top`; stops at EOF, never empty ranges.
pub fn visible_rows(data: &[u8], top: u64, count: usize) -> Vec<Range<u64>> {
    let len = data.len() as u64;
    let mut out = Vec::new();
    if top >= len {
        return out;
    }
    let mut start = top;
    for _ in 0..count {
        if start >= len {
            break;
        }
        let end = row_end(data, start);
        if end <= start {
            break;
        }
        out.push(start..end);
        start = end;
    }
    out
}

/// Start of the row before `top`. Phase 1 rows are byte windows, so this is not
/// an exact inverse of `row_end` for lines longer than 128 bytes; Phase 2
/// replaces it with the line index.
pub fn prev_row_start(data: &[u8], top: u64) -> u64 {
    if top == 0 {
        return 0;
    }
    let top = top.min(data.len() as u64);
    if top == 0 {
        return 0;
    }
    let ws = top.saturating_sub(BYTES_PER_ROW);
    let window = &data[ws as usize..(top - 1) as usize];
    match window.iter().rposition(|&b| b == b'\n') {
        Some(k) => ws + k as u64 + 1,
        None => ws,
    }
}

/// Scroll by `delta` rows (positive = down), clamped to a valid row start.
pub fn scroll_rows(data: &[u8], top: u64, delta: i64) -> u64 {
    let len = data.len() as u64;
    let mut pos = clamp_offset(top, len);
    if delta >= 0 {
        for _ in 0..delta {
            let next = row_end(data, pos);
            if next >= len {
                break;
            }
            pos = next;
        }
    } else {
        for _ in 0..delta.unsigned_abs() {
            let prev = prev_row_start(data, pos);
            if prev == pos {
                break;
            }
            pos = prev;
        }
    }
    clamp_offset(pos, len)
}

/// Number of rows needed to cover `height` (one extra for partial rows).
pub fn rows_for_height(height: f32) -> usize {
    (height.max(0.0) / ROW_HEIGHT).ceil() as usize + 1
}

/// Printable form of a row: strips one trailing line ending, replaces control
/// characters and invalid UTF-8 with `NON_PRINTABLE`, caps at `MAX_ROW_CHARS`.
pub fn display_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    let mut out = String::new();
    let mut n = 0usize;
    for chunk in bytes.utf8_chunks() {
        for c in chunk.valid().chars() {
            if n >= MAX_ROW_CHARS {
                return out;
            }
            out.push(if c.is_control() { NON_PRINTABLE } else { c });
            n += 1;
        }
        for _ in chunk.invalid() {
            if n >= MAX_ROW_CHARS {
                return out;
            }
            out.push(NON_PRINTABLE);
            n += 1;
        }
    }
    out
}

/// Scrollbar thumb `(y_from_track_top, height)`; ratios computed in f64.
pub fn thumb_geometry(top: u64, len: u64, visible_bytes: u64, track_h: f32) -> (f32, f32) {
    if len == 0 {
        return (0.0, track_h);
    }
    let track = track_h as f64;
    let h = (track * visible_bytes as f64 / len as f64)
        .max(MIN_THUMB as f64)
        .min(track);
    let y = ((top as f64 / len as f64) * (track - h)).clamp(0.0, (track - h).max(0.0));
    (y as f32, h as f32)
}

/// Inverse of `thumb_geometry`: byte offset for a thumb position.
pub fn offset_for_thumb(thumb_y: f32, track_h: f32, thumb_h: f32, len: u64) -> u64 {
    if track_h <= thumb_h || len == 0 {
        return 0;
    }
    let frac = (thumb_y as f64 / (track_h - thumb_h) as f64).clamp(0.0, 1.0);
    let off = (frac * (len - 1) as f64).round() as u64;
    clamp_offset(off, len)
}

/// `0x` + 16 uppercase hex digits.
pub fn format_offset(off: u64) -> String {
    format!("0x{off:016X}")
}

/// 16 uppercase hex digits (gutter form).
pub fn format_gutter(off: u64) -> String {
    format!("{off:016X}")
}

/// Human size, base 1024, labels B/KB/MB/GB/TB.
pub fn format_size(len: u64) -> String {
    if len < 1024 {
        return format!("{len} B");
    }
    let units = ["KB", "MB", "GB", "TB"];
    let mut v = len as f64 / 1024.0;
    let mut u = 0;
    while v >= 1024.0 && u + 1 < units.len() {
        v /= 1024.0;
        u += 1;
    }
    format!("{v:.1} {}", units[u])
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn clamp_offset_cases() {
        assert_eq!(clamp_offset(5, 0), 0);
        assert_eq!(clamp_offset(10, 10), 9);
        assert_eq!(clamp_offset(u64::MAX, 10), 9);
    }

    #[test]
    fn row_end_newline_and_window() {
        assert_eq!(row_end(b"abc\ndef", 0), 4);
        assert_eq!(row_end(b"abc\ndef", 4), 7);
        let x = vec![b'x'; 300];
        assert_eq!(row_end(&x, 0), 128);
        assert_eq!(row_end(&x, 256), 300);
    }

    #[test]
    fn row_end_never_past_eof() {
        let d = b"abc";
        assert_eq!(row_end(d, 3), 3);
        assert_eq!(row_end(d, 99), 3);
        assert_eq!(row_end(d, u64::MAX), 3);
        assert_eq!(row_end(b"", 0), 0);
    }

    #[test]
    fn visible_rows_cases() {
        assert_eq!(visible_rows(b"a\nb\nc", 0, 10), vec![0..2, 2..4, 4..5]);
        assert!(visible_rows(b"", 0, 10).is_empty());
        assert!(visible_rows(b"abc", u64::MAX, 3).is_empty());
        assert_eq!(visible_rows(b"a\nb\nc", 0, 2).len(), 2);
    }

    #[test]
    fn prev_row_start_cases() {
        let d = b"aa\nbb\ncc";
        assert_eq!(prev_row_start(d, 0), 0);
        assert_eq!(prev_row_start(d, 6), 3);
        assert_eq!(prev_row_start(d, 3), 0);
        let x = vec![b'x'; 1000];
        assert_eq!(prev_row_start(&x, 500), 372);
    }

    #[test]
    fn scroll_rows_cases() {
        assert_eq!(scroll_rows(b"a\nb\nc", 0, -5), 0);
        assert_eq!(scroll_rows(b"a\nb\nc", 0, 2), 4);
        assert!(scroll_rows(b"a\nb\nc", 0, 100) < 5);
        assert_eq!(scroll_rows(b"", 0, 3), 0);
    }

    #[test]
    fn display_text_cases() {
        assert_eq!(display_text(b"hi\x00\x01"), "hi\u{b7}\u{b7}");
        assert_eq!(display_text(b"line\r\n"), "line");
        assert_eq!(display_text(b"line\n"), "line");
        assert_eq!(display_text("é".as_bytes()), "é");
        assert_eq!(display_text(&[0xFF, b'a']), "\u{b7}a");
        assert_eq!(display_text(b"\t"), "\u{b7}");
    }

    #[test]
    fn display_text_capped() {
        assert_eq!(display_text(&vec![b'a'; 5000]).chars().count(), 4096);
    }

    #[test]
    fn rows_for_height_cases() {
        assert_eq!(rows_for_height(100.0), 6);
        assert_eq!(rows_for_height(0.0), 1);
    }

    #[test]
    fn thumb_geometry_cases() {
        let len = 20 * GIB;
        let (y, h) = thumb_geometry(0, len, 5120, 600.0);
        assert_eq!(h, 24.0);
        assert_eq!(y, 0.0);
        let (y, h) = thumb_geometry(len - 1, len, 5120, 600.0);
        assert!(y + h <= 600.0);
        assert!(y >= 600.0 - 24.0 - 0.5);
        assert_eq!(thumb_geometry(0, 0, 0, 600.0), (0.0, 600.0));
    }

    #[test]
    fn offset_for_thumb_cases() {
        let len = 20 * GIB;
        assert_eq!(offset_for_thumb(0.0, 600.0, 24.0, len), 0);
        assert_eq!(offset_for_thumb(576.0, 600.0, 24.0, len), len - 1);
        assert!(offset_for_thumb(10_000.0, 600.0, 24.0, len) < len);
        assert_eq!(offset_for_thumb(5.0, 20.0, 24.0, len), 0);
    }

    #[test]
    fn format_cases() {
        assert_eq!(format_offset(0xA0), "0x00000000000000A0");
        assert_eq!(format_gutter(0xA0), "00000000000000A0");
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1536), "1.5 KB");
        assert_eq!(format_size(20 * GIB), "20.0 GB");
    }
}
