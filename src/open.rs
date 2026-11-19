//! Open pipeline: non-blocking classification, worker-thread open, bounded first-block
//! sample, and binary / UTF-16 / EOL heuristics (D-10, D-19, HEX-05, CORE-01).

use crate::filesource::FileSource;
use crate::source::{EolKind, read_clamped};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

pub const HEAD_BYTES: usize = 65_536;
pub const SNIFF_BYTES: usize = 8_192;

#[derive(Debug)]
pub enum OpenError {
    NotRegular { name: String },
    Io { name: String, reason: String },
}

impl OpenError {
    pub fn message(&self) -> String {
        match self {
            Self::NotRegular { name } => {
                format!("Couldn't open {name}: it is not a regular file (folder, pipe or device).")
            }
            Self::Io { name, reason } => format!(
                "Couldn't open {name}: {reason}. Check that the file exists and you have permission to read it."
            ),
        }
    }

    pub fn short_message(&self) -> String {
        match self {
            Self::NotRegular { name } => format!("Can't open {name}: not a regular file"),
            Self::Io { .. } => self.message(),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::NotRegular { name } | Self::Io { name, .. } => name,
        }
    }
}

pub fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub struct Opened {
    pub source: FileSource,
    pub path: PathBuf,
    pub name: String,
    pub len: u64,
    pub head: Vec<u8>,
    pub binary: bool,
    pub utf16_bom: bool,
    pub eol: EolKind,
}

/// Checks `path` via `metadata()` (follows symlinks; does not block on FIFOs) and rejects
/// non-regular files (directories, pipes, devices).
pub fn classify(path: &Path) -> Result<u64, OpenError> {
    let name = display_name(path);
    let meta = std::fs::metadata(path).map_err(|e| OpenError::Io {
        name: name.clone(),
        reason: e.to_string(),
    })?;
    if !meta.file_type().is_file() {
        return Err(OpenError::NotRegular { name });
    }
    Ok(meta.len())
}

/// Classifies, opens a [`FileSource`], reads at most [`HEAD_BYTES`], and computes the initial
/// heuristics. Run on a worker thread via [`spawn_open`] so the UI thread never blocks (D-19).
pub fn open_blocking(path: &Path) -> Result<Opened, OpenError> {
    let name = display_name(path);
    let _ = classify(path)?;
    let source = FileSource::open(path).map_err(|e| OpenError::Io {
        name: name.clone(),
        reason: e.to_string(),
    })?;
    let len = source.identity().len;
    let head = read_clamped(&source, 0, HEAD_BYTES).map_err(|e| OpenError::Io {
        name: name.clone(),
        reason: e.to_string(),
    })?;
    let binary = looks_binary(&head);
    let utf16_bom = has_utf16_bom(&head);
    let eol = eol_of_sample(&head);
    Ok(Opened {
        source,
        path: path.to_path_buf(),
        name,
        len,
        head,
        binary,
        utf16_bom,
        eol,
    })
}

/// D-10 binary heuristic over the first [`SNIFF_BYTES`] bytes: any NUL byte or >30% control
/// bytes (excluding `\t`, `\n`, `\r`, `0x0C`, `0x1B`; including `0x7F`).
pub fn looks_binary(sample: &[u8]) -> bool {
    let n = sample.len().min(SNIFF_BYTES);
    if n == 0 {
        return false;
    }
    let slice = &sample[..n];
    if memchr::memchr(0, slice).is_some() {
        return true;
    }
    let mut ctrl = 0usize;
    for &b in slice {
        let is_ctrl = (b < 0x20 && !matches!(b, b'\t' | b'\n' | b'\r' | 0x0C | 0x1B)) || b == 0x7F;
        if is_ctrl {
            ctrl += 1;
        }
    }
    ctrl * 100 > 30 * n
}

/// True if `sample` starts with a UTF-16 LE (`FF FE`) or BE (`FE FF`) BOM.
pub fn has_utf16_bom(sample: &[u8]) -> bool {
    matches!(sample, [0xFF, 0xFE, ..] | [0xFE, 0xFF, ..])
}

/// Counts `\n` and `\r\n` pairs in `sample` to classify its line endings.
pub fn eol_of_sample(sample: &[u8]) -> EolKind {
    let mut lf = 0usize;
    let mut crlf = 0usize;
    for pos in memchr::memchr_iter(b'\n', sample) {
        lf += 1;
        if pos > 0 && sample[pos - 1] == b'\r' {
            crlf += 1;
        }
    }
    if lf == 0 {
        EolKind::Unknown
    } else if crlf == lf {
        EolKind::Crlf
    } else if crlf == 0 {
        EolKind::Lf
    } else {
        EolKind::Mixed
    }
}

pub type OpenResult = (u64, PathBuf, Result<Opened, OpenError>);

/// Opens `path` on a background thread and calls `notify()` after delivering the result.
pub fn spawn_open(
    generation: u64,
    path: PathBuf,
    notify: Box<dyn Fn() + Send>,
) -> mpsc::Receiver<OpenResult> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let res = open_blocking(&path);
        if tx.send((generation, path, res)).is_ok() {
            notify();
        }
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn binary_table() {
        assert!(!looks_binary(b""));
        assert!(!looks_binary(b"hello\n\t\r\x0C\x1Bworld"));
        assert!(looks_binary(b"abc\x00def"));

        let mut thirty_one = vec![b'a'; 100];
        for b in &mut thirty_one[..31] {
            *b = 0x01;
        }
        assert!(looks_binary(&thirty_one));

        let mut twenty_nine = vec![b'a'; 100];
        for b in &mut twenty_nine[..29] {
            *b = 0x01;
        }
        assert!(!looks_binary(&twenty_nine));

        let del_heavy = vec![0x7F; 40];
        assert!(looks_binary(&del_heavy));

        // NUL past SNIFF_BYTES is ignored.
        let mut late_nul = vec![b'a'; 9_001];
        late_nul[9_000] = 0;
        assert!(!looks_binary(&late_nul));
    }

    #[test]
    fn utf16_bom() {
        assert!(has_utf16_bom(&[0xFF, 0xFE, b'a', 0]));
        assert!(has_utf16_bom(&[0xFE, 0xFF, 0, b'a']));
        assert!(!has_utf16_bom(&[0xEF, 0xBB, 0xBF, b'a']));
        assert!(!has_utf16_bom(&[0xFF]));
    }

    #[test]
    fn eol_sample() {
        assert_eq!(eol_of_sample(b"a\nb\n"), EolKind::Lf);
        assert_eq!(eol_of_sample(b"a\r\nb\r\n"), EolKind::Crlf);
        assert_eq!(eol_of_sample(b"a\r\nb\n"), EolKind::Mixed);
        assert_eq!(eol_of_sample(b"abc"), EolKind::Unknown);
    }

    #[test]
    fn dir_rejected() {
        let dir = TempDir::new();
        let err = classify(dir.path()).unwrap_err();
        assert!(matches!(err, OpenError::NotRegular { .. }));
        let name = display_name(dir.path());
        assert_eq!(
            err.message(),
            format!("Couldn't open {name}: it is not a regular file (folder, pipe or device).")
        );
        assert_eq!(
            err.short_message(),
            format!("Can't open {name}: not a regular file")
        );
    }

    #[cfg(unix)]
    #[test]
    fn fifo_rejected() {
        let dir = TempDir::new();
        let fifo = dir.path().join("pipe");
        let status = std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("mkfifo");
        assert!(status.success());

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(classify(&fifo));
        });
        let err = rx
            .recv_timeout(Duration::from_secs(2))
            .expect("classify must not block on FIFO")
            .unwrap_err();
        assert!(matches!(err, OpenError::NotRegular { .. }));
    }

    #[cfg(windows)]
    #[test]
    fn windows_reserved_name_rejected() {
        assert!(classify(Path::new("NUL")).is_err());
    }

    #[test]
    fn missing_file() {
        let err = open_blocking(Path::new("definitely/missing/nope.txt"))
            .err()
            .unwrap();
        let msg = err.message();
        assert!(msg.starts_with("Couldn't open nope.txt: "), "got: {msg}");
        assert!(
            msg.ends_with("Check that the file exists and you have permission to read it."),
            "got: {msg}"
        );
    }

    #[test]
    fn head_is_bounded() {
        let dir = TempDir::new();
        let big = dir.file("big.txt", &vec![b'a'; 1024 * 1024]);
        let o = open_blocking(&big).unwrap();
        assert_eq!(o.head.len(), HEAD_BYTES);
        assert_eq!(o.len, 1024 * 1024);

        let small = dir.file("small.txt", b"0123456789");
        let o = open_blocking(&small).unwrap();
        assert_eq!(o.head.len(), 10);

        let empty = dir.file("empty.txt", b"");
        let o = open_blocking(&empty).unwrap();
        assert!(o.head.is_empty());
        assert!(!o.binary);
        assert_eq!(o.eol, EolKind::Unknown);
    }

    #[test]
    fn spawn_open_delivers() {
        let dir = TempDir::new();
        let path = dir.file("hi.txt", b"hello\n");
        let hits = Arc::new(AtomicUsize::new(0));
        let hits2 = Arc::clone(&hits);
        let rx = spawn_open(
            7,
            path.clone(),
            Box::new(move || {
                hits2.fetch_add(1, Ordering::Relaxed);
            }),
        );
        let (got_gen, got_path, res) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(got_gen, 7);
        assert_eq!(got_path, path);
        let opened = res.unwrap();
        assert_eq!(opened.len, 6);
        assert_eq!(opened.eol, EolKind::Lf);
        // Wait briefly for notify() which runs right after send().
        for _ in 0..50 {
            if hits.load(Ordering::Relaxed) == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(hits.load(Ordering::Relaxed), 1);
    }
}
