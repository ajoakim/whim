//! Read-only content access (D-01, D-02).
//!
//! `FileSource` (plan 02-03) implements this over positional reads; the Phase 3 piece
//! table will implement the same interface. Views and algorithms never hold a whole-file
//! slice, so a truncated or replaced file surfaces as a recoverable error.

use std::sync::atomic::{AtomicU64, Ordering};

/// Why a read failed.
#[derive(Debug)]
pub enum SourceError {
    /// The requested range is outside the source, or the file changed on disk.
    Changed,
    Io(std::io::Error),
}

impl SourceError {
    pub fn is_changed(&self) -> bool {
        matches!(self, Self::Changed)
    }
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Changed => f.write_str("file changed on disk"),
            Self::Io(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for SourceError {}

impl From<std::io::Error> for SourceError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// A read-only, randomly addressable byte source.
pub trait TextSource: Send + Sync {
    /// Length in bytes.
    fn len(&self) -> u64;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Bumped on every content change. `FileSource` returns 0 (read-only, Pitfall 11).
    fn version(&self) -> u64;

    /// Fill all of `buf` with bytes `[start, start + buf.len())`.
    ///
    /// Returns [`SourceError::Changed`] if the range is not inside [`TextSource::len`]
    /// (checked, never panics) or the underlying data changed.
    fn read_range(&self, start: u64, buf: &mut [u8]) -> Result<(), SourceError>;
}

/// Reads `min(max, len - start)` bytes at `start` (empty `Vec` if `start >= len`).
pub fn read_clamped(src: &dyn TextSource, start: u64, max: usize) -> Result<Vec<u8>, SourceError> {
    let len = src.len();
    if start >= len {
        return Ok(Vec::new());
    }
    let avail = len - start;
    let n = avail.min(max as u64) as usize;
    let mut buf = vec![0u8; n];
    src.read_range(start, &mut buf)?;
    Ok(buf)
}

/// Line-ending style detected in a sample (display-only until Phase 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EolKind {
    Lf,
    Crlf,
    Mixed,
    Unknown,
}

impl EolKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lf => "LF",
            Self::Crlf => "CRLF",
            Self::Mixed => "Mixed",
            Self::Unknown => "LF",
        }
    }
}

/// In-memory source for tests and small buffers.
///
/// Counts bytes handed out so tests can assert that algorithms never read whole files
/// (CORE-02).
pub struct MemSource {
    data: Vec<u8>,
    read_bytes: AtomicU64,
}

impl MemSource {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            read_bytes: AtomicU64::new(0),
        }
    }

    pub fn bytes_read(&self) -> u64 {
        self.read_bytes.load(Ordering::Relaxed)
    }

    pub fn reset_counter(&self) {
        self.read_bytes.store(0, Ordering::Relaxed);
    }
}

impl TextSource for MemSource {
    fn len(&self) -> u64 {
        self.data.len() as u64
    }

    fn version(&self) -> u64 {
        0
    }

    fn read_range(&self, start: u64, buf: &mut [u8]) -> Result<(), SourceError> {
        let end = start
            .checked_add(buf.len() as u64)
            .ok_or(SourceError::Changed)?;
        if end > self.len() {
            return Err(SourceError::Changed);
        }
        let (Ok(s), Ok(e)) = (usize::try_from(start), usize::try_from(end)) else {
            return Err(SourceError::Changed);
        };
        buf.copy_from_slice(&self.data[s..e]);
        self.read_bytes
            .fetch_add(buf.len() as u64, Ordering::Relaxed);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_inside_and_at_end() {
        let src = MemSource::new(vec![1, 2, 3, 4]);
        let mut buf = [0u8; 2];
        src.read_range(1, &mut buf).unwrap();
        assert_eq!(buf, [2, 3]);

        let mut tail = [0u8; 1];
        src.read_range(3, &mut tail).unwrap();
        assert_eq!(tail, [4]);

        let mut empty = [0u8; 0];
        src.read_range(4, &mut empty).unwrap();
    }

    #[test]
    fn read_past_end_is_changed_not_panic() {
        let src = MemSource::new(vec![1, 2, 3, 4]);
        let mut buf = [0u8; 2];
        let err = src.read_range(3, &mut buf).unwrap_err();
        assert!(err.is_changed());

        let mut big = [0u8; 8];
        assert!(src.read_range(0, &mut big).unwrap_err().is_changed());
    }

    #[test]
    fn offset_overflow_is_changed() {
        let src = MemSource::new(vec![1, 2, 3, 4]);
        let mut buf = [0u8; 2];
        let err = src.read_range(u64::MAX - 1, &mut buf).unwrap_err();
        assert!(err.is_changed());
    }

    #[test]
    fn read_clamped_clamps() {
        let src = MemSource::new(b"hello".to_vec());
        assert_eq!(read_clamped(&src, 0, 3).unwrap(), b"hel");
        assert_eq!(read_clamped(&src, 3, 100).unwrap(), b"lo");
        assert!(read_clamped(&src, 5, 10).unwrap().is_empty());
        assert!(read_clamped(&src, u64::MAX, 10).unwrap().is_empty());
    }

    #[test]
    fn counter_counts_bytes_read() {
        let src = MemSource::new(b"abcdef".to_vec());
        let mut buf = [0u8; 4];
        src.read_range(0, &mut buf).unwrap();
        assert_eq!(src.bytes_read(), 4);
        src.reset_counter();
        assert_eq!(src.bytes_read(), 0);
    }

    #[test]
    fn eol_labels() {
        assert_eq!(EolKind::Lf.label(), "LF");
        assert_eq!(EolKind::Crlf.label(), "CRLF");
        assert_eq!(EolKind::Mixed.label(), "Mixed");
        assert_eq!(EolKind::Unknown.label(), "LF");
    }
}
