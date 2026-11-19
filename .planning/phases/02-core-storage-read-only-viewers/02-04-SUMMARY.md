---
phase: 02-core-storage-read-only-viewers
plan: 04
subsystem: indexing
tags: [lineindex, memchr, goto, worker]
requires: [02-01]
provides: [src/lineindex.rs, src/goto.rs]
key-files.created: []
key-files.modified:
  - src/lineindex.rs
  - src/goto.rs
key-decisions:
  - One cumulative `u64` newline count per 64 KiB block keeps index RAM at 128 KiB per GiB of file
  - Worker throttles `Batch` messages to 100 ms and calls `notify()` only after a successful send; `Drop` cancels without joining
  - Used `generation` instead of `gen` (Rust edition 2024 reserved keyword)
requirements-completed:
  - CORE-04
  - CORE-05
  - CORE-07
  - HEX-03
---

# Phase 02 Plan 04: Sparse Line Index & Ctrl+G Grammar Summary

Implemented the background cancellable sparse line index with partial-index estimation, plus the total Ctrl+G parser, resolver, and preview formatter.

### What was built

* `src/lineindex.rs`: `LineIndex`, `Exactness`, `IndexMsg`, `IndexJob`, `spawn_indexer`. Uses `memchr_iter` per 64 KiB block over a reusable 4 MiB buffer, throttles batches to 100 ms, cancels via `AtomicBool`, and maps `line_of`/`offset_of` exactly in the indexed prefix and by average-line-length estimate beyond. 9 tests pass.
* `src/goto.rs`: `GotoTarget`, `parse_goto`, `resolve`, `preview`, `thousands`, `hint`. Handles `1234` (line/offset), `0x1F00`, `#8192`, `50%`, past-end clamping, and UI-SPEC preview copy. 8 tests pass.

### Deviations from Plan

- **[Rule 3 - Blocking] Rust edition 2024 reserved keyword `gen`**: Named field/method `generation` instead of `gen` on `IndexMsg` and `IndexJob`.

## Self-Check: PASSED

Ready for plan 02-05.
