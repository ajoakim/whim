---
phase: 02-core-storage-read-only-viewers
plan: 10
subsystem: document-state
tags: [doc, cleanup, worker-open, tab-strip, binary-banner]
requires: [02-03, 02-05, 02-07, 02-08]
provides: [src/doc.rs, worker-driven-open]
key-files.created: []
key-files.modified:
  - Cargo.toml
  - Cargo.lock
  - src/lib.rs
  - src/main.rs
  - src/app.rs
  - src/view/mod.rs
  - src/doc.rs
  - src/theme.rs
key-decisions:
  - Removed direct `memmap2` dependency and deleted `src/file.rs` and `src/view/rows.rs`; `memmap2` remains only transitively inside `fontique` (system font file reading), never on user files
  - `Doc` owns `FileSource`, `LineIndex`, `IndexJob`, `CopyJob`, `TextView`, `HexView`, `ViewMode`, `Selection`, binary banner state, and `PendingJump` (Q8)
  - Files > 64 MiB auto-disable word wrap on open with a 6-second transient status notice (D-05)
requirements-completed:
  - CORE-02
  - UI-01
---

# Phase 02 Plan 10: Legacy Removal & Doc State Summary

Removed the Phase 1 mmap and 128-byte-row modules (`src/file.rs`, `src/view/rows.rs`, direct `memmap2` dep) and replaced them with `Doc` state, worker-driven file opening, tab strip, binary banner, and position-preserving text<->hex mode switching.

### What was built

* Deleted `src/file.rs` and `src/view/rows.rs`; removed `memmap2` from `Cargo.toml`.
* `src/doc.rs`: `Doc`, `ViewMode`, `PendingJump`, `LARGE_WRAP_LIMIT = 64 MiB`, `Doc::new`, `reset_for_reload`, `cancel_index`, `cancel_copy`, `handle_index_msg` (with Q8 pending-jump re-anchor), `handle_copy_msg`, `toggle_mode`. 4 unit tests pass.
* `src/main.rs`: passes CLI path to `WhimApp::new` for non-blocking worker open (`open::spawn_open`).

### Deviations from Plan

- **[Note] Transitive `memmap2` in `Cargo.lock`**: `memmap2` is removed from `Cargo.toml` and no Whim code uses it; `Cargo.lock` retains `memmap2` solely as a transitive dependency of `fontique` (via `egui_system_font_provider`).

## Self-Check: PASSED

Ready for plan 02-11 (shell completion & UAT checkpoint).
