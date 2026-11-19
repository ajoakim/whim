//! Sparse, cancellable background line index (D-03, CORE-04, CORE-07).
//!
//! Stores one `u64` cumulative newline count per 64 KiB block (128 KiB per GiB of file).
//! Inside the indexed prefix `line_of`/`offset_of` are exact; beyond it they estimate
//! from the average line length seen so far.

use crate::source::{EolKind, SourceError, TextSource, read_clamped};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const BLOCK: u64 = 65_536;
pub const INDEX_READ: usize = 4 * 1024 * 1024;
pub const BATCH_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exactness {
    Exact,
    Estimated,
}

#[derive(Debug)]
pub enum IndexMsg {
    Batch {
        generation: u64,
        cum_tail: Vec<u64>,
        covered: u64,
    },
    Done {
        generation: u64,
        total_newlines: u64,
        cr_total: u64,
    },
    Failed {
        generation: u64,
        error: String,
    },
    Cancelled {
        generation: u64,
    },
}

impl IndexMsg {
    pub fn generation(&self) -> u64 {
        match *self {
            Self::Batch { generation, .. }
            | Self::Done { generation, .. }
            | Self::Failed { generation, .. }
            | Self::Cancelled { generation } => generation,
        }
    }
}

pub struct LineIndex {
    len: u64,
    cum: Vec<u64>,
    covered: u64,
    complete: bool,
    cancelled: bool,
    total_newlines: u64,
    cr_total: u64,
    head_avg: Option<f64>,
}

impl LineIndex {
    pub fn new(len: u64, head: &[u8]) -> Self {
        let head_nl = memchr::memchr_iter(b'\n', head).count();
        let head_avg = if head_nl > 0 {
            Some(head.len() as f64 / head_nl as f64)
        } else {
            None
        };
        let complete = len == 0;
        Self {
            len,
            cum: vec![0],
            covered: 0,
            complete,
            cancelled: false,
            total_newlines: 0,
            cr_total: 0,
            head_avg,
        }
    }

    pub fn apply(&mut self, msg: &IndexMsg) {
        match msg {
            IndexMsg::Batch {
                cum_tail, covered, ..
            } => {
                self.cum.extend_from_slice(cum_tail);
                self.covered = (*covered).min(self.len);
            }
            IndexMsg::Done {
                total_newlines,
                cr_total,
                ..
            } => {
                self.total_newlines = *total_newlines;
                self.cr_total = *cr_total;
                self.covered = self.len;
                self.complete = true;
            }
            IndexMsg::Cancelled { .. } => {
                self.cancelled = true;
            }
            IndexMsg::Failed { .. } => {}
        }
    }

    pub fn mark_cancelled(&mut self) {
        self.cancelled = true;
    }

    pub fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    pub fn covered(&self) -> u64 {
        self.covered
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn progress_pct(&self) -> u8 {
        if self.complete || self.len == 0 {
            return 100;
        }
        let pct = (u128::from(self.covered) * 100) / u128::from(self.len);
        pct.min(100) as u8
    }

    pub fn avg_line_bytes(&self) -> Option<f64> {
        let last = *self.cum.last().unwrap_or(&0);
        if last > 0 && self.covered > 0 {
            Some(self.covered as f64 / last as f64)
        } else {
            self.head_avg
        }
    }

    /// 0-based line number of byte `off`.
    pub fn line_of(
        &self,
        src: &dyn TextSource,
        off: u64,
    ) -> Result<(u64, Exactness), SourceError> {
        let off = off.min(self.len);
        if off == 0 {
            return Ok((0, Exactness::Exact));
        }
        if off <= self.covered {
            let max_blk = self.cum.len().saturating_sub(1);
            let b = usize::try_from(off / BLOCK).unwrap_or(max_blk).min(max_blk);
            let blk_start = (b as u64).saturating_mul(BLOCK);
            let delta = usize::try_from(off.saturating_sub(blk_start)).unwrap_or(0);
            let extra = if delta > 0 {
                let bytes = read_clamped(src, blk_start, delta)?;
                memchr::memchr_iter(b'\n', &bytes).count() as u64
            } else {
                0
            };
            return Ok((self.cum[b].saturating_add(extra), Exactness::Exact));
        }

        let base = *self.cum.last().unwrap_or(&0);
        let est = match self.avg_line_bytes() {
            Some(avg) if avg > 0.0 => {
                let extra = ((off - self.covered) as f64 / avg) as u64;
                base.saturating_add(extra)
            }
            _ => base,
        };
        Ok((est, Exactness::Estimated))
    }

    /// Byte offset of the start of 0-based line `line0`.
    pub fn offset_of(
        &self,
        src: &dyn TextSource,
        line0: u64,
    ) -> Result<(u64, Exactness), SourceError> {
        if line0 == 0 || self.len == 0 {
            return Ok((0, Exactness::Exact));
        }

        let last_cum = *self.cum.last().unwrap_or(&0);
        let max_exact_line = if self.complete {
            self.total_newlines
        } else {
            last_cum
        };

        if line0 <= max_exact_line {
            if line0 <= last_cum {
                // Find first index i with cum[i] >= line0; then block is b = i - 1 (cum[b] < line0 <= cum[b+1]).
                let idx = self.cum.partition_point(|&c| c < line0);
                let b = idx.saturating_sub(1);
                let blk_start = (b as u64).saturating_mul(BLOCK);
                let need = usize::try_from(line0 - self.cum[b] - 1)
                    .map_err(|_| SourceError::Changed)?;
                let buf = read_clamped(src, blk_start, BLOCK as usize)?;
                if let Some(pos) = memchr::memchr_iter(b'\n', &buf).nth(need) {
                    return Ok((
                        blk_start.saturating_add(pos as u64 + 1).min(self.len),
                        Exactness::Exact,
                    ));
                }
                return Err(SourceError::Changed);
            } else {
                // Complete index, line lies in the final partial block (< BLOCK bytes).
                let b = self.cum.len().saturating_sub(1);
                let blk_start = (b as u64).saturating_mul(BLOCK);
                let need = usize::try_from(line0 - last_cum - 1)
                    .map_err(|_| SourceError::Changed)?;
                let tail_len = usize::try_from(self.len.saturating_sub(blk_start))
                    .map_err(|_| SourceError::Changed)?;
                let buf = read_clamped(src, blk_start, tail_len)?;
                if let Some(pos) = memchr::memchr_iter(b'\n', &buf).nth(need) {
                    return Ok((
                        blk_start.saturating_add(pos as u64 + 1).min(self.len),
                        Exactness::Exact,
                    ));
                }
                return Err(SourceError::Changed);
            }
        }

        if self.complete {
            // Past last line: return start of last line (or 0 if no newlines).
            return self.offset_of(src, self.total_newlines);
        }

        // Estimate beyond indexed prefix.
        let est = match self.avg_line_bytes() {
            Some(avg) if avg > 0.0 => {
                let delta = (line0.saturating_sub(last_cum) as f64) * avg;
                let off = (self.covered as f64 + delta).min((self.len - 1) as f64);
                (off.max(0.0) as u64).min(self.len - 1)
            }
            _ => self.len - 1,
        };
        Ok((est, Exactness::Estimated))
    }

    pub fn total_lines(&self) -> (u64, Exactness) {
        if self.complete {
            return (self.total_newlines.saturating_add(1), Exactness::Exact);
        }
        let min_lines = self.cum.last().copied().unwrap_or(0).saturating_add(1);
        let est = match self.avg_line_bytes() {
            Some(avg) if avg > 0.0 => {
                let by_avg = ((self.len as f64 / avg) as u64).saturating_add(1);
                min_lines.max(by_avg)
            }
            _ => min_lines,
        };
        (est, Exactness::Estimated)
    }

    pub fn eol(&self) -> Option<EolKind> {
        if !self.complete {
            return None;
        }
        if self.total_newlines == 0 {
            Some(EolKind::Unknown)
        } else if self.cr_total == 0 {
            Some(EolKind::Lf)
        } else if self.cr_total == self.total_newlines {
            Some(EolKind::Crlf)
        } else {
            Some(EolKind::Mixed)
        }
    }

    #[cfg(test)]
    pub(crate) fn checkpoints(&self) -> usize {
        self.cum.len()
    }
}

pub struct IndexJob {
    pub generation: u64,
    pub cancel: Arc<AtomicBool>,
    pub rx: Receiver<IndexMsg>,
    handle: Option<JoinHandle<()>>,
}

impl IndexJob {
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for IndexJob {
    fn drop(&mut self) {
        // Cancel only; never block the UI thread by joining.
        self.cancel();
    }
}

pub fn spawn_indexer(
    generation: u64,
    src: Box<dyn TextSource>,
    notify: Box<dyn Fn() + Send>,
) -> IndexJob {
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_w = Arc::clone(&cancel);
    let (tx, rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        let len = src.len();
        if len == 0 {
            if tx
                .send(IndexMsg::Done {
                    generation,
                    total_newlines: 0,
                    cr_total: 0,
                })
                .is_ok()
            {
                notify();
            }
            return;
        }

        let mut buf = vec![0u8; INDEX_READ];
        let mut pos = 0u64;
        let mut total_nl = 0u64;
        let mut cr_total = 0u64;
        let mut pending: Vec<u64> = Vec::new();
        let mut covered = 0u64;
        let mut last_sent = Instant::now();

        while pos < len {
            if cancel_w.load(Ordering::Relaxed) {
                if tx.send(IndexMsg::Cancelled { generation }).is_ok() {
                    notify();
                }
                return;
            }

            let step = (len - pos).min(INDEX_READ as u64) as usize;
            if let Err(e) = src.read_range(pos, &mut buf[..step]) {
                if tx
                    .send(IndexMsg::Failed {
                        generation,
                        error: e.to_string(),
                    })
                    .is_ok()
                {
                    notify();
                }
                return;
            }

            let mut off = 0usize;
            while off < step {
                let rem = step - off;
                let blk_len = rem.min(BLOCK as usize);
                let slice = &buf[off..off + blk_len];
                total_nl = total_nl.saturating_add(memchr::memchr_iter(b'\n', slice).count() as u64);
                cr_total = cr_total.saturating_add(memchr::memchr_iter(b'\r', slice).count() as u64);
                if blk_len == BLOCK as usize {
                    covered = covered.saturating_add(BLOCK);
                    pending.push(total_nl);
                }
                off += blk_len;
            }
            pos = pos.saturating_add(step as u64);

            if !pending.is_empty() && last_sent.elapsed() >= BATCH_INTERVAL {
                let cum_tail = std::mem::take(&mut pending);
                if tx
                    .send(IndexMsg::Batch {
                        generation,
                        cum_tail,
                        covered,
                    })
                    .is_err()
                {
                    return;
                }
                notify();
                last_sent = Instant::now();
            }
        }

        if !pending.is_empty() {
            if tx
                .send(IndexMsg::Batch {
                    generation,
                    cum_tail: pending,
                    covered,
                })
                .is_err()
            {
                return;
            }
            notify();
        }
        if tx
            .send(IndexMsg::Done {
                generation,
                total_newlines: total_nl,
                cr_total,
            })
            .is_ok()
        {
            notify();
        }
    });

    IndexJob {
        generation,
        cancel,
        rx,
        handle: Some(handle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::MemSource;
    use crate::testutil::{XorShift, fixtures};
    use std::sync::atomic::AtomicUsize;

    struct ZeroSource(u64);

    impl TextSource for ZeroSource {
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
            buf.fill(0);
            Ok(())
        }
    }

    struct FailingSource {
        len: u64,
        fail_at: u64,
    }

    impl TextSource for FailingSource {
        fn len(&self) -> u64 {
            self.len
        }
        fn version(&self) -> u64 {
            0
        }
        fn read_range(&self, start: u64, buf: &mut [u8]) -> Result<(), SourceError> {
            if start >= self.fail_at {
                return Err(SourceError::Changed);
            }
            buf.fill(b'a');
            Ok(())
        }
    }

    fn drain(idx: &mut LineIndex, job: &IndexJob) -> usize {
        let mut msgs = 0;
        loop {
            let msg = job.rx.recv_timeout(Duration::from_secs(10)).unwrap();
            msgs += 1;
            let done = matches!(
                msg,
                IndexMsg::Done { .. } | IndexMsg::Failed { .. } | IndexMsg::Cancelled { .. }
            );
            idx.apply(&msg);
            if done {
                break;
            }
        }
        msgs
    }

    #[test]
    fn matches_reference() {
        let mut rng = XorShift::new(2026);
        let rates = [0u32, 1, 5];
        for case in 0..30 {
            let len = if case == 0 {
                0
            } else {
                rng.below(600 * 1024) as usize
            };
            let rate = rates[case % rates.len()];
            let data = fixtures::random_text(&mut rng, len, rate);
            let head = &data[..data.len().min(65_536)];
            let mut idx = LineIndex::new(data.len() as u64, head);
            let job = spawn_indexer(
                1,
                Box::new(MemSource::new(data.clone())),
                Box::new(|| {}),
            );
            drain(&mut idx, &job);
            assert!(idx.is_complete());

            let src = MemSource::new(data.clone());
            let mut checks = 0;
            while checks < 20 && !data.is_empty() {
                let off = rng.below(data.len() as u64);
                let expected = memchr::memchr_iter(b'\n', &data[..off as usize]).count() as u64;
                let (got, ex) = idx.line_of(&src, off).unwrap();
                assert_eq!(ex, Exactness::Exact);
                assert_eq!(got, expected);

                let (start_off, ex2) = idx.offset_of(&src, got).unwrap();
                assert_eq!(ex2, Exactness::Exact);
                assert!(start_off <= off);
                if got < idx.total_newlines {
                    let (next_off, _) = idx.offset_of(&src, got + 1).unwrap();
                    assert!(off < next_off);
                }
                checks += 1;
            }
        }
    }

    #[test]
    fn partial_exact_prefix() {
        let mut data = Vec::new();
        for i in 0..20_000 {
            data.extend_from_slice(format!("line {i:05}\n").as_bytes());
        }
        let src = MemSource::new(data.clone());
        let mut idx = LineIndex::new(data.len() as u64, &data[..65_536]);

        let c1 = memchr::memchr_iter(b'\n', &data[..65_536]).count() as u64;
        let c2 = c1 + memchr::memchr_iter(b'\n', &data[65_536..131_072]).count() as u64;
        idx.apply(&IndexMsg::Batch {
            generation: 1,
            cum_tail: vec![c1, c2],
            covered: 131_072,
        });

        let (l_in, ex_in) = idx.line_of(&src, 100_000).unwrap();
        assert_eq!(ex_in, Exactness::Exact);
        assert_eq!(
            l_in,
            memchr::memchr_iter(b'\n', &data[..100_000]).count() as u64
        );

        let mut prev = 0;
        for off in (131_073..data.len() as u64).step_by(4096) {
            let (l, ex) = idx.line_of(&src, off).unwrap();
            assert_eq!(ex, Exactness::Estimated);
            assert!(l >= prev);
            prev = l;
        }
    }

    #[test]
    fn offset_of_estimate() {
        let mut data = Vec::new();
        for i in 0..20_000 {
            data.extend_from_slice(format!("line {i:05}\n").as_bytes());
        }
        let src = MemSource::new(data.clone());
        let mut idx = LineIndex::new(data.len() as u64, &data[..65_536]);
        let c1 = memchr::memchr_iter(b'\n', &data[..65_536]).count() as u64;
        idx.apply(&IndexMsg::Batch {
            generation: 1,
            cum_tail: vec![c1],
            covered: 65_536,
        });

        let mut prev = 0;
        for line in (c1 + 10..25_000).step_by(500) {
            let (off, ex) = idx.offset_of(&src, line).unwrap();
            assert_eq!(ex, Exactness::Estimated);
            assert!(off < data.len() as u64);
            assert!(off >= prev);
            prev = off;
        }
    }

    #[test]
    fn no_newlines() {
        let data = fixtures::long_line(200_000);
        let mut idx = LineIndex::new(data.len() as u64, &data[..65_536]);
        let job = spawn_indexer(1, Box::new(MemSource::new(data.clone())), Box::new(|| {}));
        drain(&mut idx, &job);
        assert_eq!(idx.total_lines(), (1, Exactness::Exact));
        let src = MemSource::new(data);
        assert_eq!(idx.line_of(&src, 150_000).unwrap(), (0, Exactness::Exact));
    }

    #[test]
    fn eol_from_counts() {
        for (bytes, expected) in [
            (b"a\nb\nc\n".to_vec(), EolKind::Lf),
            (b"a\r\nb\r\n".to_vec(), EolKind::Crlf),
            (b"a\r\nb\n".to_vec(), EolKind::Mixed),
            (b"no newlines".to_vec(), EolKind::Unknown),
        ] {
            let mut idx = LineIndex::new(bytes.len() as u64, &bytes);
            assert_eq!(idx.eol(), None);
            let job = spawn_indexer(1, Box::new(MemSource::new(bytes)), Box::new(|| {}));
            drain(&mut idx, &job);
            assert_eq!(idx.eol(), Some(expected));
        }
    }

    #[test]
    fn worker_cancel() {
        let sixty_four_gib = 64u64 * 1024 * 1024 * 1024;
        let job = spawn_indexer(1, Box::new(ZeroSource(sixty_four_gib)), Box::new(|| {}));
        std::thread::sleep(Duration::from_millis(50));
        job.cancel();
        let t0 = Instant::now();
        let mut saw_cancel = false;
        while t0.elapsed() < Duration::from_secs(2) {
            match job.rx.recv_timeout(Duration::from_millis(500)) {
                Ok(IndexMsg::Cancelled { .. }) => {
                    saw_cancel = true;
                    break;
                }
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        assert!(saw_cancel);
        assert!(t0.elapsed() < Duration::from_millis(500));
    }

    #[test]
    fn worker_batches_throttled() {
        let len = 32u64 * 1024 * 1024;
        let notifies = Arc::new(AtomicUsize::new(0));
        let n2 = Arc::clone(&notifies);
        let t0 = Instant::now();
        let job = spawn_indexer(
            1,
            Box::new(ZeroSource(len)),
            Box::new(move || {
                n2.fetch_add(1, Ordering::Relaxed);
            }),
        );
        let mut batches = 0usize;
        let mut dones = 0usize;
        let mut total_msgs = 0usize;
        loop {
            let msg = job.rx.recv_timeout(Duration::from_secs(10)).unwrap();
            total_msgs += 1;
            match msg {
                IndexMsg::Batch { .. } => batches += 1,
                IndexMsg::Done { .. } => {
                    dones += 1;
                    break;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        let elapsed_ms = t0.elapsed().as_millis() as usize;
        assert_eq!(dones, 1);
        assert!(batches <= elapsed_ms / 100 + 2, "batches={batches}, ms={elapsed_ms}");
        // Wait briefly for the final notify().
        for _ in 0..50 {
            if notifies.load(Ordering::Relaxed) == total_msgs {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(notifies.load(Ordering::Relaxed), total_msgs);
    }

    #[test]
    fn worker_failed() {
        let src = FailingSource {
            len: 8 * 1024 * 1024,
            fail_at: 4 * 1024 * 1024,
        };
        let job = spawn_indexer(1, Box::new(src), Box::new(|| {}));
        let mut failed = false;
        while let Ok(msg) = job.rx.recv_timeout(Duration::from_secs(5)) {
            if matches!(msg, IndexMsg::Failed { .. }) {
                failed = true;
                break;
            }
        }
        assert!(failed);
    }

    #[test]
    fn memory_per_block() {
        let len = 10u64 * 1024 * 1024;
        let mut idx = LineIndex::new(len, &[]);
        let job = spawn_indexer(1, Box::new(ZeroSource(len)), Box::new(|| {}));
        drain(&mut idx, &job);
        assert_eq!(idx.checkpoints(), (len / BLOCK) as usize + 1);
    }
}
