---
phase: 02-core-storage-read-only-viewers
plan: 11
subsystem: app-shell
tags: [dialogs, status-bar, drag-drop, rfd, uat]
requires: [02-06, 02-10]
provides: [src/dialogs.rs, src/status.rs, full-phase2-shell]
key-files.created: []
key-files.modified:
  - src/dialogs.rs
  - src/status.rs
  - src/app.rs
  - src/main.rs
key-decisions:
  - Native `rfd::FileDialog::pick_file()` runs on a background thread (`open_dialog_worker`) seeded with session `last_dir`, waking egui via `ctx.request_repaint()`
  - Status bar implements exact UI-SPEC collapse order (drop EOL -> Encoding -> Size -> Col -> Offset; Line and indexing progress never drop)
  - Copy confirmation modal handles both `Confirm` (> 16 MiB) and `OverCap` (> 256 MiB output) with Esc dismissal and dimmed scrim
  - Esc priority enforced: Go-to box > copy dialog > About > cancel copy > cancel indexing
requirements-completed:
  - UI-01
  - OPEN-02
  - OPEN-09
---

# Phase 02 Plan 11: Shell Integration, Dialogs & UAT Summary

Wired the full Phase 2 desktop shell — non-blocking `File > Open`, drag & drop, collapsible status bar with indexing progress and cancel, copy-size confirmation modal, Go-to box with live preview, and menu/shortcut controls — and completed manual UAT.

### What was built

* `src/dialogs.rs`: `CopyDialog`, `CopyResult`, `open_dialog_worker` (background `rfd::FileDialog`), and `confirm_copy_ui` (modal scrim + 400 pt centered window for `Confirm` and `OverCap`). 2 unit tests pass.
* `src/status.rs`: `Status`, `TransientMsg`, `copy_size_text`, `eol_label`, `indexing_text`, `fit_segments` (collapse order), and `render_status_bar` (2 pt accent progress bar + `✕` cancel button + `--bench` right slot). 4 unit tests pass.
* `src/app.rs`: full shell wiring for menus (`File`, `Edit`, `View`, `Help`), shortcuts (`Ctrl+O/Q/C/G/H`, `Alt+Z`, `Esc`), drag & drop (`dropped_files` path-only), tab strip, dismissible binary banner, Go-to box with live preview and Q8 pending-jump re-anchor, copy tiers -> `ctx.copy_text`, and worker draining.
* Manual UAT checkpoint approved by user.

### Deviations from Plan

None - plan executed exactly as written.

## Self-Check: PASSED

Ready for plan 02-12 (measurements, validation/gate docs, and CI verification).
