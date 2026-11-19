---
phase: 02-core-storage-read-only-viewers
plan: 06
subsystem: selection-copy
tags: [selection, clipboard, copy-worker]
requires: [02-01]
provides: [src/selection.rs, src/copy.rs]
key-files.created: []
key-files.modified:
  - src/selection.rs
  - src/copy.rs
key-decisions:
  - Selection is pure `(anchor, head)` `u64` offsets; `select_all` does zero I/O regardless of file size
  - `word_bounds` reads a bounded `<= 512` byte window, aligns start past UTF-8 continuation bytes, and expands by `CharClass`
  - Copy caps apply to produced output text (16 MiB immediate, 256 MiB hard cap; hex max ~85.3 MiB raw bytes); worker is cancellable per 1 MiB step and uses zero-copy `String::from_utf8` for valid UTF-8
  - Used `generation` instead of `gen` (Rust edition 2024 reserved keyword)
requirements-completed:
  - EDIT-10
  - CORE-07
---

# Phase 02 Plan 06: Selection & Cancellable Copy Pipeline Summary

Implemented the offset-based selection model with bounded word-boundary scanning, plus the tiered, cancellable copy worker supporting both text and uppercase hex copy.

### What was built

* `src/selection.rs`: `Selection` (`caret`, `range`, `len`, `is_empty`, `extend_to`, `select_all`, `clamp`), `CharClass`, `class_of`, `word_bounds` (`WORD_SCAN = 256`). 8 tests pass.
* `src/copy.rs`: `COPY_IMMEDIATE_MAX = 16 MiB`, `COPY_HARD_CAP = 256 MiB`, `CopyKind`, `CopyTier`, `output_size`, `tier`, `max_bytes_for`, `hex_text`, `CopyMsg`, `CopyJob`, `spawn_copy`. 10 tests pass.

### Deviations from Plan

- **[Rule 3 - Blocking] Rust edition 2024 reserved keyword `gen`**: Named field/method `generation` instead of `gen` on `CopyMsg` and `CopyJob`.

## Self-Check: PASSED

Wave 2 complete. Ready for Wave 3 (plans 02-07 and 02-08).
