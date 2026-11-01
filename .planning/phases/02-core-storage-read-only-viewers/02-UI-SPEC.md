---
phase: 2
slug: core-storage-read-only-viewers
status: draft
shadcn_initialized: false
preset: none
created: 2026-11-01
extends: ../01-toolkit-footprint-gate/01-UI-SPEC.md
---

# Phase 2 — UI Design Contract

> Native Rust app (egui/eframe 0.37, glow). Values are egui points, `Color32`, `FontId`. This file EXTENDS the approved Phase 1 contract; every Phase 1 token (spacing 4/8/16/24, palette, 14pt JetBrains Mono, 24pt bars, 20pt rows) is locked and not redefined here. Sources: 02-CONTEXT.md D-01..D-19, 01-UI-SPEC.md, `src/theme.rs`. Anything else is Claude's discretion, marked "(default)".

**Visual anchor:** the text/hex content area. The gutter, banner, status bar and dialogs are recessive; only selection and the single accent-coloured progress/focus elements draw attention.

---

## Design System

| Property | Value |
|----------|-------|
| Tool | none (native egui) |
| Components | `egui::Panel::top` (menu + tab strip), `egui::Panel::bottom` (status bar), `CentralPanel` (custom painter view), `MenuBar`, `egui::Window` (modal dialogs), `ProgressBar` NOT used (custom 2pt bar, see Status bar) |
| Icons | none; Unicode glyphs only: `✕` (cancel), `~` (estimate), `›` (hard-break), `·` (non-printable). `✕` and `›` must exist in egui's default fonts; if `✕` is missing, fall back to the egui-bundled emoji-icon font (already in `default_fonts`) |
| Fonts | unchanged: UI = egui proportional; content/gutter/hex = JetBrains Mono 14 |
| Theme | dark only (Phase 8 adds light) |

New theme constants to add in `src/theme.rs` (derived, not new hues):

| Const | Value | Use |
|-------|-------|-----|
| `MUTED_DIM` | `MUTED` at alpha 153 (60%) | estimated line numbers, hex ASCII `·` placeholders |
| `SELECTION` | `Color32::from_rgba_unmultiplied(0x4C,0x9A,0xFF,89)` (= existing `selection.bg_fill`) | text and hex selection |
| `SELECTION_INACTIVE` | same RGB at alpha 51 (20%) | selection when window/view unfocused |
| `OVERLAY_SCRIM` | `Color32::from_black_alpha(128)` | behind modal dialogs |
| `DROP_FILL` | accent RGB at alpha 26 (10%) | drag-over overlay fill |

---

## Spacing Scale

Phase 1 scale holds (4/8/16/24). Phase 2 additionally uses xl and 2xl.

| Token | Value | Phase 2 usage |
|-------|-------|---------------|
| xs | 4 | gap between status segment text and divider; banner button vertical padding |
| sm | 8 | gutter-to-text gap; banner inner gap; dialog button gap |
| md | 16 | gutter left padding; content left inset (text starts 16pt after gutter's right edge divider region, see below); dialog body spacing |
| lg | 24 | tab strip height; banner height; hex column group gap |
| xl | 32 | dialog inner margin |
| 2xl | 48 | hex offset-to-bytes column gap is NOT this; used only for Ctrl+G box width padding (see Go-to) |
| 3xl | 64 | not used |

Exceptions: row height remains 20pt (14 x 1.4). Progress bar 2pt high and divider/border strokes 1pt are strokes, not spacing. Drop-affordance border 2pt stroke.

### Layout contract

Top to bottom, fixed:
1. Menu bar `Panel::top`, 24pt.
2. Tab strip `Panel::top`, 24pt, directly below menu bar (Phase 1 forbade stubbing it; Phase 2 introduces it per D-08).
3. Binary banner, 24pt, only when shown, inside CentralPanel above the viewer (pushes the viewer down; not a floating overlay).
4. Viewer (CentralPanel, zero margin, painter fills rect).
5. Status bar `Panel::bottom`, 24pt.

Window min 480x320 stays; the status bar collapses segments by priority (see Status bar) so 480pt never clips.

---

## Typography

No new sizes or weights: Body 14, Label/Small 12, Editor mono 14; Regular only (400). Semibold is still reserved and unused.

| Element | Style |
|---------|-------|
| Line numbers, hex offsets | Monospace 14, `MUTED` (exact) / `MUTED_DIM` (estimated) |
| Content text | Monospace 14, `TEXT` |
| Hex bytes | Monospace 14, `TEXT`; zero bytes `00` in `MUTED` (default) |
| Hex ASCII column | Monospace 14, printable `TEXT`, non-printable `·` in `MUTED_DIM` |
| Tab title | Body 14 |
| Status segments | Small 12, `MUTED` |
| Banner text / dialog body | Body 14 |
| Dialog title | Body 14 (no bold available) |

---

## Color

Locked palette: 60% `#1E1F22`, 30% `#2B2D31`, accent `#4C9AFF`, destructive `#F2555A`. No new hues.

Accent reserved for (Phase 1 list retained, Phase 2 additions marked +):
1. Text and hex selection background at 35% alpha (+ hex ASCII mirror selection, same value).
2. IME preedit underline / caret (carried).
3. Keyboard focus ring.
4. Hovered menu item text.
5. + Active tab 2pt bottom underline.
6. + Indexing progress bar fill (2pt).
7. + Drag-over drop affordance border (2pt) and 10% fill.
8. + Primary (default) dialog/banner button text and focus ring ("View as hex", "Copy").

Destructive `#F2555A` reserved for: error text (file changed on disk, cannot open, copy exceeds hard cap). Not used on buttons: Phase 2 has no destructive actions (copy confirm is a size warning, not destructive).

Banner uses `SECONDARY` fill and `SEPARATOR` 1pt bottom border (not accent, not a warning hue) to stay recessive.

---

## Interaction & Visual Contract

### Tab strip (single file, D-08)
- 24pt, fill `SECONDARY`, 1pt `SEPARATOR` bottom border.
- One tab at left, 8pt left margin: filename (Body 14, `TEXT`), middle-truncated with `…` at max 240pt; 8pt horizontal padding; 2pt `ACCENT` underline along the tab bottom. Full path in hover tooltip.
- No close button, no `+`, no reorder in Phase 2 (Phase 5). Empty state (no file): strip is hidden entirely (menu bar only), so the empty screen stays minimal.
- Tab title prefix `⌀ ` is NOT used. While the file is changed-on-disk error, title gets no marker; the error lives in the viewer and status bar.

### Line-number gutter (D-03, D-06)
- Default ON; toggled by View > Line numbers. Hidden: text inset returns to 16pt.
- Width: computed from digit count of the max line number (exact total if indexed, else the estimated total), minimum 4 digits, in monospace cells, plus 16pt left padding and 8pt right padding. Width only grows during a session of one file (no jitter while scrolling) and uses a thousands separator NEVER (monospace alignment).
- Right-aligned numbers, 1-based. Background = dominant (same as editor, no separate fill); a 1pt `SEPARATOR` vertical line at gutter right edge; text begins 8pt after the line.
- Exact numbers: `MUTED`. Estimated numbers (indexing incomplete, row beyond the indexed prefix): `MUTED_DIM` and prefixed with `~` (e.g. `~48213`), the `~` counts in the right-aligned width. Rows inside the already-indexed prefix show exact (non-tilde) numbers. When indexing completes, tilde and dimming drop on the next frame with no animation and no layout shift (gutter width reserves the `~` cell while indexing).
- Wrapped continuation rows (soft wrap, same logical line): gutter blank.
- Hard-broken continuation rows (line > 10,000 chars, D-04): gutter shows `›` in `MUTED`, right-aligned, instead of a number. Rationale: distinguishes "this line is so long it was split" from a normal wrap, without extra chrome. Tooltip on hover over the `›`: "Long line continued ({n} chars/row)" (default).
- Current line (caret/first selected line) number rendered in `TEXT` instead of `MUTED` (default; no background highlight).

### Word wrap rendering (D-04, D-05)
- Wrap ON default; wraps at content width = viewer width - gutter - 16pt right inset, at word boundaries where possible, else at character boundary.
- Hard break at 10,000 chars regardless of wrap setting. Wrap OFF: lines truncate at 10,000-char rows, horizontal scrolling is NOT provided in Phase 2 (each over-wide line shows as one row up to the viewport edge; content beyond is reachable only by turning wrap on). Default; flag as known limitation, and show it in the View menu tooltip: "Lines longer than the window are cut off when wrap is off."
- Auto-disable when file > 64 MB: status bar left notice "Word wrap off for large files" (Label 12, `MUTED`) shown 6 s then fades out by simply disappearing (no animation, one `request_repaint_after(6s)`); View > Word wrap shows unchecked and remains toggleable.
- Row layout rule: layout only viewport rows + 1; row Y pixel-snapped via existing `view/snap.rs`.
- Control chars and invalid bytes render as `·` in `MUTED_DIM` (Phase 1 rule, retained). Tabs render as 4 cells (default).

### Hex view layout (D-09, HEX-01)
16 bytes per row, 20pt row height, monospace cell width `w`:

```
 00000000000012A0  4C 9A FF 00 12 AB CD EF   01 02 03 04 05 06 07 08  |L.......  ........|
 ^offset(16 hex)   ^8 bytes (sep 1 cell)  ^24pt group gap             ^ASCII (16 cells)
```
- Offset column: 16 hex digits, uppercase, `MUTED`, 16pt left padding, then 16pt gap.
- Bytes: two groups of 8, `XX` separated by 1 cell (`w`); 24pt gap between groups; 16pt gap to ASCII.
- ASCII column: 16 cells, printable 0x20..0x7E as-is, others `·` `MUTED_DIM`; no `|` delimiters drawn, a 1pt `SEPARATOR` vertical line at its left edge.
- Column header row: none (default; minimal). Offset header omitted.
- Total min width ≈ 16 + 16w+16 + 16w + 15w + 24 + 16 + 16w ≈ 63 cells + 72pt; at JetBrains Mono 14 (w≈8.4pt) ≈ 600pt. Below that width the view scrolls horizontally is NOT supported; content clips at the right edge (min window 480 shows ASCII partially; default window 1024 fine). Default.
- Switching text<->hex (Ctrl+H) preserves byte position: the top visible row's start byte maps to the hex row containing it, and back to the logical line containing it. Gutter toggle does not affect hex.
- Hex view is available for any file; word wrap and line numbers menu items are disabled (greyed) while in hex view.

### Selection colours (D-13)
- Text view and hex byte cells: `SELECTION` (accent 35%) rect painted behind glyphs, full row height (20pt), no outline. In hex view the byte cell rect spans the two digits plus the half-gap on each side so contiguous selections have no seams; the matching ASCII cells highlight with the same fill (mirrored).
- Window unfocused: `SELECTION_INACTIVE`.
- Selection across rows paints per-row rects (never one huge rect), clipped to viewport.
- Selected-text colour stays `TEXT` (contrast on 35% accent over `#1E1F22` ≥ 7:1).
- Double-click word, triple-click line, Shift+click/arrows, Ctrl+A per D-13. Hex view: click/drag/Shift select bytes; double-click selects the byte, triple-click selects the 16-byte row.
- Copy in hex view produces uppercase hex pairs separated by single spaces (e.g. `4C 9A FF`).

### Binary-file banner (D-10)
- 24pt high, full viewer width, `SECONDARY` fill, 1pt `SEPARATOR` bottom border, 16pt left padding.
- Content: "This looks like a binary file." (Body 14, `TEXT`) · 8pt gap · button "View as hex" (accent text, hover underline, primary) · right-aligned `✕` dismiss button (24x24 hit target, `MUTED`, tooltip "Dismiss").
- Dismiss is per-file-open (not persisted). Esc does not dismiss it (Esc is reserved for cancel; see below). Never auto-switches.
- Banner stays dismissed after switching to hex and back.

### Go-to box, Ctrl+G (D-11)
- `egui::Window`-style popup anchored top-center of the viewer, 8pt below the top of the viewer (below banner if present), fixed 320pt wide (default), no title bar, `SECONDARY` fill, 1pt `SEPARATOR` border, 8pt inner margin.
- Contents: single-line `TextEdit`, monospace 14, auto-focused, hint text (muted): text view "Line, 0xOffset, #Offset or %" ; hex view "Offset, 0xOffset, #Offset or %". Below the field, an optional 12pt `MUTED` line previewing the interpretation live: "Line 1,234", "Byte 8,192 (0x2000)", "50% of file". Invalid input: that line becomes `DESTRUCTIVE` "Not a valid position" and Enter does nothing.
- Enter jumps and closes; Esc or click outside closes; Ctrl+G again re-focuses. Jumps beyond end clamp to the last line/byte with preview "Past end, will go to last line" (default).
- Before indexing completes a line jump shows preview "Line 5,000,000 (estimated, ~)" and the view re-corrects after indexing (scroll position re-anchored to the same line; no popup).
- Menu entry: Edit-less app, so lives under View > Go to... (Ctrl+G).

### Copy-size confirm dialog (D-14)
- Modal `egui::Window`, centered, fixed width 400pt (default), 32pt inner margin (xl), `OVERLAY_SCRIM` behind, fill `SECONDARY` with `SEPARATOR` 1pt border (default).
- Title "Copy large selection?". Body: "The selection is {size}. Copying it may take a moment and use that much memory." Hard-cap variant (> 256 MB, no copy buttons): body "The selection is {size}, which is over the 256 MB copy limit. Select a smaller range." (size in destructive colour), single button "OK".
- Buttons right-aligned, 8pt gap: "Cancel" (secondary) and "Copy {size}" (primary, accent text, Enter-activates). Esc = Cancel.
- Progress for the copy itself appears in the status bar (D-12) with ✕.

### "File changed on disk" error state (D-01)
- Triggered by a failed/short positional read or a changed length/mtime.
- Viewer: the last good rows stay painted but dimmed to 40% alpha; centered over them a plain block (no card), 14pt `DESTRUCTIVE` heading "File changed on disk" and 14pt `MUTED` body "{filename} was modified or truncated while open. Reload to view its current contents." with a primary button "Reload" (accent text) and secondary "Close file". No modal.
- Status bar left slot: "File changed on disk" in `DESTRUCTIVE`; position/size segments blank to "—".
- Reload re-opens the same path (cancels indexing, resets scroll to previous byte offset clamped to new length). Never panics, never partially paints garbage.

### Status bar (D-12, D-16)
24pt, `SECONDARY`, 12pt `MUTED`, segments separated by 8pt gaps and 1pt `SEPARATOR` dividers (as Phase 1). Left to right:

| # | Segment | Example | Notes |
|---|---------|---------|-------|
| 1 | Line | `Ln 1,234` / `Ln ~48,213` | thousands separators here (proportional font); `~` when estimated; hex view: replaced by `Offset 0x00000000000012A0` |
| 2 | Column | `Col 5` | text view only; hex view shows `Sel 16 B` when selection exists |
| 3 | Byte offset | `Offset 0x12A0` | text view; hex view merges into segment 1 |
| 4 | Size | `20.0 GB` | kept from Phase 1 |
| 5 | Encoding | `UTF-8` | display only |
| 6 | EOL | `LF` / `CRLF` / `Mixed` | detected, display only |
| 7 | Right-aligned: indexing | `Indexing 42%` + 80x2pt `ACCENT` bar on `SEPARATOR` track + `✕` | visible only while running |

- Transient message slot (notices, errors) sits left of segment 1 and replaces it while displayed.
- Collapse order as width shrinks: drop EOL, then Encoding, then Size, then Col, then Offset (Line and indexing progress never drop).
- Indexing ✕: 24x24 hit target, `MUTED`, hover `TEXT`, tooltip "Cancel indexing (Esc)". Cancel leaves line numbers estimated permanently for that file, status shows "Indexing cancelled" for 4 s, then segment hides; line numbers continue with `~`.
- Esc cancels the most recent long operation (indexing, copy). Esc priority: Go-to box > dialog > operation cancel.
- `Startup`/backend labels only in `--bench` mode (right side after indexing slot).
- Repaint rule: indexing progress updates via `request_repaint` at most every 100 ms while running (default), zero repaints idle.

### Drag-over drop affordance (D-15)
- While a file hovers the window (`raw_input.hovered_files` non-empty): full viewer rect gets `DROP_FILL` and an inset 8pt, 2pt `ACCENT` border (rounded 4); centered label Body 14 `TEXT`: "Drop to open {filename}" (if multiple: "Drop to open {first filename} (only the first file opens)"). Overlay removed immediately on leave/drop. Menu and status bar untouched.
- Drop of a directory or non-regular file: status message "Can't open {name}: not a regular file" (`DESTRUCTIVE`, 6 s).

### Menus

Modifier = Cmd on macOS, Ctrl elsewhere.

| Menu | Item | Shortcut | State |
|------|------|----------|-------|
| File | Open... | Ctrl+O | opens `rfd` dialog in last directory (session) |
| File | Quit | Ctrl+Q (Cmd+Q) | |
| View | Word wrap | Alt+Z | checkbox; disabled in hex |
| View | Line numbers | (none) | checkbox; disabled in hex |
| View | Hex view | Ctrl+H | checkbox; disabled with no file |
| View | Go to... | Ctrl+G | disabled with no file |
| Help | About Whim | | unchanged |

Also: Ctrl+A select all, Ctrl+C copy (not in menus; no Edit menu until Phase 3). PageUp/PageDown/Home/End/Ctrl+Home/Ctrl+End navigate (End = end of the displayed row for hard-broken lines; Ctrl+End = end of file, fixing WR-02/WR-04). Scroll-thumb minimum 24pt retained; thumb maps byte offset linearly in hex view, estimated row fraction in text view.

---

## Copywriting Contract

| Element | Copy |
|---------|------|
| Primary CTA | "Open..." (File menu) |
| Empty heading | "No file open" |
| Empty body | "Drop a file here, press Ctrl+O, or run `whim <path>`." (Cmd+O on macOS; centered, muted, 14pt) |
| Empty file (0 bytes) | "This file is empty." (centered, muted); hex view shows the same |
| Binary banner | "This looks like a binary file." [View as hex] |
| Error: cannot open | "Couldn't open {filename}: {reason}. Check that the file exists and you have permission to read it." |
| Error: not regular file | "Couldn't open {name}: it is not a regular file (folder, pipe or device)." |
| Error: changed on disk | "File changed on disk" / "{filename} was modified or truncated while open. Reload to view its current contents." |
| Error: bad go-to | "Not a valid position" |
| Wrap notice | "Word wrap off for large files" |
| Indexing | "Indexing {n}%" ; tooltip "Cancel indexing (Esc)"; "Indexing cancelled" |
| Drop overlay | "Drop to open {filename}" |
| Copy confirm | Title "Copy large selection?"; body per dialog above; buttons "Cancel" / "Copy {size}" |
| Copy over cap | "The selection is {size}, which is over the 256 MB copy limit. Select a smaller range." / "OK" |
| Destructive actions | none in Phase 2 (read-only); copy confirm is a size warning, not destructive |
| Disabled tooltips | "Open a file first" (View items with no file); "Not available in hex view" (wrap, line numbers) |

Size formatting: decimal-ish "GB/MB/KB/B" with one decimal (e.g. `20.0 GB`), consistent with Phase 1.

---

## Registry Safety

| Registry | Blocks Used | Safety Gate |
|----------|-------------|-------------|
| shadcn official | none (not applicable) | not required |
| Third-party registries | none | not applicable |

New cargo dependencies (`rfd`, `memchr`) are covered by the footprint gate (`cargo xtask check-size`), not this table.

---

## Checker Sign-Off

- [ ] Dimension 1 Copywriting
- [ ] Dimension 2 Visuals
- [ ] Dimension 3 Color
- [ ] Dimension 4 Typography
- [ ] Dimension 5 Spacing
- [ ] Dimension 6 Registry Safety

**Approval:** pending
