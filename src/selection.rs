//! Offset-based selection model and bounded double-click word bounds (D-13, D-14, EDIT-10).

use crate::source::{SourceError, TextSource, read_clamped};
use std::ops::Range;

pub const WORD_SCAN: u64 = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub anchor: u64,
    pub head: u64,
}

impl Selection {
    pub fn caret(off: u64) -> Self {
        Self {
            anchor: off,
            head: off,
        }
    }

    pub fn range(&self) -> Range<u64> {
        let a = self.anchor.min(self.head);
        let b = self.anchor.max(self.head);
        a..b
    }

    pub fn len(&self) -> u64 {
        self.anchor.abs_diff(self.head)
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn extend_to(&mut self, off: u64) {
        self.head = off;
    }

    pub fn select_all(len: u64) -> Self {
        Self {
            anchor: 0,
            head: len,
        }
    }

    pub fn clamp(&mut self, len: u64) {
        self.anchor = self.anchor.min(len);
        self.head = self.head.min(len);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CharClass {
    Word,
    Space,
    Other,
}

pub fn class_of(c: char) -> CharClass {
    if c.is_alphanumeric() || c == '_' {
        CharClass::Word
    } else if c.is_whitespace() {
        CharClass::Space
    } else {
        CharClass::Other
    }
}

/// Computes double-click word bounds around `off` by reading at most `WORD_SCAN` bytes on each
/// side through `src`. Never returns a boundary inside a UTF-8 character.
pub fn word_bounds(src: &dyn TextSource, off: u64) -> Result<Range<u64>, SourceError> {
    let len = src.len();
    if len == 0 {
        return Ok(0..0);
    }

    let target = off.min(len);
    let win_start = target.saturating_sub(WORD_SCAN);
    let win_end = target.saturating_add(WORD_SCAN).min(len);
    let win_len = usize::try_from(win_end - win_start).map_err(|_| SourceError::Changed)?;
    let bytes = read_clamped(src, win_start, win_len)?;

    // Advance past up to 3 leading UTF-8 continuation bytes so decoding starts on a char boundary.
    let mut skip = 0usize;
    while skip < 3 && skip < bytes.len() && (bytes[skip] & 0xC0) == 0x80 {
        skip += 1;
    }
    let slice = &bytes[skip..];
    let base = win_start + skip as u64;

    // Collect (abs_start, abs_end, class) for each decoded token in the window.
    let mut tokens: Vec<(u64, u64, CharClass)> = Vec::with_capacity(slice.len());
    let mut rel = 0u64;
    for chunk in slice.utf8_chunks() {
        for c in chunk.valid().chars() {
            let w = c.len_utf8() as u64;
            tokens.push((base + rel, base + rel + w, class_of(c)));
            rel += w;
        }
        for _ in chunk.invalid() {
            tokens.push((base + rel, base + rel + 1, CharClass::Other));
            rel += 1;
        }
    }

    if tokens.is_empty() {
        return Ok(target..target);
    }

    // Pick token containing `target`, or the last token when `target == len`.
    let idx = if target >= tokens[tokens.len() - 1].1 {
        tokens.len() - 1
    } else {
        tokens
            .iter()
            .position(|&(s, e, _)| target >= s && target < e)
            .unwrap_or(0)
    };

    let cls = tokens[idx].2;
    let mut lo = idx;
    while lo > 0 && tokens[lo - 1].2 == cls {
        lo -= 1;
    }
    let mut hi = idx;
    while hi + 1 < tokens.len() && tokens[hi + 1].2 == cls {
        hi += 1;
    }
    Ok(tokens[lo].0..tokens[hi].1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemSource;

    #[test]
    fn caret_and_range() {
        let c = Selection::caret(5);
        assert_eq!(c.anchor, 5);
        assert_eq!(c.head, 5);
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);

        let mut s = Selection {
            anchor: 10,
            head: 3,
        };
        assert_eq!(s.range(), 3..10);
        assert_eq!(s.len(), 7);
        s.extend_to(20);
        assert_eq!(s.anchor, 10);
        assert_eq!(s.head, 20);
        assert_eq!(s.range(), 10..20);
    }

    #[test]
    fn select_all_and_clamp() {
        let gib20 = 20u64 * 1024 * 1024 * 1024;
        let mut s = Selection::select_all(gib20);
        assert_eq!(s.range(), 0..gib20);
        s.clamp(100);
        assert_eq!(s.range(), 0..100);
    }

    #[test]
    fn word_bounds_ascii() {
        let src = MemSource::new(b"hello world_x, ok".to_vec());
        // 'w' is at byte 6, "world_x" is 6..13
        assert_eq!(word_bounds(&src, 6).unwrap(), 6..13);
        // space at byte 5
        assert_eq!(word_bounds(&src, 5).unwrap(), 5..6);
        // ',' at byte 13
        assert_eq!(word_bounds(&src, 13).unwrap(), 13..14);
    }

    #[test]
    fn word_bounds_cjk_and_utf8() {
        let s = "é日本語 x";
        let src = MemSource::new(s.as_bytes().to_vec());
        let hon = s.find('本').unwrap() as u64;
        let space = s.find(' ').unwrap() as u64;
        // 'é' + "日本語" are all alphanumeric => 0..space
        assert_eq!(word_bounds(&src, hon).unwrap(), 0..space);
        // Middle byte of '本' still snaps to full char boundaries.
        let r = word_bounds(&src, hon + 1).unwrap();
        assert_eq!(r, 0..space);
        assert!(s.is_char_boundary(r.start as usize));
        assert!(s.is_char_boundary(r.end as usize));
    }

    #[test]
    fn word_bounds_at_end_and_empty() {
        let src = MemSource::new(b"abc".to_vec());
        assert_eq!(word_bounds(&src, 3).unwrap(), 0..3);

        let empty = MemSource::new(Vec::new());
        assert_eq!(word_bounds(&empty, 0).unwrap(), 0..0);
    }

    #[test]
    fn word_bounds_is_bounded_on_huge_word() {
        let src = MemSource::new(vec![b'a'; 1024 * 1024]);
        src.reset_counter();
        let r = word_bounds(&src, 500_000).unwrap();
        assert!(src.bytes_read() <= 512 + 8, "read={}", src.bytes_read());
        assert!(r.end - r.start <= 512);
    }

    #[test]
    fn word_bounds_window_never_splits_utf8() {
        // Fill with 3-byte chars so window start (off - 256) lands inside a char.
        let text = "日".repeat(400);
        let src = MemSource::new(text.as_bytes().to_vec());
        let off = 600;
        let r = word_bounds(&src, off).unwrap();
        assert!(text.is_char_boundary(r.start as usize));
        assert!(text.is_char_boundary(r.end as usize));
    }

    #[test]
    fn class_of_variants() {
        assert_eq!(class_of('a'), CharClass::Word);
        assert_eq!(class_of('_'), CharClass::Word);
        assert_eq!(class_of('日'), CharClass::Word);
        assert_eq!(class_of(' '), CharClass::Space);
        assert_eq!(class_of('\t'), CharClass::Space);
        assert_eq!(class_of(','), CharClass::Other);
    }
}
