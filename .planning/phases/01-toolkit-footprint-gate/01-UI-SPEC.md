---
phase: 1
slug: toolkit-footprint-gate
status: draft
shadcn_initialized: false
preset: none
created: 2026-10-31
---

# Phase 1 — UI Design Contract

> Visual and interaction contract for the walking skeleton. Native Rust app (egui/eframe 0.37, glow). All values are egui points (logical px, scaled by `pixels_per_point`), not CSS. Intentionally minimal: this phase measures the toolkit, it does not design the product. Later phases extend this file.

Sources: CONTEXT.md (skeleton scope, fonts, IME decisions), STACK.md (egui/glow, custom painter), ROADMAP.md (success criteria 1, 4). Everything else is Claude's discretion, marked "(default)".

---

## Design System

| Property | Value |
|----------|-------|
| Tool | none (shadcn/Tailwind not applicable; native egui) |
| Preset | not applicable |
| Component library | egui built-ins only (`TopBottomPanel`, `CentralPanel`, `menu::bar`, `ScrollArea` NOT used for the main view) |
| Icon library | none in Phase 1 (text-only menus and status) |
| Font (UI) | egui default proportional font (bundled Ubuntu-Light/Hack set from `default_fonts`) |
| Font (editor) | one bundled monospace font, registered as `FontFamily::Monospace` first priority (default: JetBrains Mono Regular, OFL; swap allowed if binary-size budget demands, e.g. Hack already in egui). No CJK fallback (UI-05 deferred). |
| Theme | egui dark `Visuals::dark()` as base, with overrides below. Light/OS theme deferred to Phase 8. |

---

## Spacing Scale

Declared values (multiples of 4, in points). Set via `Style.spacing`.

| Token | Value | Usage |
|-------|-------|-------|
| xs | 4 | `item_spacing.y`, status bar segment inner padding, menu item vertical padding |
| sm | 8 | `item_spacing.x`, status bar segment gap, `button_padding.x` |
| md | 16 | Panel inner margin horizontal, editor left text inset |
| lg | 24 | Menu bar height, status bar height |
| xl | 32 | Not used in Phase 1 |
| 2xl | 48 | Not used in Phase 1 |
| 3xl | 64 | Not used in Phase 1 |

Exceptions: editor row height is derived from the font (monospace size 14 x 1.4 = 20pt rows, fixed for byte-offset virtual scroll; 20 is a multiple of 4). IME preedit underline is 1pt stroke (stroke widths are not spacing).

Layout contract:
- `TopBottomPanel::top` holds the menu bar (24pt high, no tabs strip yet; the tab area is a Phase 5 concern and must not be stubbed visibly).
- `TopBottomPanel::bottom` holds the status bar (24pt high).
- `CentralPanel` holds the viewer, zero inner margin except 16pt left inset for text; the painter fills the entire rect.
- Window default 1024x720 points, min 480x320. Remains correct under `pixels_per_point` of 1.0, 1.5, 2.0 and when dragged between mixed-DPI monitors (re-layout on `ScaleFactorChanged`; font textures rebuilt by egui).

---

## Typography

Exactly 3 sizes and 2 weights (egui has no true weight axis for bundled fonts; Regular is the only face loaded, "Strong" is rendered with egui `RichText::strong()` only if a bold face is bundled, otherwise omitted in Phase 1).

| Role | Size | Weight | Line Height |
|------|------|--------|-------------|
| Body (menu items, UI text) | 14 | Regular (400) | 1.4 (egui derives from font; `TextStyle::Body`) |
| Label (status bar) | 12 | Regular (400) | 1.4 (`TextStyle::Small`) |
| Editor (monospace rows) | 14 | Regular (400) | 1.4 fixed = 20pt row height (`TextStyle::Monospace`) |

Weights: Regular (400) and Semibold (600, reserved for later headings; unused in Phase 1). No Heading/Display role in Phase 1.

Editor text rules (default):
- Paint only visible rows via `Painter::galley` per row; never lay out more than rows in viewport + 1.
- Per-row layout width capped: truncate display at 4096 chars per row for the skeleton (STACK.md recommends cap ~10k; skeleton uses lower for speed).
- Text must be pixel-snapped: row y positions rounded to physical pixels (`round_to_pixels`) to keep text crisp at fractional DPI.
- Non-UTF-8 / non-printable bytes in skeleton view render as `·` (U+00B7) in the muted color; a 20 GB sparse file is mostly NUL bytes and must render without allocation spikes.

---

## Color

Dark theme only in Phase 1. Values are `Color32`.

| Role | Value | Usage |
|------|-------|-------|
| Dominant (60%) | `#1E1F22` | `panel_fill` / `window_fill` / editor background |
| Secondary (30%) | `#2B2D31` | Menu bar fill, status bar fill, menu popup fill, separators `#3A3C41` |
| Accent (10%) | `#4C9AFF` | See reserved list below |
| Destructive | `#F2555A` | Not used in Phase 1 (reserved for error text in status bar) |
| Text primary | `#D8DADF` | Editor text, menu text (contrast > 10:1 on dominant) |
| Text muted | `#8B8F98` | Status bar labels, non-printable placeholder `·`, row offset gutter (contrast >= 4.5:1 on secondary) |

Accent reserved for: (1) text selection background in the editor at 35% alpha (egui `visuals.selection.bg_fill`), (2) IME preedit underline and caret, (3) focus ring on menu items / keyboard-focused widgets (`visuals.selection.stroke`), (4) hovered menu item highlight text. Never used for plain text, borders or backgrounds of panels.

---

## Interaction Contract

### Menu bar (stub)
Menus exist as shells to prove menu rendering and keyboard nav; items are mostly inert.

| Menu | Items | Behavior in Phase 1 |
|------|-------|---------------------|
| File | Open... (Ctrl/Cmd+O), Quit (Ctrl+Q; Cmd+Q on macOS) | Open uses `rfd` only if already dependent; otherwise file path comes from CLI arg (`whim <path>`, via lexopt) and Open is disabled with tooltip "Coming in a later phase". Quit closes the viewport. |
| View | Wrap lines, Line numbers | Disabled (greyed), Phase 2 |
| Help | About Whim | Shows a small window with version and active backend (glow/wgpu) |

Modifier key shown/used: Cmd on macOS, Ctrl elsewhere (`Modifiers::COMMAND`). Alt-key menu mnemonics not required.

### Virtualized view
- Vertical scroll is a byte-offset virtual scroll: scroll position = a byte offset in `[0, file_len)`, rows are fixed height; no line index. Mouse wheel, trackpad, PageUp/PageDown, Home/End, arrow Up/Down scroll; scrollbar drag maps to byte offset linearly (thumb has a minimum 24pt length for a 20 GB file).
- Row model: each row displays a fixed `bytes_per_row` window (default 128 bytes, no wrap; or break at `\n` if found within window). First page reads only visible rows' bytes from the mmap.
- Gutter (default): 16-hex-digit byte offset of row start in muted color, 16pt right padding. This doubles as a visible proof of position in a 20 GB file.
- Read-only; clicking sets nothing. Text selection is NOT required in Phase 1 (Phase 3), except as needed to exercise IME below.
- Repaint policy: repaint only on input events or scroll animation. Idle must produce zero repaints (no `request_repaint` loop, no blinking caret timer unless IME is active). This is what the idle CPU/RSS measurement checks.

### IME preedit
- On `Event::Ime(Enabled)` show composition; on `Preedit(text)` render preedit string at the caret position in the editor (a single fixed probe caret at the top-left text cell of the viewer is acceptable in Phase 1) with a 1pt accent underline; on `Commit(text)` append text to a small in-memory probe line (not file data; file stays read-only) rendered below the preedit, cleared on Escape.
- Set `ctx.output_mut(|o| o.ime = Some(IMEOutput { rect, cursor_rect }))` every frame while the viewer has focus so the OS candidate window appears adjacent to the caret (below the cursor_rect, not covering it).
- Required manual test: Windows Japanese IME (Microsoft IME): compose, convert, commit, cancel with Esc. Record results; mark macOS/Linux "unverified" if not run.

### Status bar
Left to right, separated by 8pt gaps and 1pt `#3A3C41` dividers, 12pt muted text:
1. `Offset 0x00000000000000A0` (byte offset of first visible row, hex)
2. `Size 20.0 GB` (human readable, base-1024 label "GiB" is NOT used; use "GB" for brevity)
3. Right-aligned: `glow` or `wgpu` backend label, and `Startup 142 ms` (self-reported time to first painted frame, shown for the gate harness; also printed to stdout when `--bench` flag is passed).

No encoding, EOL, line:col or indexing progress yet (Phase 2).

### Harness-visible behavior (for the measurement tooling)
- `--bench` flag: window opens, first frame painted, startup time printed as `startup_ms=<n>`, then process stays idle for the harness to sample RSS/CPU, exits on signal or after `--bench-exit-after <secs>`.
- No splash, no animation, no fade-in: first frame must show the final layout (menu, view, status bar).

---

## Copywriting Contract

| Element | Copy |
|---------|------|
| Primary CTA | "Open File..." (File menu; disabled in Phase 1 if no dialog dependency) |
| Empty state heading | "No file open" |
| Empty state body | "Run `whim <path>` or choose File > Open to view a file." (centered, muted, 14pt, in the viewer area when no path given) |
| Error state (cannot open) | "Couldn't open {filename}: {reason}. Check the path and permissions, then try again." (shown in status bar left slot, in destructive color, plus centered in viewer) |
| Error state (map failed) | "Couldn't map {filename}: {reason}. The file may be locked by another program." |
| Destructive confirmation | none: Phase 1 has no destructive actions (read-only skeleton) |
| Disabled item tooltip | "Coming in a later phase" |
| About window | "Whim {version} — toolkit: egui 0.37 ({backend})" |

---

## Registry Safety

| Registry | Blocks Used | Safety Gate |
|----------|-------------|-------------|
| shadcn official | none (not applicable, no web UI) | not required |
| Third-party registries | none | not applicable |

Cargo dependency vetting is covered by the footprint gate (cargo-bloat, `cargo deny` if added), not by this table.

---

## Checker Sign-Off

- [ ] Dimension 1 Copywriting: PASS
- [ ] Dimension 2 Visuals: PASS
- [ ] Dimension 3 Color: PASS
- [ ] Dimension 4 Typography: PASS
- [ ] Dimension 5 Spacing: PASS
- [ ] Dimension 6 Registry Safety: PASS

**Approval:** pending
