//! Segment decoding with an exact display-char -> segment-byte map (CORE-06, EDIT-10).
//!
//! Uses `slice::utf8_chunks` (never `from_utf8_lossy`) so invalid UTF-8 sequences and
//! control characters map cleanly to `·` while preserving exact byte offsets for selection
//! and caret hit-testing.

use std::ops::Range;

pub const NON_PRINTABLE: char = '\u{00B7}';
pub const TAB_CELLS: usize = 4;

pub struct Decoded {
    pub text: String,
    /// `byte_of_char[i]` is the segment-relative byte offset of display char `i`;
    /// one extra sentinel entry at the end equals the content end (before stripped EOL).
    pub byte_of_char: Vec<u32>,
    /// Display-char index ranges rendered in `theme::MUTED_DIM` (controls and invalid bytes).
    pub dim: Vec<Range<usize>>,
}

impl Decoded {
    pub fn char_count(&self) -> usize {
        self.byte_of_char.len().saturating_sub(1)
    }

    pub fn byte_for_char(&self, idx: usize) -> u32 {
        let last = self.byte_of_char.len().saturating_sub(1);
        self.byte_of_char[idx.min(last)]
    }
}

/// Decodes a segment's raw bytes into display text + char-to-byte map.
///
/// Strips one trailing `\n` (and a preceding `\r`), or a lone trailing `\r` when
/// `next_is_newline` is true (CRLF split across a hard break).
pub fn decode_segment(bytes: &[u8], next_is_newline: bool) -> Decoded {
    let trimmed = if let Some(rest) = bytes.strip_suffix(b"\n") {
        rest.strip_suffix(b"\r").unwrap_or(rest)
    } else if next_is_newline {
        bytes.strip_suffix(b"\r").unwrap_or(bytes)
    } else {
        bytes
    };

    let mut text = String::with_capacity(trimmed.len());
    let mut byte_of_char: Vec<u32> = Vec::with_capacity(trimmed.len() + 1);
    let mut dim: Vec<Range<usize>> = Vec::new();
    let mut byte_off = 0usize;

    for chunk in trimmed.utf8_chunks() {
        for c in chunk.valid().chars() {
            let b = u32::try_from(byte_off).unwrap_or(u32::MAX);
            if c == '\t' {
                for _ in 0..TAB_CELLS {
                    text.push(' ');
                    byte_of_char.push(b);
                }
            } else if c.is_control() {
                let ci = byte_of_char.len();
                text.push(NON_PRINTABLE);
                byte_of_char.push(b);
                push_dim(&mut dim, ci);
            } else {
                text.push(c);
                byte_of_char.push(b);
            }
            byte_off += c.len_utf8();
        }
        let inv = chunk.invalid();
        if !inv.is_empty() {
            let b = u32::try_from(byte_off).unwrap_or(u32::MAX);
            let ci = byte_of_char.len();
            text.push(NON_PRINTABLE);
            byte_of_char.push(b);
            push_dim(&mut dim, ci);
            byte_off += inv.len();
        }
    }

    byte_of_char.push(u32::try_from(trimmed.len()).unwrap_or(u32::MAX));
    Decoded {
        text,
        byte_of_char,
        dim,
    }
}

fn push_dim(dim: &mut Vec<Range<usize>>, idx: usize) {
    if let Some(last) = dim.last_mut()
        && last.end == idx
    {
        last.end = idx + 1;
        return;
    }
    dim.push(idx..idx + 1);
}

/// Inverse lookup: display-char index containing segment-relative byte `rel`.
///
/// Offsets inside a tab's expansion map to its first space; offsets at or past the
/// content end return `d.char_count()`.
pub fn char_for_byte(d: &Decoded, rel: u32) -> usize {
    let count = d.char_count();
    if count == 0 {
        return 0;
    }
    if rel >= d.byte_of_char[count] {
        return count;
    }
    // First index where byte_of_char[i] > rel, then step back to the first char sharing
    // the previous byte offset (handles multi-space tabs).
    let upper = d.byte_of_char[..count].partition_point(|&b| b <= rel);
    let mut idx = upper.saturating_sub(1);
    let target_b = d.byte_of_char[idx];
    while idx > 0 && d.byte_of_char[idx - 1] == target_b {
        idx -= 1;
    }
    idx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{XorShift, fixtures};

    #[test]
    fn ascii() {
        let d = decode_segment(b"ab\n", false);
        assert_eq!(d.text, "ab");
        assert_eq!(d.byte_of_char, vec![0, 1, 2]);
        assert!(d.dim.is_empty());
        assert_eq!(d.char_count(), 2);
        assert_eq!(d.byte_for_char(99), 2);
    }

    #[test]
    fn crlf() {
        let d = decode_segment(b"ab\r\n", false);
        assert_eq!(d.text, "ab");
        assert_eq!(d.byte_of_char, vec![0, 1, 2]);

        let split = decode_segment(b"ab\r", true);
        assert_eq!(split.text, "ab");
        assert_eq!(split.byte_of_char, vec![0, 1, 2]);

        let keep = decode_segment(b"ab\r", false);
        assert_eq!(keep.text, "ab\u{00B7}");
    }

    #[test]
    fn tab() {
        let d = decode_segment(b"a\tb", false);
        assert_eq!(d.text, "a    b");
        assert_eq!(d.byte_of_char, vec![0, 1, 1, 1, 1, 2, 3]);
        assert_eq!(char_for_byte(&d, 1), 1);
    }

    #[test]
    fn controls() {
        let d = decode_segment(b"\x01x\x7F", false);
        assert_eq!(d.text, "\u{00B7}x\u{00B7}");
        assert_eq!(d.dim, vec![0..1, 2..3]);
    }

    #[test]
    fn invalid() {
        let d = decode_segment(b"ok \xFF\xFE z", false);
        assert_eq!(d.text, "ok \u{00B7}\u{00B7} z");
        assert_eq!(d.byte_of_char[3], 3);
        assert_eq!(d.byte_of_char[4], 4);
        assert_eq!(d.dim, vec![3..5]);
    }

    #[test]
    fn multibyte() {
        let d = decode_segment("é日".as_bytes(), false);
        assert_eq!(d.text, "é日");
        assert_eq!(d.byte_of_char, vec![0, 2, 5]);
        assert_eq!(char_for_byte(&d, 0), 0);
        assert_eq!(char_for_byte(&d, 1), 0);
        assert_eq!(char_for_byte(&d, 2), 1);
        assert_eq!(char_for_byte(&d, 4), 1);
        assert_eq!(char_for_byte(&d, 5), 2);
    }

    #[test]
    fn map_monotone() {
        let mut rng = XorShift::new(321);
        for _ in 0..200 {
            let len = rng.below(512) as usize;
            let bytes = fixtures::random_text(&mut rng, len, 0);
            let d = decode_segment(&bytes, false);
            assert_eq!(d.byte_of_char.len(), d.text.chars().count() + 1);
            for w in d.byte_of_char.windows(2) {
                assert!(w[0] <= w[1]);
            }
        }
    }
}
