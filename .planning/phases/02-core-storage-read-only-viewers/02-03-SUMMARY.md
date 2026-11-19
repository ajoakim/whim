---
phase: 02-core-storage-read-only-viewers
plan: 03
subsystem: storage
tags: [filesource, positional-io, lru-cache, open-pipeline]
requires: [02-01]
provides: [src/filesource.rs, src/open.rs]
key-files.created: []
key-files.modified:
  - src/filesource.rs
  - src/open.rs
key-decisions:
  - Positional reads (`read_at` on Unix, `seek_read` on Windows) with identity checks replace direct mmap access so external truncation returns `SourceError::Changed` instead of SIGBUS
  - `reopen` / `reopen_for_scan` open fresh handles by path (never `try_clone`) and verify `(len, modified)` identity
  - Used `generation` parameter name instead of `gen` (reserved keyword in Rust edition 2024)
requirements-completed:
  - CORE-01
  - CORE-02
  - HEX-05
---

# Phase 02 Plan 03: Storage & Open Pipeline Summary

Implemented `FileSource` with bounded positional reads and change detection, plus the non-blocking worker-thread open pipeline with binary, UTF-16 BOM, and EOL heuristics.

### What was built

* `src/filesource.rs`: `FileSource` implementing `TextSource` over `read_at`/`seek_read`, a 16 x 64 KiB LRU `ChunkCache`, direct reads above 256 KiB, `reopen`/`reopen_for_scan` with `FILE_FLAG_SEQUENTIAL_SCAN` on Windows, and `(len, modified)` identity verification. 8 tests pass (random reads, cache bound, truncate, grow, reopen, direct read, past-end, empty).
* `src/open.rs`: `classify` via `metadata().is_file()` before `File::open`, `open_blocking` sampling at most 64 KiB, `looks_binary` (NUL or >30% controls in first 8 KiB), `has_utf16_bom`, `eol_of_sample`, UI-SPEC error messages, and `spawn_open` worker. 8 tests pass.

### Deviations from Plan

- **[Rule 3 - Blocking] Rust edition 2024 reserved keyword `gen`**: Renamed parameter/variable `gen` to `generation` in `spawn_open` because `gen` is reserved in edition 2024.

## Self-Check: PASSED

Ready for plan 02-04.
