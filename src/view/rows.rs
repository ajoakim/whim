//! Byte-offset row model for the virtualized view (pure, no egui).

// IMPL_PLACEHOLDER

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
