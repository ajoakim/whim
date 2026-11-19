---
phase: 02-core-storage-read-only-viewers
plan: 01
subsystem: core
tags: [storage, architecture]
requires: []
provides: [src/source.rs, src/testutil.rs, src/fonts.rs, module-skeleton]
key-files.created:
  - src/source.rs
  - src/testutil.rs
  - src/fonts.rs
key-files.modified:
  - Cargo.toml
  - src/lib.rs
  - src/main.rs
  - src/theme.rs
key-decisions:
  - TextSource abstracts read-only positional access with clamping and overflow prevention
  - eframe system font fallback is disabled and manually invoked via FontFallback state machine on first missing glyph to preserve zero-overhead ASCII startup
  - TempDir test util uses single `create_dir` and includes process/nanos logic to guarantee safety and uniqueness per Phase 1 lesson WR-05
requirements-completed:
  - CORE-02
  - UI-05
---

# Phase 02 Plan 01: Foundation and Module Skeleton Summary

Laid the foundation for Phase 2, registering all new modules, defining the `TextSource` contract for chunked file reads without whole-file allocation, and implementing a lazy CJK font fallback.

### What was built

* `TextSource` abstraction (`src/source.rs`) established to back the `FileSource` positional chunk cache. Included `MemSource` for immediate testing.
* Deterministic fixture generator (`XorShift`, `fixtures`) and safe temporary directories (`TempDir`) implemented for upcoming Wave 2 tests.
* Egui `system_font_fallback` disabled in `main.rs`, replaced with a `FontFallback` state machine (`src/fonts.rs`) that only installs the heavy OS font provider when a non-ASCII missing glyph is actually encountered.
* New theme constants added (`MUTED_DIM`, `SELECTION`, `ROW_HEIGHT`, etc.).
* Crate dependency delta applied: added `clipboard` to `eframe`, `rfd` (xdg-portal), `memchr`, `egui_system_font_provider`.

### Deviations from Plan

None - plan executed exactly as written.

Ready for next wave.
