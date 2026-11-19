//! Copy size tiers, hex text formatting, and cancellable copy worker (D-14, Q4, CORE-07).

use crate::source::TextSource;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const COPY_IMMEDIATE_MAX: u64 = 16 * 1024 * 1024;
pub const COPY_HARD_CAP: u64 = 256 * 1024 * 1024;
pub const COPY_STEP: usize = 1024 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyKind {
    Text,
    Hex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyTier {
    Immediate,
    Confirm,
    OverCap,
}

/// Number of output UTF-8 bytes produced when copying `bytes` raw bytes.
///
/// Text copy outputs `bytes`; hex copy outputs `"XX XX ..."` (`3 * n - 1` for `n > 0`).
pub fn output_size(kind: CopyKind, bytes: u64) -> u64 {
    match kind {
        CopyKind::Text => bytes,
        CopyKind::Hex => {
            if bytes == 0 {
                0
            } else {
                bytes.saturating_mul(3).saturating_sub(1)
            }
        }
    }
}

pub fn tier(kind: CopyKind, bytes: u64) -> CopyTier {
    let out = output_size(kind, bytes);
    if out <= COPY_IMMEDIATE_MAX {
        CopyTier::Immediate
    } else if out <= COPY_HARD_CAP {
        CopyTier::Confirm
    } else {
        CopyTier::OverCap
    }
}

/// Largest raw byte selection whose formatted output fits within [`COPY_HARD_CAP`].
pub fn max_bytes_for(kind: CopyKind) -> u64 {
    match kind {
        CopyKind::Text => COPY_HARD_CAP,
        CopyKind::Hex => (COPY_HARD_CAP + 1) / 3,
    }
}

/// Appends uppercase hex pairs separated by single spaces into `out`.
pub fn hex_text(bytes: &[u8], leading_space: bool, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 || leading_space {
            out.push(' ');
        }
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0F) as usize] as char);
    }
}

#[derive(Debug)]
pub enum CopyMsg {
    Progress {
        generation: u64,
        done: u64,
        total: u64,
    },
    Done {
        generation: u64,
        text: String,
    },
    Failed {
        generation: u64,
        error: String,
    },
    Cancelled {
        generation: u64,
    },
}

impl CopyMsg {
    pub fn generation(&self) -> u64 {
        match *self {
            Self::Progress { generation, .. }
            | Self::Done { generation, .. }
            | Self::Failed { generation, .. }
            | Self::Cancelled { generation } => generation,
        }
    }
}

pub struct CopyJob {
    pub generation: u64,
    pub cancel: Arc<AtomicBool>,
    pub rx: Receiver<CopyMsg>,
    pub started: Instant,
    handle: Option<JoinHandle<()>>,
}

impl CopyJob {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for CopyJob {
    fn drop(&mut self) {
        // Cancel only; never block the UI thread by joining.
        self.cancel();
    }
}

pub fn spawn_copy(
    generation: u64,
    src: Box<dyn TextSource>,
    range: Range<u64>,
    kind: CopyKind,
    notify: Box<dyn Fn() + Send>,
) -> CopyJob {
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_w = Arc::clone(&cancel);
    let (tx, rx) = mpsc::channel();
    let started = Instant::now();

    let handle = std::thread::spawn(move || {
        let total = range.end.saturating_sub(range.start);
        if range.end < range.start || range.end > src.len() {
            if tx
                .send(CopyMsg::Failed {
                    generation,
                    error: "selection outside file".to_owned(),
                })
                .is_ok()
            {
                notify();
            }
            return;
        }
        if tier(kind, total) == CopyTier::OverCap {
            if tx
                .send(CopyMsg::Failed {
                    generation,
                    error: "selection exceeds the copy limit".to_owned(),
                })
                .is_ok()
            {
                notify();
            }
            return;
        }
        if total == 0 {
            if tx
                .send(CopyMsg::Done {
                    generation,
                    text: String::new(),
                })
                .is_ok()
            {
                notify();
            }
            return;
        }

        let mut last_sent = Instant::now();
        match kind {
            CopyKind::Text => {
                let Ok(cap) = usize::try_from(total) else {
                    if tx
                        .send(CopyMsg::Failed {
                            generation,
                            error: "selection exceeds the copy limit".to_owned(),
                        })
                        .is_ok()
                    {
                        notify();
                    }
                    return;
                };
                let mut raw = vec![0u8; cap];
                let mut done = 0usize;
                while done < cap {
                    if cancel_w.load(Ordering::Relaxed) {
                        if tx.send(CopyMsg::Cancelled { generation }).is_ok() {
                            notify();
                        }
                        return;
                    }
                    let step = (cap - done).min(COPY_STEP);
                    if let Err(e) =
                        src.read_range(range.start + done as u64, &mut raw[done..done + step])
                    {
                        if tx
                            .send(CopyMsg::Failed {
                                generation,
                                error: e.to_string(),
                            })
                            .is_ok()
                        {
                            notify();
                        }
                        return;
                    }
                    done += step;
                    if done < cap && last_sent.elapsed() >= PROGRESS_INTERVAL {
                        if tx
                            .send(CopyMsg::Progress {
                                generation,
                                done: done as u64,
                                total,
                            })
                            .is_err()
                        {
                            return;
                        }
                        notify();
                        last_sent = Instant::now();
                    }
                }
                let text = String::from_utf8(raw)
                    .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
                if tx.send(CopyMsg::Done { generation, text }).is_ok() {
                    notify();
                }
            }
            CopyKind::Hex => {
                let Ok(cap) = usize::try_from(output_size(CopyKind::Hex, total)) else {
                    if tx
                        .send(CopyMsg::Failed {
                            generation,
                            error: "selection exceeds the copy limit".to_owned(),
                        })
                        .is_ok()
                    {
                        notify();
                    }
                    return;
                };
                let mut out = String::with_capacity(cap);
                let mut chunk = vec![0u8; COPY_STEP.min(total as usize)];
                let mut done = 0u64;
                while done < total {
                    if cancel_w.load(Ordering::Relaxed) {
                        if tx.send(CopyMsg::Cancelled { generation }).is_ok() {
                            notify();
                        }
                        return;
                    }
                    let step = (total - done).min(chunk.len() as u64) as usize;
                    if let Err(e) = src.read_range(range.start + done, &mut chunk[..step]) {
                        if tx
                            .send(CopyMsg::Failed {
                                generation,
                                error: e.to_string(),
                            })
                            .is_ok()
                        {
                            notify();
                        }
                        return;
                    }
                    hex_text(&chunk[..step], done > 0, &mut out);
                    done += step as u64;
                    if done < total && last_sent.elapsed() >= PROGRESS_INTERVAL {
                        if tx
                            .send(CopyMsg::Progress {
                                generation,
                                done,
                                total,
                            })
                            .is_err()
                        {
                            return;
                        }
                        notify();
                        last_sent = Instant::now();
                    }
                }
                if tx
                    .send(CopyMsg::Done {
                        generation,
                        text: out,
                    })
                    .is_ok()
                {
                    notify();
                }
            }
        }
    });

    CopyJob {
        generation,
        cancel,
        rx,
        started,
        handle: Some(handle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{MemSource, SourceError};

    struct SlowZeroSource(u64);

    impl TextSource for SlowZeroSource {
        fn len(&self) -> u64 {
            self.0
        }
        fn version(&self) -> u64 {
            0
        }
        fn read_range(&self, start: u64, buf: &mut [u8]) -> Result<(), SourceError> {
            let end = start
                .checked_add(buf.len() as u64)
                .ok_or(SourceError::Changed)?;
            if end > self.0 {
                return Err(SourceError::Changed);
            }
            std::thread::sleep(Duration::from_millis(5));
            buf.fill(0);
            Ok(())
        }
    }

    struct FailingSource(u64);

    impl TextSource for FailingSource {
        fn len(&self) -> u64 {
            self.0
        }
        fn version(&self) -> u64 {
            0
        }
        fn read_range(&self, _start: u64, _buf: &mut [u8]) -> Result<(), SourceError> {
            Err(SourceError::Changed)
        }
    }

    #[test]
    fn output_size_and_caps() {
        assert_eq!(output_size(CopyKind::Text, 100), 100);
        assert_eq!(output_size(CopyKind::Hex, 0), 0);
        assert_eq!(output_size(CopyKind::Hex, 3), 8);
        assert_eq!(max_bytes_for(CopyKind::Text), COPY_HARD_CAP);
        assert_eq!(max_bytes_for(CopyKind::Hex), 89_478_485);
        assert!(output_size(CopyKind::Hex, 89_478_485) <= COPY_HARD_CAP);
        assert!(output_size(CopyKind::Hex, 89_478_486) > COPY_HARD_CAP);
    }

    #[test]
    fn tier_boundaries() {
        assert_eq!(tier(CopyKind::Text, 0), CopyTier::Immediate);
        assert_eq!(
            tier(CopyKind::Text, COPY_IMMEDIATE_MAX),
            CopyTier::Immediate
        );
        assert_eq!(
            tier(CopyKind::Text, COPY_IMMEDIATE_MAX + 1),
            CopyTier::Confirm
        );
        assert_eq!(tier(CopyKind::Text, COPY_HARD_CAP), CopyTier::Confirm);
        assert_eq!(tier(CopyKind::Text, COPY_HARD_CAP + 1), CopyTier::OverCap);

        assert_eq!(tier(CopyKind::Hex, 5_592_405), CopyTier::Immediate);
        assert_eq!(tier(CopyKind::Hex, 5_592_406), CopyTier::Confirm);
        assert_eq!(tier(CopyKind::Hex, 90_000_000), CopyTier::OverCap);
    }

    #[test]
    fn hex_formatting_chunked() {
        let mut s = String::new();
        hex_text(&[0x4C, 0x9A, 0xFF], false, &mut s);
        assert_eq!(s, "4C 9A FF");

        let mut split = String::new();
        hex_text(&[0x4C, 0x9A], false, &mut split);
        hex_text(&[0xFF], true, &mut split);
        assert_eq!(split, s);
    }

    #[test]
    fn worker_text_valid_utf8() {
        let data = vec![b'a'; 3 * 1024 * 1024];
        let job = spawn_copy(
            1,
            Box::new(MemSource::new(data.clone())),
            0..data.len() as u64,
            CopyKind::Text,
            Box::new(|| {}),
        );
        loop {
            match job.rx.recv_timeout(Duration::from_secs(10)).unwrap() {
                CopyMsg::Progress { .. } => continue,
                CopyMsg::Done { text, .. } => {
                    assert_eq!(text.as_bytes(), data.as_slice());
                    break;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn worker_text_invalid_utf8() {
        let data = b"ok \xFF\xFE end".to_vec();
        let expected = String::from_utf8_lossy(&data).into_owned();
        let job = spawn_copy(
            2,
            Box::new(MemSource::new(data.clone())),
            0..data.len() as u64,
            CopyKind::Text,
            Box::new(|| {}),
        );
        match job.rx.recv_timeout(Duration::from_secs(5)).unwrap() {
            CopyMsg::Done { text, .. } => assert_eq!(text, expected),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn worker_hex_one_mib() {
        let data: Vec<u8> = (0..(1024 * 1024)).map(|i| (i & 0xFF) as u8).collect();
        let mut expected = String::with_capacity(data.len() * 3);
        hex_text(&data, false, &mut expected);
        let job = spawn_copy(
            3,
            Box::new(MemSource::new(data.clone())),
            0..data.len() as u64,
            CopyKind::Hex,
            Box::new(|| {}),
        );
        loop {
            match job.rx.recv_timeout(Duration::from_secs(10)).unwrap() {
                CopyMsg::Progress { .. } => continue,
                CopyMsg::Done { text, .. } => {
                    assert_eq!(text, expected);
                    break;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }

    #[test]
    fn worker_cancel() {
        let len = 200 * 1024 * 1024;
        let job = spawn_copy(
            4,
            Box::new(SlowZeroSource(len)),
            0..len,
            CopyKind::Text,
            Box::new(|| {}),
        );
        std::thread::sleep(Duration::from_millis(20));
        job.cancel();
        let t0 = Instant::now();
        let mut saw_cancel = false;
        while t0.elapsed() < Duration::from_secs(2) {
            match job.rx.recv_timeout(Duration::from_millis(500)) {
                Ok(CopyMsg::Cancelled { .. }) => {
                    saw_cancel = true;
                    break;
                }
                Ok(CopyMsg::Progress { .. }) => continue,
                Ok(other) => panic!("unexpected {other:?}"),
                Err(_) => break,
            }
        }
        assert!(saw_cancel);
        assert!(t0.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn worker_read_failure() {
        let job = spawn_copy(
            5,
            Box::new(FailingSource(100)),
            0..100,
            CopyKind::Text,
            Box::new(|| {}),
        );
        assert!(matches!(
            job.rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            CopyMsg::Failed { .. }
        ));
    }

    #[test]
    fn worker_range_outside_len() {
        let job = spawn_copy(
            6,
            Box::new(MemSource::new(b"abc".to_vec())),
            0..10,
            CopyKind::Text,
            Box::new(|| {}),
        );
        assert!(matches!(
            job.rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            CopyMsg::Failed { .. }
        ));
    }

    #[test]
    fn worker_over_cap_refused() {
        let job = spawn_copy(
            7,
            Box::new(SlowZeroSource(COPY_HARD_CAP + 10)),
            0..(COPY_HARD_CAP + 1),
            CopyKind::Text,
            Box::new(|| {}),
        );
        assert!(matches!(
            job.rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            CopyMsg::Failed { .. }
        ));
    }
}
