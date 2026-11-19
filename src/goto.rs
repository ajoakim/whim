//! Go-to box (Ctrl+G) grammar, target resolution and live preview text (D-11, CORE-05, HEX-03).

use crate::lineindex::{Exactness, LineIndex};
use crate::source::{SourceError, TextSource};

pub const INVALID_TEXT: &str = "Not a valid position";

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GotoTarget {
    /// 1-based line number.
    Line(u64),
    Offset(u64),
    Percent(f64),
}

#[derive(Debug, PartialEq, Eq)]
pub struct Invalid;

/// Parses a Ctrl+G query.
///
/// - `0x1F00` / `0X1f00` -> [`GotoTarget::Offset`]
/// - `#8192` -> [`GotoTarget::Offset`]
/// - `50%` / `12.5%` (in `0.0..=100.0`) -> [`GotoTarget::Percent`]
/// - `1234` -> [`GotoTarget::Line`] in text mode (must be `>= 1`), [`GotoTarget::Offset`] in hex mode
pub fn parse_goto(input: &str, hex_mode: bool) -> Result<GotoTarget, Invalid> {
    let s = input.trim();
    if s.is_empty() {
        return Err(Invalid);
    }

    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        if hex.is_empty() || hex.len() > 16 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(Invalid);
        }
        let v = u64::from_str_radix(hex, 16).map_err(|_| Invalid)?;
        return Ok(GotoTarget::Offset(v));
    }

    if let Some(dec) = s.strip_prefix('#') {
        if dec.is_empty() || !dec.bytes().all(|b| b.is_ascii_digit()) {
            return Err(Invalid);
        }
        let v = dec.parse::<u64>().map_err(|_| Invalid)?;
        return Ok(GotoTarget::Offset(v));
    }

    if let Some(pct) = s.strip_suffix('%') {
        if pct.is_empty()
            || !pct.bytes().all(|b| b.is_ascii_digit() || b == b'.')
            || pct.bytes().filter(|&b| b == b'.').count() > 1
            || pct == "."
        {
            return Err(Invalid);
        }
        let v = pct.parse::<f64>().map_err(|_| Invalid)?;
        if !v.is_finite() || !(0.0..=100.0).contains(&v) {
            return Err(Invalid);
        }
        return Ok(GotoTarget::Percent(v));
    }

    if !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Invalid);
    }
    let v = s.parse::<u64>().map_err(|_| Invalid)?;
    if hex_mode {
        Ok(GotoTarget::Offset(v))
    } else if v == 0 {
        Err(Invalid)
    } else {
        Ok(GotoTarget::Line(v))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Resolved {
    pub offset: u64,
    pub exactness: Exactness,
    pub clamped: bool,
}

pub fn resolve(
    target: GotoTarget,
    src: &dyn TextSource,
    index: &LineIndex,
) -> Result<Resolved, SourceError> {
    let len = src.len();
    if len == 0 {
        return Ok(Resolved {
            offset: 0,
            exactness: Exactness::Exact,
            clamped: false,
        });
    }
    let last = len - 1;
    match target {
        GotoTarget::Offset(off) => {
            let clamped = off >= len;
            Ok(Resolved {
                offset: off.min(last),
                exactness: Exactness::Exact,
                clamped,
            })
        }
        GotoTarget::Percent(p) => {
            let scaled = (p * 1000.0).round() as u128;
            let raw = (u128::from(len) * scaled) / 100_000;
            let off = (raw as u64).min(last);
            Ok(Resolved {
                offset: off,
                exactness: Exactness::Exact,
                clamped: false,
            })
        }
        GotoTarget::Line(line1) => {
            let line0 = line1.saturating_sub(1);
            let (total, total_ex) = index.total_lines();
            let clamped = total_ex == Exactness::Exact && line1 > total;
            let (off, exactness) = index.offset_of(src, line0)?;
            Ok(Resolved {
                offset: off.min(last),
                exactness,
                clamped,
            })
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Preview {
    pub text: String,
    pub valid: bool,
}

pub fn preview(
    input: &str,
    hex_mode: bool,
    len: u64,
    total_lines: (u64, Exactness),
) -> Preview {
    let Ok(target) = parse_goto(input, hex_mode) else {
        return Preview {
            text: INVALID_TEXT.to_owned(),
            valid: false,
        };
    };

    let text = match target {
        GotoTarget::Line(n) => {
            let (total, ex) = total_lines;
            if ex == Exactness::Exact && n > total {
                "Past end, will go to last line".to_owned()
            } else if ex == Exactness::Estimated {
                format!("Line {} (estimated, ~)", thousands(n))
            } else {
                format!("Line {}", thousands(n))
            }
        }
        GotoTarget::Offset(off) => {
            if len > 0 && off >= len {
                "Past end, will go to last byte".to_owned()
            } else {
                format!("Byte {} (0x{off:X})", thousands(off))
            }
        }
        GotoTarget::Percent(p) => {
            if (p.fract()).abs() < 1e-9 {
                format!("{}% of file", p as u64)
            } else {
                format!("{p}% of file")
            }
        }
    };
    Preview { text, valid: true }
}

pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, &b) in bytes.iter().enumerate() {
        let rem = bytes.len() - i;
        if i > 0 && rem.is_multiple_of(3) {
            out.push(',');
        }
        out.push(b as char);
    }
    out
}

pub fn hint(hex_mode: bool) -> &'static str {
    if hex_mode {
        "Offset, 0xOffset, #Offset or %"
    } else {
        "Line, 0xOffset, #Offset or %"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lineindex::{IndexMsg, spawn_indexer};
    use crate::source::MemSource;
    use std::time::Duration;

    fn complete_index(bytes: &[u8]) -> (MemSource, LineIndex) {
        let mut idx = LineIndex::new(bytes.len() as u64, bytes);
        let job = spawn_indexer(1, Box::new(MemSource::new(bytes.to_vec())), Box::new(|| {}));
        while let Ok(m) = job.rx.recv_timeout(Duration::from_secs(5)) {
            let done = matches!(m, IndexMsg::Done { .. });
            idx.apply(&m);
            if done {
                break;
            }
        }
        (MemSource::new(bytes.to_vec()), idx)
    }

    #[test]
    fn parse_valid_inputs() {
        assert_eq!(parse_goto("1234", false), Ok(GotoTarget::Line(1234)));
        assert_eq!(parse_goto("1234", true), Ok(GotoTarget::Offset(1234)));
        assert_eq!(parse_goto("0", true), Ok(GotoTarget::Offset(0)));
        assert_eq!(parse_goto("0x1F00", false), Ok(GotoTarget::Offset(7936)));
        assert_eq!(parse_goto("0X1f00", true), Ok(GotoTarget::Offset(7936)));
        assert_eq!(parse_goto("#8192", false), Ok(GotoTarget::Offset(8192)));
        assert_eq!(parse_goto("50%", false), Ok(GotoTarget::Percent(50.0)));
        assert_eq!(parse_goto("12.5%", false), Ok(GotoTarget::Percent(12.5)));
        assert_eq!(parse_goto(" 42 ", false), Ok(GotoTarget::Line(42)));
    }

    #[test]
    fn parse_invalid_inputs() {
        for bad in [
            "",
            "   ",
            "0",
            "abc",
            "0x",
            "#",
            "101%",
            "-5",
            "1.5",
            "0xZZ",
            "99999999999999999999999",
            "%",
            ".%",
        ] {
            assert_eq!(parse_goto(bad, false), Err(Invalid), "input={bad:?}");
        }
    }

    #[test]
    fn resolve_offsets_and_percents() {
        let (src, idx) = complete_index(&[b'a'; 200]);
        assert_eq!(
            resolve(GotoTarget::Offset(250), &src, &idx).unwrap(),
            Resolved {
                offset: 199,
                exactness: Exactness::Exact,
                clamped: true
            }
        );
        assert_eq!(
            resolve(GotoTarget::Percent(100.0), &src, &idx).unwrap().offset,
            199
        );
        assert_eq!(
            resolve(GotoTarget::Percent(50.0), &src, &idx).unwrap().offset,
            100
        );
        assert_eq!(
            resolve(GotoTarget::Line(1), &src, &idx).unwrap().offset,
            0
        );
    }

    #[test]
    fn resolve_lines_with_complete_index() {
        let (src, idx) = complete_index(b"a\nb\nc");
        assert_eq!(
            resolve(GotoTarget::Line(2), &src, &idx).unwrap(),
            Resolved {
                offset: 2,
                exactness: Exactness::Exact,
                clamped: false
            }
        );
        assert_eq!(
            resolve(GotoTarget::Line(3), &src, &idx).unwrap(),
            Resolved {
                offset: 4,
                exactness: Exactness::Exact,
                clamped: false
            }
        );
        assert_eq!(
            resolve(GotoTarget::Line(9), &src, &idx).unwrap(),
            Resolved {
                offset: 4,
                exactness: Exactness::Exact,
                clamped: true
            }
        );
    }

    #[test]
    fn resolve_empty_file() {
        let (src, idx) = complete_index(b"");
        assert_eq!(
            resolve(GotoTarget::Offset(10), &src, &idx).unwrap().offset,
            0
        );
    }

    #[test]
    fn preview_copy() {
        assert_eq!(
            preview("1234", false, 50_000, (5_000, Exactness::Exact)),
            Preview {
                text: "Line 1,234".into(),
                valid: true
            }
        );
        assert_eq!(
            preview("#8192", false, 50_000, (5_000, Exactness::Exact)),
            Preview {
                text: "Byte 8,192 (0x2000)".into(),
                valid: true
            }
        );
        assert_eq!(
            preview("50%", false, 50_000, (5_000, Exactness::Exact)),
            Preview {
                text: "50% of file".into(),
                valid: true
            }
        );
        assert_eq!(
            preview("5000000", false, 50_000, (5_000, Exactness::Estimated)),
            Preview {
                text: "Line 5,000,000 (estimated, ~)".into(),
                valid: true
            }
        );
        assert_eq!(
            preview("6000", false, 50_000, (5_000, Exactness::Exact)),
            Preview {
                text: "Past end, will go to last line".into(),
                valid: true
            }
        );
        assert_eq!(
            preview("99999", true, 50_000, (5_000, Exactness::Exact)),
            Preview {
                text: "Past end, will go to last byte".into(),
                valid: true
            }
        );
        assert_eq!(
            preview("nope", false, 50_000, (5_000, Exactness::Exact)),
            Preview {
                text: "Not a valid position".into(),
                valid: false
            }
        );
    }

    #[test]
    fn thousands_formatting() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn hint_strings() {
        assert_eq!(hint(false), "Line, 0xOffset, #Offset or %");
        assert_eq!(hint(true), "Offset, 0xOffset, #Offset or %");
    }
}
