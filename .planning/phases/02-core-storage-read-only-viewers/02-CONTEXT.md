# Phase 2: Core Storage & Read-Only Viewers - Context

**Gathered:** 2026-11-01
**Status:** Ready for planning

<domain>
## Phase Boundary

User can open a file of any size (File > Open, drag & drop, CLI) and immediately scroll, jump and inspect it in a line-based text view or a hex view, select and copy, with the whole file never loaded into memory. Requirements: CORE-01..07, EDIT-05, EDIT-06, EDIT-10, HEX-01, HEX-03, HEX-05, UI-01, UI-05, OPEN-02, OPEN-09. Read-only: editing is Phase 3, tabs are Phase 5 (OPEN-01), encoding switching Phase 5.

</domain>

<decisions>
## Implementation Decisions

### Storage & line model
- D-01: Content access via positional reads (`read_at`/`seek_read`) into a small chunk cache, NOT direct `&[u8]` over mmap. Viewport reads a few KB; the indexer streams 1–4 MB blocks. A truncated/changed file surfaces as a recoverable "file changed on disk" error, never SIGBUS/UB. Resolves Phase 1 review WR-01. (memmap2 may stay only where it is provably safe, or be removed.)
- D-02: Read-only `TextSource` abstraction now (len, read_range, version); the piece table arrives in Phase 3 behind the same interface.
- D-03: Background, cancellable line indexer: one checkpoint (byte offset + line number) per 64 KB block, memchr-based. Until complete, line numbers are estimated from the average line length sampled so far and shown with a `~` marker; they become exact when indexing finishes.
- D-04: Lines longer than 10,000 chars are hard-broken into display rows; a multi-GB single line scrolls by rows and is never laid out whole. This replaces Phase 1's fixed 128-byte rows (fixes review WR-02 End key and WR-04 non-inverse scroll).

### Text view behaviour
- D-05: Word wrap ON by default (wrap to window width), auto-disabled for files > 64 MB with a status-bar notice; user can re-enable. Toggle: View > Word wrap, Alt+Z.
- D-06: Line numbers ON by default, right-aligned gutter; estimated numbers rendered dimmed. Toggle: View > Line numbers.
- D-07: CJK fallback via lazily loaded system fonts (Yu Gothic / PingFang / Noto CJK) on first missing glyph; no bundled CJK font (size budget).
- D-08: Single document at a time in Phase 2; File > Open / drop replaces the current file. Tab strip shows the one open file. Real tabs in Phase 5.

### Hex view, binary detection & jumping
- D-09: Hex layout 16 bytes/row: 64-bit hex offset, two groups of 8 hex bytes, ASCII column with `·` for non-printables. View > Hex view / Ctrl+H; switching keeps the same byte position.
- D-10: Binary heuristic: NUL byte or >30% control bytes in the first 8 KB → open as text with a dismissible banner "This looks like a binary file — [View as hex]". Never auto-switch.
- D-11: Unified Ctrl+G box: `1234` = line, `0x1F00` or `#8192` = byte offset, `50%` = percent; in hex view a bare number is an offset. Jump to line before indexing completes uses the estimate and self-corrects.
- D-12: Long operations show progress in the status bar with ✕ cancel; Esc also cancels. Opening a new file cancels the previous file's indexing.

### Selection, opening & status bar
- D-13: Selection: mouse drag, Shift+arrows/click, double-click word, triple-click line, Ctrl+A; Ctrl+C copies. Hex view selects bytes and copies as hex text.
- D-14: Copy caps: ≤16 MB copies immediately; above that a confirm dialog with size; hard cap 256 MB. Selection stored as offsets, so Ctrl+A on 20 GB is free.
- D-15: File > Open via `rfd` (Linux via xdg-portal, no GTK link); drag & drop takes the first file (multi-file waits for tabs); last directory remembered for the session.
- D-16: Status bar: `Ln 1,234 (~)` · `Col 5` · byte offset · `UTF-8` · `LF`/`CRLF` (detected, display-only) · indexing % with ✕. Startup ms / backend shown only in `--bench` mode.

### Carried from Phase 1 (locked)
- D-17: Toolkit egui/eframe 0.37 + glow; footprint gates: stripped size ≤ 10 MB (Phase 1 gate; project < 15 MB), idle footprint (settled, per-OS comparable metric) ≤ 70 MB, startup ≤ 300 ms warm. Re-measure with `cargo xtask measure` and append to docs/measurements.md.
- D-18: Phase 2 setup task: hard-gate macOS CI (real GL) at idle footprint ≤ 70 MB and startup median ≤ 600 ms; software-GL runners stay report-only; size hard everywhere (user decision 2026-11-01).
- D-19: File::open on a FIFO/device must not block the UI thread (Phase 1 review note): open/stat off the UI thread or reject non-regular files with a clear message.

### Claude's Discretion
- Module layout, chunk-cache size/eviction, exact estimation formula, keyboard shortcuts not listed above, banner/dialog styling within UI-SPEC.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `src/bench.rs`, `src/backend.rs`, `src/args.rs` (lexopt CLI, `--bench`), `src/theme.rs` (UI-SPEC colours, bundled JetBrains Mono), `src/ime.rs` (IME probe state machine), `src/view/snap.rs` (HiDPI pixel snapping).
- `xtask` measure/check-size/sparse harness; `examples/bare.rs` baseline; CI matrix in `.github/workflows/ci.yml`.

### Established Patterns
- eframe 0.37 API: `App::ui`, `egui::Panel::top/bottom`, `MenuBar`, `egui::output::IMEOutput`; no idle repaints (no `request_repaint` without a reason); explicit glow renderer.
- Tests colocated as `#[cfg(test)]` modules; pure logic (rows, snap) kept separate from painting.

### Integration Points
- `src/file.rs` (mmap) and `src/view/rows.rs` (128-byte rows) are replaced by TextSource + line index + new row model.
- `src/view/mod.rs` painter is extended for gutter, wrap, selection, hex mode; `src/app.rs` gains menus, dialogs, drop handling, status bar.

</code_context>

<specifics>
## Specific Ideas

- User feedback after testing Phase 1: text must follow line breaks and window width (wrap toggle), line numbers must be real lines and toggleable, approximate numbers while counting are fine, and select+copy is needed for daily use.

</specifics>

<deferred>
## Deferred Ideas

- Multi-file drop / tabs → Phase 5 (OPEN-01).
- Encoding / EOL switching → Phase 5.
- Cold start, mixed-DPI drag, macOS/Linux IME manual checks → open in 01-HUMAN-UAT.md.

</deferred>
