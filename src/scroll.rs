//! Stateless paragraph (segment) boundary rule with exact inverse stepping (D-04, CORE-03, CORE-06).
//!
//! Boundaries are `{0, len} ∪ {n + 1 : data[n] == '\n'} ∪ {adj(p) : p % CAP == 0, 0 < p < len,
//! no '\n' in [p - CAP, p)}`, where `adj(p)` advances past at most 3 UTF-8 continuation bytes
//! (`0x80..=0xBF`), capped at `len`. Because the rule depends only on absolute offsets and a
//! bounded window (`<= 3 * CAP + 8` bytes), forward and backward steps are exact inverses and
//! multi-GB single lines scroll in O(CAP) bytes per step.

use crate::source::{SourceError, TextSource, read_clamped};

pub const CAP: u64 = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    pub start: u64,
    pub end: u64,
    pub newline_end: bool,
    pub hard_continuation: bool,
}

fn is_utf8_cont(b: u8) -> bool {
    (b & 0xC0) == 0x80
}

/// Adjust `p` forward past at most 3 UTF-8 continuation bytes, capped at `len`.
/// `win` covers `[win_start, win_start + win.len())` and must contain `[p, min(len, p + 3))`.
fn adj_in_window(p: u64, len: u64, win_start: u64, win: &[u8]) -> u64 {
    let mut k = 0u64;
    while k < 3 && p + k < len {
        let idx = (p + k - win_start) as usize;
        match win.get(idx) {
            Some(&b) if is_utf8_cont(b) => k += 1,
            _ => break,
        }
    }
    (p + k).min(len)
}

pub(crate) fn next_boundary_cap(
    src: &dyn TextSource,
    s: u64,
    cap: u64,
) -> Result<u64, SourceError> {
    let len = src.len();
    if s >= len {
        return Ok(len);
    }

    // We need:
    // - backward context down to (first aligned q > s) - cap, which is >= s.saturating_sub(cap)
    // - forward context up to s + 2 * cap + 4 (enough for any hard break + 3 cont bytes).
    let win_start = s.saturating_sub(cap);
    let win_end = s.saturating_add(2 * cap + 4).min(len);
    let win_len = usize::try_from(win_end - win_start).map_err(|_| SourceError::Changed)?;
    let win = read_clamped(src, win_start, win_len)?;
    let s_rel = (s - win_start) as usize;

    // Nearest newline boundary at or after s.
    let nl_bound = match memchr::memchr(b'\n', &win[s_rel..]) {
        Some(k) => s + k as u64 + 1,
        None => win_end,
    };

    // Check aligned hard-break candidates q > s with q < len and q <= nl_bound.
    // The first multiple of cap strictly greater than s is ((s / cap) + 1) * cap.
    let mut q = (s / cap + 1).saturating_mul(cap);
    while q < len && q <= nl_bound {
        // Test if [q - cap, q) is newline-free.
        let check_start = (q - cap - win_start) as usize;
        let check_end = (q - win_start) as usize;
        if check_end <= win.len() && memchr::memchr(b'\n', &win[check_start..check_end]).is_none() {
            let a = adj_in_window(q, len, win_start, &win);
            if a > s {
                return Ok(a.min(nl_bound));
            }
        }
        q = match q.checked_add(cap) {
            Some(next) => next,
            None => break,
        };
    }

    Ok(nl_bound)
}

pub(crate) fn segment_start_cap(
    src: &dyn TextSource,
    a: u64,
    cap: u64,
) -> Result<u64, SourceError> {
    let len = src.len();
    let a = a.min(len);
    if a == 0 {
        return Ok(0);
    }
    if a == len {
        return Ok(len);
    }

    // Look back up to 3 * cap bytes so we cover [p - cap, p) for up to two multiples below a,
    // plus 3 bytes past a for adj(p) when p == a.
    let win_start = a.saturating_sub(3 * cap);
    let win_end = a.saturating_add(4).min(len);
    let win_len = usize::try_from(win_end - win_start).map_err(|_| SourceError::Changed)?;
    let win = read_clamped(src, win_start, win_len)?;
    let a_rel = (a - win_start) as usize;

    // Last '\n' strictly before a gives newline boundary n1 = pos + 1 (or win_start if none).
    let nl_rel = memchr::memrchr(b'\n', &win[..a_rel]);
    let n1 = match nl_rel {
        Some(k) => win_start + k as u64 + 1,
        None => win_start,
    };

    // Candidates p = k * cap <= a. If p - cap >= n1 (or when no newline was found in window,
    // p - cap >= win_start), check that [p - cap, p) has no '\n' and adj(p) <= a.
    let mut p = (a / cap) * cap;
    while p > 0 && p >= n1.saturating_add(cap) {
        if p < len {
            let check_start = (p - cap - win_start) as usize;
            let check_end = (p - win_start) as usize;
            if check_end <= win.len()
                && memchr::memchr(b'\n', &win[check_start..check_end]).is_none()
            {
                let adj = adj_in_window(p, len, win_start, &win);
                if adj <= a {
                    return Ok(adj);
                }
            }
        }
        p = p.saturating_sub(cap);
    }

    // No hard break qualifies; if a newline was found in the window, return n1.
    // Otherwise win_start == 0 (if win_start > 0, there were >= 3*cap bytes with no '\n',
    // so at least one aligned multiple p with p - cap >= win_start must have qualified above).
    Ok(n1)
}

pub fn next_boundary(src: &dyn TextSource, s: u64) -> Result<u64, SourceError> {
    next_boundary_cap(src, s, CAP)
}

pub fn segment_start(src: &dyn TextSource, a: u64) -> Result<u64, SourceError> {
    segment_start_cap(src, a, CAP)
}

pub fn prev_segment_start(src: &dyn TextSource, start: u64) -> Result<Option<u64>, SourceError> {
    if start == 0 {
        return Ok(None);
    }
    let prev = segment_start(src, start - 1)?;
    Ok(Some(prev))
}

pub fn segment_at(src: &dyn TextSource, start: u64) -> Result<Segment, SourceError> {
    let end = next_boundary(src, start)?;
    let newline_end = if end > start {
        let mut last = [0u8; 1];
        src.read_range(end - 1, &mut last)?;
        last[0] == b'\n'
    } else {
        false
    };
    let hard_continuation = if start == 0 {
        false
    } else {
        let mut prev = [0u8; 1];
        src.read_range(start - 1, &mut prev)?;
        prev[0] != b'\n'
    };
    Ok(Segment {
        start,
        end,
        newline_end,
        hard_continuation,
    })
}

pub fn last_segment_start(src: &dyn TextSource) -> Result<u64, SourceError> {
    let len = src.len();
    if len == 0 {
        Ok(0)
    } else {
        segment_start(src, len - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemSource;
    use crate::testutil::{XorShift, fixtures};

    fn walk_boundaries(src: &dyn TextSource, cap: u64) -> Vec<u64> {
        let len = src.len();
        let mut out = vec![0];
        let mut cur = 0;
        while cur < len {
            let next = next_boundary_cap(src, cur, cap).unwrap();
            assert!(next > cur, "must advance: cur={cur} next={next}");
            assert!(next <= len);
            out.push(next);
            cur = next;
        }
        out
    }

    fn verify_inverse(data: &[u8], cap: u64, exhaustive: bool) {
        let src = MemSource::new(data.to_vec());
        let len = data.len() as u64;
        let b = walk_boundaries(&src, cap);

        for &pos in &b {
            assert_eq!(
                segment_start_cap(&src, pos, cap).unwrap(),
                pos,
                "boundary {pos} not fixed point (len={len}, cap={cap})"
            );
            if pos < len {
                assert!(
                    !is_utf8_cont(data[pos as usize]),
                    "boundary {pos} landed on UTF-8 continuation byte"
                );
            }
        }
        for w in b.windows(2) {
            assert!(
                w[1] - w[0] < 2 * cap + 4,
                "segment too long: {}..{} (cap={cap})",
                w[0],
                w[1]
            );
            if w[0] > 0 {
                let prev = segment_start_cap(&src, w[1] - 1, cap).unwrap();
                assert_eq!(prev, w[0]);
            }
        }
        for i in 1..b.len() {
            let prev = segment_start_cap(&src, b[i] - 1, cap).unwrap();
            assert_eq!(prev, b[i - 1], "prev of B[{i}]={} expected {}", b[i], b[i - 1]);
        }

        if exhaustive {
            for x in 0..len {
                let expected = *b.iter().rfind(|&&v| v <= x).unwrap();
                assert_eq!(
                    segment_start_cap(&src, x, cap).unwrap(),
                    expected,
                    "segment_start({x}) mismatch"
                );
            }
        }
    }

    #[test]
    fn inverse_small_cap() {
        let mut rng = XorShift::new(777);
        let rates = [0u32, 1, 20];
        for i in 0..300 {
            let len = if i == 0 {
                0
            } else {
                rng.below(4_000) as usize
            };
            let data = fixtures::random_text(&mut rng, len, rates[i % rates.len()]);
            verify_inverse(&data, 16, i < 40);
        }
    }

    #[test]
    fn inverse_real_cap() {
        let mut rng = XorShift::new(888);
        for i in 0..20 {
            let len = 30_000 + rng.below(90_000) as usize;
            let rate = if i == 0 { 0 } else { (i % 5) as u32 };
            let data = fixtures::random_text(&mut rng, len, rate);
            verify_inverse(&data, CAP, false);
        }
        // 3-byte CJK chars straddling multiples of CAP.
        let cjk: Vec<u8> = "日".repeat(12_000).into_bytes();
        verify_inverse(&cjk, CAP, false);
    }

    #[test]
    fn segment_bounds() {
        let cap = 32u64;
        // Line <= cap bytes is never broken.
        let short = format!("{}\n{}", "a".repeat(30), "b".repeat(10));
        let src = MemSource::new(short.into_bytes());
        assert_eq!(walk_boundaries(&src, cap), vec![0, 31, 41]);

        // Line > 2*cap bytes is always broken.
        let long = fixtures::long_line(200);
        let src = MemSource::new(long);
        let b = walk_boundaries(&src, cap);
        assert!(b.len() > 3);
        for w in b.windows(2) {
            assert!(w[1] - w[0] < 2 * cap + 4);
        }
    }

    #[test]
    fn crlf_boundary() {
        // Place \r\n straddling a multiple of cap=16: bytes 0..15 = 'a', 15 = '\r', 16 = '\n'.
        let mut data = vec![b'a'; 15];
        data.push(b'\r');
        data.push(b'\n');
        data.extend_from_slice(b"next");
        verify_inverse(&data, 16, true);
        let src = MemSource::new(data);
        let b = walk_boundaries(&src, 16);
        assert!(b.contains(&17), "boundary after LF must exist: {b:?}");
    }

    #[test]
    fn long_line_is_o1() {
        let data = vec![b'x'; 50 * 1024 * 1024];
        let src = MemSource::new(data);
        src.reset_counter();

        let mut pos = 0u64;
        for _ in 0..1_000 {
            pos = next_boundary(&src, pos).unwrap();
        }
        for _ in 0..1_000 {
            pos = prev_segment_start(&src, pos).unwrap().unwrap();
        }
        assert_eq!(pos, 0);
        assert!(
            src.bytes_read() / 2_000 <= 3 * CAP + 8,
            "avg bytes/step = {}",
            src.bytes_read() / 2_000
        );
    }

    #[test]
    fn last_segment() {
        let empty = MemSource::new(Vec::new());
        assert_eq!(last_segment_start(&empty).unwrap(), 0);

        let src = MemSource::new(b"aa\nbb\ncc".to_vec());
        let b = walk_boundaries(&src, CAP);
        assert_eq!(last_segment_start(&src).unwrap(), b[b.len() - 2]);
    }

    #[test]
    fn segment_flags() {
        let src = MemSource::new(b"abc\ndef".to_vec());
        let s0 = segment_at(&src, 0).unwrap();
        assert_eq!(
            s0,
            Segment {
                start: 0,
                end: 4,
                newline_end: true,
                hard_continuation: false,
            }
        );
        let s1 = segment_at(&src, 4).unwrap();
        assert_eq!(
            s1,
            Segment {
                start: 4,
                end: 7,
                newline_end: false,
                hard_continuation: false,
            }
        );

        let long = MemSource::new(fixtures::long_line(25_000));
        let first = segment_at(&long, 0).unwrap();
        assert!(!first.newline_end);
        assert!(!first.hard_continuation);
        let cont = segment_at(&long, first.end).unwrap();
        assert!(cont.hard_continuation);
    }
}
