---
phase: 02-core-storage-read-only-viewers
plan: 09
subsystem: text-selection
tags: [textview, selection, hit-test, word-bounds]
requires: [02-07]
provides: [textview-selection-interaction]
key-files.created: []
key-files.modified:
  - src/view/textview.rs
key-decisions:
  - Mouse click hit-tests via `Galley::cursor_from_pos` and maps `CCursor.index` through `Decoded::byte_for_char` to an absolute `u64` byte offset
  - Double-click delegates to `selection::word_bounds` (bounded 256-byte scan); triple-click selects `[segment_start, next_boundary)`
  - Selection rectangles render per visual row in `SELECTION` (focused) or `SELECTION_INACTIVE` (unfocused) using `Row::x_offset`
requirements-completed:
  - EDIT-10
---

# Phase 02 Plan 09: TextView Selection Interaction Summary

Wired mouse and keyboard selection, caret hit-testing, word/paragraph selection, and per-row selection highlight painting into `TextView`.

### What was built

* `src/view/textview.rs`: added `increment_scroll_epoch`, `handle_click` (`Galley::cursor_from_pos` + `byte_for_char`), `handle_word_select` (`selection::word_bounds`), `handle_triple_select` (`scroll::segment_start`..`next_boundary`), keyboard `Ctrl+A`, `Shift+Arrow`/`Home`/`End`/`PageUp`/`PageDown`, and per-row selection rect painting.
* Added `word_and_triple_select_and_click_hit_test` test verifying word bounds, paragraph bounds, click hit-testing across rows, and `scroll_epoch` increment. All 8 `view::textview` tests pass.

### Deviations from Plan

None - plan executed exactly as written.

## Self-Check: PASSED

Ready for plan 02-10.
