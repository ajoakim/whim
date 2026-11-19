//! Positional-read file source with a bounded LRU chunk cache (D-01, D-02, CORE-02).
//!
//! No `mmap` is used: every read is a positional `read_at`/`seek_read` verified against the
//! file's `(len, modified)` identity. External truncation or replacement surfaces as
//! [`SourceError::Changed`] rather than SIGBUS or UB (Phase 1 WR-01).

use crate::source::{SourceError, TextSource};
use std::fs::File;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

pub const CACHE_BLOCK: usize = 65_536;
pub const CACHE_BLOCKS: usize = 16;
pub const DIRECT_READ_MIN: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileIdentity {
    pub len: u64,
    pub modified: Option<SystemTime>,
}

impl FileIdentity {
    fn from_file(f: &File) -> Result<Self, SourceError> {
        let meta = f.metadata()?;
        Ok(Self {
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }
}

struct CacheEntry {
    idx: u64,
    data: Box<[u8]>,
    last_use: u64,
}

struct ChunkCache {
    entries: Vec<CacheEntry>,
    tick: u64,
}

impl ChunkCache {
    fn new() -> Self {
        Self {
            entries: Vec::with_capacity(CACHE_BLOCKS),
            tick: 0,
        }
    }

    fn next_tick(&mut self) -> u64 {
        self.tick = self.tick.wrapping_add(1);
        self.tick
    }

    fn get(&mut self, idx: u64) -> Option<&[u8]> {
        let pos = self.entries.iter().position(|e| e.idx == idx)?;
        let tick = self.next_tick();
        self.entries[pos].last_use = tick;
        Some(&self.entries[pos].data)
    }

    fn insert(&mut self, idx: u64, data: Box<[u8]>) -> &[u8] {
        let tick = self.next_tick();
        if let Some(pos) = self.entries.iter().position(|e| e.idx == idx) {
            self.entries[pos].data = data;
            self.entries[pos].last_use = tick;
            return &self.entries[pos].data;
        }
        if self.entries.len() < CACHE_BLOCKS {
            self.entries.push(CacheEntry {
                idx,
                data,
                last_use: tick,
            });
            let last = self.entries.len() - 1;
            return &self.entries[last].data;
        }
        // Evict least-recently-used entry.
        let mut lru_pos = 0usize;
        let mut lru_tick = self.entries[0].last_use;
        for (i, e) in self.entries.iter().enumerate().skip(1) {
            if e.last_use < lru_tick {
                lru_tick = e.last_use;
                lru_pos = i;
            }
        }
        self.entries[lru_pos] = CacheEntry {
            idx,
            data,
            last_use: tick,
        };
        &self.entries[lru_pos].data
    }
}

pub struct FileSource {
    path: PathBuf,
    file: File,
    identity: FileIdentity,
    cache: Mutex<ChunkCache>,
}

impl FileSource {
    /// Opens `path` read-only and captures its `(len, modified)` identity.
    pub fn open(path: &Path) -> Result<Self, SourceError> {
        let file = File::open(path)?;
        let identity = FileIdentity::from_file(&file)?;
        Ok(Self {
            path: path.to_path_buf(),
            file,
            identity,
            cache: Mutex::new(ChunkCache::new()),
        })
    }

    /// Opens a fresh OS handle by path (never `try_clone`, which shares the file cursor on
    /// Windows) and verifies the identity matches `self.identity`.
    pub fn reopen(&self) -> Result<Self, SourceError> {
        let other = Self::open(&self.path)?;
        if other.identity != self.identity {
            return Err(SourceError::Changed);
        }
        Ok(other)
    }

    /// Like [`FileSource::reopen`], with a sequential-scan hint on Windows for background workers.
    pub fn reopen_for_scan(&self) -> Result<Self, SourceError> {
        let file = open_for_scan(&self.path)?;
        let identity = FileIdentity::from_file(&file)?;
        if identity != self.identity {
            return Err(SourceError::Changed);
        }
        Ok(Self {
            path: self.path.clone(),
            file,
            identity,
            cache: Mutex::new(ChunkCache::new()),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn identity(&self) -> FileIdentity {
        self.identity
    }

    #[cfg(test)]
    pub fn cached_blocks(&self) -> usize {
        self.cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entries
            .len()
    }

    fn check_identity(&self) -> Result<(), SourceError> {
        let now = FileIdentity::from_file(&self.file)?;
        if now != self.identity {
            return Err(SourceError::Changed);
        }
        Ok(())
    }
}

#[cfg(windows)]
fn open_for_scan(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_SEQUENTIAL_SCAN: u32 = 0x0800_0000;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_SEQUENTIAL_SCAN)
        .open(path)
}

#[cfg(not(windows))]
fn open_for_scan(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

#[cfg(unix)]
fn read_at_once(f: &File, buf: &mut [u8], off: u64) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    f.read_at(buf, off)
}

#[cfg(windows)]
fn read_at_once(f: &File, buf: &mut [u8], off: u64) -> std::io::Result<usize> {
    use std::os::windows::fs::FileExt;
    f.seek_read(buf, off)
}

/// Positional `read_exact`: loops over short reads, retries `Interrupted`, treats EOF (`0`) as
/// [`SourceError::Changed`].
pub fn read_full_at(f: &File, mut buf: &mut [u8], mut off: u64) -> Result<(), SourceError> {
    while !buf.is_empty() {
        match read_at_once(f, buf, off) {
            Ok(0) => return Err(SourceError::Changed),
            Ok(n) => {
                off = off.checked_add(n as u64).ok_or(SourceError::Changed)?;
                buf = &mut buf[n..];
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(SourceError::Io(e)),
        }
    }
    Ok(())
}

impl TextSource for FileSource {
    fn len(&self) -> u64 {
        self.identity.len
    }

    fn version(&self) -> u64 {
        0
    }

    fn read_range(&self, start: u64, buf: &mut [u8]) -> Result<(), SourceError> {
        let end = start
            .checked_add(buf.len() as u64)
            .ok_or(SourceError::Changed)?;
        if end > self.identity.len {
            return Err(SourceError::Changed);
        }
        if buf.is_empty() {
            return Ok(());
        }

        if buf.len() >= DIRECT_READ_MIN {
            self.check_identity()?;
            return read_full_at(&self.file, buf, start);
        }

        let blk_size = CACHE_BLOCK as u64;
        let first_idx = start / blk_size;
        let last_idx = (end - 1) / blk_size;
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        let mut out_pos = 0usize;

        for idx in first_idx..=last_idx {
            let blk_start = idx.checked_mul(blk_size).ok_or(SourceError::Changed)?;
            let slice_start = if idx == first_idx {
                usize::try_from(start - blk_start).map_err(|_| SourceError::Changed)?
            } else {
                0
            };
            let blk_end = blk_start
                .checked_add(blk_size)
                .map_or(self.identity.len, |v| v.min(self.identity.len));
            let slice_end =
                usize::try_from(end.min(blk_end) - blk_start).map_err(|_| SourceError::Changed)?;
            let copy_len = slice_end
                .checked_sub(slice_start)
                .ok_or(SourceError::Changed)?;

            if let Some(cached) = cache.get(idx) {
                let src = cached
                    .get(slice_start..slice_end)
                    .ok_or(SourceError::Changed)?;
                buf[out_pos..out_pos + copy_len].copy_from_slice(src);
                out_pos += copy_len;
                continue;
            }

            self.check_identity()?;
            let blk_len = usize::try_from(blk_end - blk_start).map_err(|_| SourceError::Changed)?;
            let mut raw = vec![0u8; blk_len].into_boxed_slice();
            read_full_at(&self.file, &mut raw, blk_start)?;
            let inserted = cache.insert(idx, raw);
            let src = inserted
                .get(slice_start..slice_end)
                .ok_or(SourceError::Changed)?;
            buf[out_pos..out_pos + copy_len].copy_from_slice(src);
            out_pos += copy_len;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{TempDir, XorShift};
    use std::io::Write;

    #[test]
    fn random_reads_match() {
        let dir = TempDir::new();
        let mut rng = XorShift::new(12345);
        let len = 1024 * 1024;
        let data: Vec<u8> = (0..len).map(|_| (rng.next_u64() & 0xFF) as u8).collect();
        let path = dir.file("rand.bin", &data);
        let src = FileSource::open(&path).unwrap();

        for _ in 0..2_000 {
            let start = rng.below(len as u64) as usize;
            let max_l = (len - start).min(200_000);
            let rlen = if max_l == 0 {
                0
            } else {
                rng.below(max_l as u64 + 1) as usize
            };
            let mut buf = vec![0u8; rlen];
            src.read_range(start as u64, &mut buf).unwrap();
            assert_eq!(buf.as_slice(), &data[start..start + rlen]);
        }
    }

    #[test]
    fn cache_bounded() {
        let dir = TempDir::new();
        let data = vec![0xABu8; 4 * 1024 * 1024];
        let path = dir.file("big.bin", &data);
        let src = FileSource::open(&path).unwrap();
        let mut rng = XorShift::new(99);
        let mut buf = [0u8; 128];
        for _ in 0..1_000 {
            let off = rng.below((data.len() - 128) as u64);
            src.read_range(off, &mut buf).unwrap();
            assert!(src.cached_blocks() <= CACHE_BLOCKS);
        }
        assert_eq!(src.cached_blocks(), CACHE_BLOCKS);
    }

    #[test]
    fn truncate() {
        let dir = TempDir::new();
        let data = vec![b'x'; 300 * 1024];
        let path = dir.file("trunc.bin", &data);
        let src = FileSource::open(&path).unwrap();

        let f = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        f.set_len(1_000).unwrap();
        drop(f);

        let mut buf = [0u8; 4096];
        let err = src.read_range(100_000, &mut buf).unwrap_err();
        assert!(err.is_changed());
    }

    #[test]
    fn grow_is_changed() {
        let dir = TempDir::new();
        let data = vec![b'a'; 200 * 1024];
        let path = dir.file("grow.bin", &data);
        let src = FileSource::open(&path).unwrap();

        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"more bytes").unwrap();
        drop(f);

        let mut buf = [0u8; 64];
        let err = src.read_range(150_000, &mut buf).unwrap_err();
        assert!(err.is_changed());
    }

    #[test]
    fn reopen_identity() {
        let dir = TempDir::new();
        let path = dir.file("reopen.bin", b"hello world");
        let src = FileSource::open(&path).unwrap();
        let copy = src.reopen().unwrap();
        assert_eq!(copy.len(), src.len());
        let scan = src.reopen_for_scan().unwrap();
        assert_eq!(scan.len(), src.len());

        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"!").unwrap();
        drop(f);

        assert!(src.reopen().err().unwrap().is_changed());
        assert!(src.reopen_for_scan().err().unwrap().is_changed());
    }

    #[test]
    fn large_direct_read() {
        let dir = TempDir::new();
        let data = vec![0x5Au8; 1024 * 1024];
        let path = dir.file("direct.bin", &data);
        let src = FileSource::open(&path).unwrap();
        let mut buf = vec![0u8; 1024 * 1024];
        src.read_range(0, &mut buf).unwrap();
        assert_eq!(buf, data);
        assert_eq!(src.cached_blocks(), 0);
    }

    #[test]
    fn past_end() {
        let dir = TempDir::new();
        let path = dir.file("short.bin", b"abcd");
        let src = FileSource::open(&path).unwrap();
        let mut buf = [0u8; 4];
        assert!(src.read_range(2, &mut buf).unwrap_err().is_changed());
        assert!(src.read_range(u64::MAX, &mut buf).unwrap_err().is_changed());
    }

    #[test]
    fn empty_file() {
        let dir = TempDir::new();
        let path = dir.file("empty.bin", b"");
        let src = FileSource::open(&path).unwrap();
        assert_eq!(src.len(), 0);
        src.read_range(0, &mut []).unwrap();
    }
}
