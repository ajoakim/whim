//! Per-tab document state (single tab in Phase 2, structured for Phase 5).

use crate::copy::{CopyJob, CopyMsg};
use crate::filesource::FileSource;
use crate::goto::{self, GotoTarget};
use crate::lineindex::{IndexJob, IndexMsg, LineIndex, spawn_indexer};
use crate::open::Opened;
use crate::selection::Selection;
use crate::source::{EolKind, TextSource};
use crate::view::hexview::HexView;
use crate::view::textview::TextView;
use std::path::PathBuf;

/// Files larger than 64 MiB open with word wrap disabled by default (D-05).
pub const LARGE_WRAP_LIMIT: u64 = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Text,
    Hex,
}

#[derive(Clone, Copy, Debug)]
pub struct PendingJump {
    pub target: GotoTarget,
    pub saved_scroll_epoch: u64,
}

pub struct Doc {
    pub path: PathBuf,
    pub name: String,
    pub source: FileSource,
    pub index: LineIndex,
    pub index_job: Option<IndexJob>,
    pub copy_job: Option<CopyJob>,
    pub text_view: TextView,
    pub hex_view: HexView,
    pub mode: ViewMode,
    pub sel: Selection,
    pub binary: bool,
    pub utf16_bom: bool,
    pub binary_banner_dismissed: bool,
    pub pending_jump: Option<PendingJump>,
    pub eol: EolKind,
    pub notice: Option<&'static str>,
    pub frozen: bool,
    pub wrap_auto_disabled: bool,
    pub generation: u64,
}

impl Doc {
    /// Creates a new document from an [`Opened`] result and starts its background line indexer
    /// using `notify` to wake the UI on batched progress.
    pub fn new(opened: Opened, generation: u64, notify: Box<dyn Fn() + Send>) -> Self {
        let len = opened.len;
        let index = LineIndex::new(len, &opened.head);
        let index_job = if len > 0 {
            opened
                .source
                .reopen_for_scan()
                .ok()
                .map(|scan_src| spawn_indexer(generation, Box::new(scan_src), notify))
        } else {
            None
        };

        let mut text_view = TextView::new();
        let wrap_auto_disabled = len > LARGE_WRAP_LIMIT;
        if wrap_auto_disabled {
            text_view.wrap = false;
        }

        Self {
            path: opened.path,
            name: opened.name,
            source: opened.source,
            index,
            index_job,
            copy_job: None,
            text_view,
            hex_view: HexView::new(),
            mode: ViewMode::Text,
            sel: Selection::caret(0),
            binary: opened.binary,
            utf16_bom: opened.utf16_bom,
            binary_banner_dismissed: false,
            pending_jump: None,
            eol: opened.eol,
            notice: None,
            frozen: false,
            wrap_auto_disabled,
            generation,
        }
    }

    pub fn cancel_index(&mut self) {
        if let Some(job) = self.index_job.take() {
            job.cancel();
        }
        self.index.mark_cancelled();
    }

    pub fn cancel_copy(&mut self) {
        if let Some(job) = self.copy_job.take() {
            job.cancel();
        }
    }

    /// Resets the document in place after a reload, cancelling any running workers and clamping
    /// the anchor/selection to the new length.
    pub fn reset_for_reload(
        &mut self,
        opened: Opened,
        generation: u64,
        notify: Box<dyn Fn() + Send>,
    ) {
        self.cancel_index();
        self.cancel_copy();
        let prev_top = match self.mode {
            ViewMode::Text => self.text_view.top(),
            ViewMode::Hex => self.hex_view.top(),
        };
        self.path = opened.path;
        self.name = opened.name;
        self.source = opened.source;
        self.index = LineIndex::new(opened.len, &opened.head);
        self.index_job = if opened.len > 0 {
            self.source
                .reopen_for_scan()
                .ok()
                .map(|scan_src| spawn_indexer(generation, Box::new(scan_src), notify))
        } else {
            None
        };
        self.text_view.reset();
        self.hex_view.reset();
        self.wrap_auto_disabled = opened.len > LARGE_WRAP_LIMIT;
        if self.wrap_auto_disabled {
            self.text_view.wrap = false;
        }
        let clamped = if opened.len == 0 {
            0
        } else {
            prev_top.min(opened.len - 1)
        };
        let _ = self.text_view.jump_to(&self.source, clamped);
        self.hex_view.jump_to(clamped);
        self.sel.clamp(opened.len);
        self.binary = opened.binary;
        self.utf16_bom = opened.utf16_bom;
        self.binary_banner_dismissed = false;
        self.pending_jump = None;
        self.eol = opened.eol;
        self.frozen = false;
        self.generation = generation;
    }

    /// Processes an [`IndexMsg`], ignoring messages from stale generations.
    /// Returns `Some(err_msg)` if indexing failed.
    pub fn handle_index_msg(&mut self, msg: IndexMsg) -> Option<String> {
        if msg.generation() != self.generation {
            return None;
        }
        let mut failed = None;
        let finished = matches!(
            msg,
            IndexMsg::Done { .. } | IndexMsg::Failed { .. } | IndexMsg::Cancelled { .. }
        );
        if let IndexMsg::Failed { ref error, .. } = msg {
            failed = Some(error.clone());
        }
        self.index.apply(&msg);
        if finished {
            self.index_job = None;
            if let Some(eol) = self.index.eol() {
                self.eol = eol;
            }
            // Q8: re-anchor a pending line jump if the user has not scrolled since.
            if self.index.is_complete()
                && let Some(pj) = self.pending_jump.take()
                && self.text_view.scroll_epoch == pj.saved_scroll_epoch
                && let Ok(res) = goto::resolve(pj.target, &self.source, &self.index)
            {
                let _ = self.text_view.jump_to(&self.source, res.offset);
                self.hex_view.jump_to(res.offset);
            }
        }
        failed
    }

    /// Processes a [`CopyMsg`], returning `Some(Ok(text))` on Done, `Some(Err(msg))` on Failed.
    pub fn handle_copy_msg(&mut self, msg: CopyMsg) -> Option<Result<String, String>> {
        let active_gen = self.copy_job.as_ref().map(|j| j.generation)?;
        if msg.generation() != active_gen {
            return None;
        }
        match msg {
            CopyMsg::Progress { .. } => None,
            CopyMsg::Done { text, .. } => {
                self.copy_job = None;
                Some(Ok(text))
            }
            CopyMsg::Failed { error, .. } => {
                self.copy_job = None;
                Some(Err(error))
            }
            CopyMsg::Cancelled { .. } => {
                self.copy_job = None;
                None
            }
        }
    }

    /// Toggles between Text and Hex modes while preserving the current byte position (D-09).
    pub fn toggle_mode(&mut self) {
        match self.mode {
            ViewMode::Text => {
                self.hex_view.set_top_from_text(self.text_view.top());
                self.mode = ViewMode::Hex;
            }
            ViewMode::Hex => {
                let off = self.hex_view.top_to_text();
                let _ = self.text_view.jump_to(&self.source, off);
                self.mode = ViewMode::Text;
            }
        }
    }

    pub fn current_top(&self) -> u64 {
        match self.mode {
            ViewMode::Text => self.text_view.top(),
            ViewMode::Hex => self.hex_view.top(),
        }
    }

    pub fn len(&self) -> u64 {
        self.source.len()
    }

    pub fn is_empty(&self) -> bool {
        self.source.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open::open_blocking;
    use crate::testutil::TempDir;

    #[test]
    fn doc_new_and_cancel_index() {
        let dir = TempDir::new();
        let path = dir.file("sample.txt", b"hello\nworld\n");
        let opened = open_blocking(&path).unwrap();
        let mut doc = Doc::new(opened, 1, Box::new(|| {}));
        assert_eq!(doc.name, "sample.txt");
        assert_eq!(doc.mode, ViewMode::Text);
        assert!(!doc.binary);
        assert!(!doc.wrap_auto_disabled);
        assert!(doc.index_job.is_some());

        doc.cancel_index();
        assert!(doc.index_job.is_none());
        assert!(doc.index.is_cancelled());
    }

    #[test]
    fn handle_index_msg_ignores_stale_gen() {
        let dir = TempDir::new();
        let path = dir.file("a.txt", b"one\ntwo\n");
        let opened = open_blocking(&path).unwrap();
        let mut doc = Doc::new(opened, 5, Box::new(|| {}));
        doc.handle_index_msg(IndexMsg::Done {
            generation: 4,
            total_newlines: 99,
            cr_total: 0,
        });
        assert!(!doc.index.is_complete());

        doc.handle_index_msg(IndexMsg::Done {
            generation: 5,
            total_newlines: 2,
            cr_total: 0,
        });
        assert!(doc.index.is_complete());
        assert!(doc.index_job.is_none());
    }

    #[test]
    fn reset_for_reload_resets_state() {
        let dir = TempDir::new();
        let path = dir.file("r.txt", b"alpha\nbeta\ngamma\n");
        let o1 = open_blocking(&path).unwrap();
        let mut doc = Doc::new(o1, 1, Box::new(|| {}));
        doc.frozen = true;
        doc.binary_banner_dismissed = true;

        let o2 = open_blocking(&path).unwrap();
        doc.reset_for_reload(o2, 2, Box::new(|| {}));
        assert!(!doc.frozen);
        assert!(!doc.binary_banner_dismissed);
        assert_eq!(doc.generation, 2);
    }

    #[test]
    fn mode_switch_preserves_position() {
        let dir = TempDir::new();
        let path = dir.file("m.txt", b"0123456789abcdef\nsecond line\n");
        let opened = open_blocking(&path).unwrap();
        let mut doc = Doc::new(opened, 1, Box::new(|| {}));
        doc.text_view.jump_to(&doc.source, 17).unwrap();
        doc.toggle_mode();
        assert_eq!(doc.mode, ViewMode::Hex);
        assert_eq!(doc.hex_view.top(), 16);
        doc.toggle_mode();
        assert_eq!(doc.mode, ViewMode::Text);
        assert_eq!(doc.text_view.top(), 0);
    }
}
