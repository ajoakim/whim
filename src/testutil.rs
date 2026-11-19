//! Test-only helpers: unique temp dirs, a tiny PRNG and deterministic fixtures.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique directory under the system temp dir, removed on drop.
///
/// Created with an exclusive single-directory create (not a recursive create) so a
/// pre-existing path is an error rather than silently reused or followed (Phase 1 WR-05).
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new() -> Self {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "whim-test-{}-{n}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir(&path).expect("create unique temp dir");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// Writes `bytes` to `name` inside the directory and returns its path.
    pub fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, bytes).expect("write fixture file");
        p
    }
}

impl Default for TempDir {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Deterministic xorshift64* PRNG for fixture generation.
pub struct XorShift(u64);

impl XorShift {
    pub fn new(seed: u64) -> Self {
        Self(if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed })
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        assert!(n > 0, "below(0) has no values");
        self.next_u64() % n
    }

    pub fn chance(&mut self, pct: u32) -> bool {
        self.below(100) < u64::from(pct.min(100))
    }
}

pub mod fixtures {
    use super::XorShift;

    /// `line 0\r\nline 1\r\n...` with no trailing newline after the last line.
    pub fn crlf_lines(n: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for i in 0..n {
            if i > 0 {
                out.extend_from_slice(b"\r\n");
            }
            out.extend_from_slice(format!("line {i}").as_bytes());
        }
        out
    }

    /// `0\n1\n2\n...` with no trailing newline after the last line.
    pub fn tiny_lines(n: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for i in 0..n {
            if i > 0 {
                out.push(b'\n');
            }
            out.extend_from_slice(format!("{i}").as_bytes());
        }
        out
    }

    /// Repeating ASCII with no newline at all.
    pub fn long_line(len: usize) -> Vec<u8> {
        const PAT: &[u8] = b"abcdefghij";
        (0..len).map(|i| PAT[i % PAT.len()]).collect()
    }

    pub fn invalid_utf8() -> Vec<u8> {
        b"ok \xFF\xFE bad \xC3 end\n".to_vec()
    }

    /// Japanese, Chinese and Korean text, one numbered line per entry.
    pub fn cjk_lines(n: usize) -> Vec<u8> {
        let mut out = Vec::new();
        for i in 0..n {
            if i > 0 {
                out.push(b'\n');
            }
            out.extend_from_slice(format!("日本語のテキスト 中文 한국어 {i}").as_bytes());
        }
        out
    }

    /// Mixed ASCII, `é`, `日`, tabs and newlines. Multi-byte chars are never split
    /// (except when the length budget ends exactly inside one).
    pub fn random_text(rng: &mut XorShift, len: usize, newline_pct: u32) -> Vec<u8> {
        const CHARS: &[&[u8]] = &[
            b"a", b"b", b"c", b"x", b"y", b"z", b"0", b"9", b" ", b"\t", b"\xC3\xA9",
            b"\xE6\x97\xA5",
        ];
        let mut out = Vec::with_capacity(len);
        while out.len() < len {
            if rng.chance(newline_pct) {
                out.push(b'\n');
                continue;
            }
            let c = CHARS[rng.below(CHARS.len() as u64) as usize];
            if out.len() + c.len() > len {
                break;
            }
            out.extend_from_slice(c);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_dirs_are_unique() {
        let a = TempDir::new();
        let b = TempDir::new();
        assert_ne!(a.path(), b.path());
        assert!(a.path().is_dir());
    }

    #[test]
    fn fixtures_are_deterministic() {
        let mut a = XorShift::new(42);
        let mut b = XorShift::new(42);
        assert_eq!(
            fixtures::random_text(&mut a, 500, 10),
            fixtures::random_text(&mut b, 500, 10)
        );
        assert_eq!(fixtures::tiny_lines(3), b"0\n1\n2");
        assert_eq!(fixtures::crlf_lines(2), b"line 0\r\nline 1");
        assert_eq!(fixtures::long_line(12), b"abcdefghijab");
    }
}
