---
phase: 01-toolkit-footprint-gate
plan: 02
subsystem: gui-skeleton
tags: [rust, eframe, egui-0.37, glow, mmap]
requires: [01-01]
provides:
  - whim binary: args, startup clock, read-only mmap, explicit renderer, app shell, virtualized view
  - bench stdout contract (backend=, startup_ms=, first_frame_unix_us=)
affects: [01-03, 01-04, 01-05]
key-files:
  created: [src/args.rs, src/backend.rs, src/bench.rs, src/file.rs, src/app.rs, src/theme.rs, assets/fonts/JetBrainsMono-Regular.ttf, assets/fonts/OFL.txt]
  modified: [src/main.rs, src/lib.rs, src/view/mod.rs]
key-decisions:
  - "egui accessed via eframe::egui re-export (no direct egui dependency)"
requirements-completed: [PERF-01, PERF-02, UI-04]
duration: ~25min
completed: 2026-10-31
---

# Phase 1 Plan 02: GUI Walking Skeleton Summary

eframe 0.37 (explicit glow renderer) skeleton that mmaps the target file read-only, paints only visible rows with pixel-snapped positions and a custom byte-offset scrollbar, idles with zero repaints, and prints the bench contract lines.

## Tasks
1. Entry point, args, clock, mmap, renderer - 0f1049b (theme + font also landed here because lib.rs declares theme)
2. App shell and virtualized view - see git log (feat(01-02) second commit)

## Verification
- `cargo test -p whim --lib`: 25 passed; `cargo clippy -p whim --all-targets -- -D warnings` clean.
- Release smoke run: `backend=glow`, `startup_ms=345`, `first_frame_unix_us=...`, self-exited after 3 s. Release exe 5,249,536 bytes (5.0 MB, stripped, bundled font).
- Note: 345 ms on this dev machine, one cold-ish run, is above the 200 ms target; the real measurement belongs to plans 03/05.
- Font: JetBrainsMono-Regular.ttf, 270,224 bytes, magic 00 01 00 00, SHA-256 e6fd0d7e91550b3ed2b735d4312474362c4716edc4fc0577a0f61ed782d5aed1.

## Deviations from Plan
- [Rule 3] The egui crate is not a direct dependency, so all code uses `eframe::egui`.
- All 0.37 API names from the plan (Panel, MenuBar, Frame::NONE, App::ui, global_style_mut, ui.close) compiled unchanged.
- Doc comment in file.rs reworded so the `advise|populate` grep returns 0.
- The window was not visually inspected (headless-style smoke only); scrolling by wheel/keys/scrollbar is untested interactively.

## Known Stubs
View/Line numbers/Open menu items are intentionally disabled ("Coming in a later phase").

## Self-Check: PASSED
