---
phase: 02-core-storage-read-only-viewers
plan: 05
subsystem: scrolling-decoding
tags: [scroll, segment, utf8, scrollbar]
requires: [02-01]
provides: [src/scroll.rs, src/text.rs, src/view/scrollbar.rs]
key-files.created: []
key-files.modified:
  - src/scroll.rs
  - src/text.rs
  - src/view/scrollbar.rs
key-decisions:
  - Stateless boundary set `{0, len} ∪ {n+1} ∪ {adj(p) : p % CAP == 0, no '\n' in [p-CAP, p)}` makes forward and backward stepping exact inverses in O(CAP) reads per step
  - Segment decoder uses `utf8_chunks` (never `from_utf8_lossy`) to maintain an exact display-char to segment-byte offset table for hit-testing and selection
  - Shared `scrollbar` widget computes thumb geometry in `f64` with a 24 pt minimum thumb and drag grab-offset persistence in egui temp memory
requirements-completed:
  - CORE-03
  - CORE-06
---

# Phase 02 Plan 05: Stateless Segment Rule, Decoder & Scrollbar Summary

Implemented the stateless paragraph-boundary rule with property-tested inverse stepping, the `utf8_chunks` segment decoder with char->byte mapping, and the shared f64 scrollbar widget.

### What was built

* `src/scroll.rs`: `CAP = 10_000`, `Segment`, `next_boundary`, `segment_start`, `prev_segment_start`, `segment_at`, `last_segment_start`. 7 tests pass including 300-case `inverse_small_cap`, `inverse_real_cap` with CJK straddling `CAP`, `crlf_boundary`, and 50 MB `long_line_is_o1`.
* `src/text.rs`: `Decoded`, `decode_segment`, `char_for_byte`, `NON_PRINTABLE`, `TAB_CELLS = 4`. 7 tests pass.
* `src/view/scrollbar.rs`: `VBAR_W = 12.0`, `HBAR_H = 8.0`, `MIN_THUMB = 24.0`, `Axis`, `thumb`, `frac_for_thumb`, `scrollbar`. 3 tests pass.

### Deviations from Plan

None - plan executed exactly as written.

## Self-Check: PASSED

Ready for plan 02-06.
