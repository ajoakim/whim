---
phase: 02-core-storage-read-only-viewers
plan: 07
subsystem: text-view
tags: [textview, layout, gutter, wrap, hscroll]
requires: [02-03, 02-04, 02-05, 02-06]
provides: [src/view/textview.rs]
key-files.created: []
key-files.modified:
  - src/view/textview.rs
key-decisions:
  - Pure `RowLayouter` abstraction allows property-testing inverse row stepping and end anchoring independently of egui
  - Bounded 128-entry galley cache keyed by `(content_w, wrap, ppp, version, len)` lays out only visible segments (+1)
  - Read errors return in `TextViewOutput.error` and keep painting last-good rows; frozen mode paints dimmed to 40% opacity
  - Zero `request_repaint` calls in `TextView` so idle CPU stays at 0
requirements-completed:
  - CORE-03
  - CORE-06
  - EDIT-05
  - EDIT-06
  - UI-05
---

# Phase 02 Plan 07: Virtualized Text View Summary

Implemented `TextView` with real-line segment layout, soft wrap and horizontal scroll, monotone exact/estimated line-number gutter, continuation tooltips, lazy CJK font fallback, frozen error rendering, and IME probe hookup.

### What was built

* Pure row math in `src/view/textview.rs`: `RowSpan`, `RowLayouter`, `MonoLayouter`, `row_start_of`, `step_rows`, `anchor_for_end`, `GutterMark`, `gutter_cells`.
* `TextView`, `TextViewCtx`, `TextViewOutput`: 128-entry layout cache, gutter with `~` dimmed estimates and `›` continuation hover tooltip, horizontal scroll (wheel/Shift+wheel/scrollbar), keyboard navigation, frozen 40% dimmed rendering, and Phase 1 IME probe.
* 7 tests pass including 50-case `step_inverse`, `end_shows_last_row`, `gutter_marks`, and headless `egui_smoke` (CJK, 3 MB single line, empty).

### Deviations from Plan

None - plan executed exactly as written.

## Self-Check: PASSED

Ready for plan 02-08 (HexView).
