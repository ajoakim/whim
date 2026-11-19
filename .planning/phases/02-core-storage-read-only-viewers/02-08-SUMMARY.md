---
phase: 02-core-storage-read-only-viewers
plan: 08
subsystem: hex-view
tags: [hexview, hit-test, selection, scrollbar]
requires: [02-03, 02-05, 02-06]
provides: [src/view/hexview.rs]
key-files.created: []
key-files.modified:
  - src/view/hexview.rs
key-decisions:
  - Fixed 16-byte rows allow exact proportional f64 scrollbar geometry and O(1) hit testing via monospace cell arithmetic without `Galley::cursor_from_pos`
  - Selection rectangles span hex byte pairs plus half-gaps and mirror onto the ASCII column (focused `SELECTION`, unfocused `SELECTION_INACTIVE`)
  - Position-preserving text<->hex switch aligns anchor down to 16 bytes on entry and snaps via `segment_start` on return
requirements-completed:
  - HEX-01
  - HEX-03
---

# Phase 02 Plan 08: Virtualized Hex View Summary

Implemented `HexView` with virtualized 16-byte rows, three columns (offset, two 8-byte hex groups, ASCII), cell-arithmetic hit testing, mirrored selection highlights, proportional scrollbar, and position-preserving switching.

### What was built

* `src/view/hexview.rs`: `HexColumns`, `format_hex_row`, `align_row`, `HexView` (`new`, `reset`, `top`, `set_top_from_text`, `top_to_text`, `top_to_text_segment`, `jump_to`, `scroll_epoch`, `show`).
* Supports mouse click/drag, Shift+click, double-click (1 byte), triple-click (16-byte row), Ctrl+A, keyboard navigation, horizontal scroll when narrow, frozen 40% dimmed rendering, and empty-file message.
* 6 tests pass including formatting, hit-test arithmetic, alignment/switch, scrollbar math, hex copy pairs, and headless `egui_smoke_hex_and_empty`.

### Deviations from Plan

None - plan executed exactly as written.

## Self-Check: PASSED

Wave 3 complete. Ready for Wave 4 (plans 02-09, 02-10, 02-11, 02-12).
