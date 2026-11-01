# FastNotes

## What This Is

A small, lightweight, cross-platform desktop file editor in the spirit of Notepad++, with a built-in Markdown renderer and hex view/byte editing. It is meant to be the user's go-to app for quickly opening and viewing (and editing) files — especially downloaded files and very large files (tens of GB) that conventional editors choke on.

## Core Value

Any file, of any size, opens instantly and can be viewed and edited without the app ever loading the whole file into memory.

## Requirements

### Validated

(None yet — ship to validate)

### Active

**Large-file core**
- [ ] Open files of tens of GB+ with the first page visible in < 1 s (memory-mapped / chunked, never full load)
- [ ] Edit huge files (insert/delete/replace text) and save them, without loading the whole file into RAM (piece table / rope over mmap)
- [ ] Smooth scrolling and jump-to-line/offset in huge files

**Editor**
- [ ] Tabs — multiple files open at once
- [ ] Syntax highlighting for common languages
- [ ] Find / replace, including regex, that works on huge files
- [ ] Encoding detection (UTF-8, UTF-16, Latin-1, …) and line-ending detection, both shown in the status bar and switchable
- [ ] Standard editing: undo/redo, selection, copy/paste, save / save as

**Markdown**
- [ ] `.md` tabs can toggle between source and rendered view

**Hex**
- [ ] Hex view: hex + ASCII side by side with offsets, for any file (including huge ones)
- [ ] Overwrite individual bytes, go to offset, save
- [ ] Binary files open as text by default; user switches to hex manually

**Opening files**
- [ ] Drag & drop files onto the window
- [ ] "Open with" / registerable as default app for file types
- [ ] CLI: `fastnotes <file>` opens the file
- [ ] Recent files list and restore last session's tabs

**Performance / footprint**
- [ ] Instant startup (window visible in ~200 ms, cold)
- [ ] Low idle RAM (tens of MB, not hundreds)
- [ ] Single small portable binary (target < ~15 MB), no installer or runtime required

### Out of Scope

- Full hex editor (insert/delete bytes, hex pattern search, data inspector) — v1 hex is view + byte overwrite only; may revisit later
- Auto-detect binary and open in hex — user explicitly prefers text-first, manual switch
- Live split-view Markdown preview — toggle view chosen instead
- Notepad++-style toolbars/sidebars, VS Code-style file tree / command palette — UI must stay minimal
- Plugin system, LSP/IDE features, git integration — not a code IDE; keeps it lightweight
- Mobile / web versions — desktop only

## Context

- Primary use case: the user's default app for files they download and for large files (logs, dumps, data files) that Notepad++ and similar editors struggle with (Notepad++ degrades past a few hundred MB).
- Equal mix of viewing and editing — editing is not an afterthought, including on huge files.
- Greenfield project, empty repository.
- Language choice: user asked for Rust or Go "depending on how fast and lightweight" — Rust chosen (see Key Decisions). GUI toolkit still to be chosen by research (candidates: egui/eframe, iced, Slint, native wrappers); it must hit the startup, RAM and binary-size targets and support custom-rendered virtualized text and hex views.

## Constraints

- **Tech stack**: Rust — no GC, mmap-friendly, small static binaries; Go GUI options are heavier or webview-based
- **Platforms**: Windows, Linux, macOS — all three are first-class
- **Performance**: < ~200 ms cold startup; < 1 s to first page of a 20 GB file; idle RAM in tens of MB
- **Footprint**: single portable binary, target < ~15 MB, no runtime/installer dependency
- **Memory model**: never read a whole file into memory; all views (text, hex, search) must work over mmap/chunks
- **UI**: minimal — tabs + editor + status bar, menus for everything else

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust over Go | Fastest/lightest option: no GC, mature mmap, small binaries, better native GUI options than Go | — Pending |
| Support editing (not just viewing) huge files | User wants to edit lines in tens-of-GB files; requires piece table / rope over mmap | — Pending |
| Markdown: source/rendered toggle (not split) | User preference; simpler and lighter | — Pending |
| Hex v1 = view + byte overwrite | Covers the user's need; full hex editor deferred | — Pending |
| Binary files open as text first | User preference; manual switch to hex | — Pending |
| Minimal UI | User preference: tabs + editor + status bar | — Pending |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-10-31 after initialization*
