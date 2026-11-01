# Roadmap: Whim

## Overview

Whim is built core-first. Phase 1 is a gate: it proves the toolkit and footprint claims (startup, RAM, size, HiDPI, IME) before anything else depends on them. Phases 2-4 then prove the Core Value: instant, read-only text and hex viewing of tens-of-GB files, then editing, then a trustworthy save pipeline. Phase 5 adds encodings, tabs and file-change safety. Phase 6 adds streaming search and tail mode. Phase 7 adds the leaf features (highlighting, Markdown). Phase 8 finishes OS integration, session, preferences and release packaging. Footprint budgets (PERF-01/02/03) are a standing gate re-checked in every phase, not just where they are first assigned.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

- [ ] **Phase 1: Toolkit & Footprint Gate** - Walking skeleton on all three OSes that measures startup, RAM, HiDPI and IME and decides egui vs. fallback
- [ ] **Phase 2: Core Storage & Read-Only Viewers** - Open any-size file instantly in virtualized text and hex views with background line index
- [ ] **Phase 3: Editing, Undo & Hex Overwrite** - Insert/delete/replace anywhere, selection, clipboard, IME, bounded undo, byte overwrite
- [ ] **Phase 4: Safe Save & Recovery** - Atomic, cancellable save of edited huge files, in-place patching, Save As, crash recovery journal
- [ ] **Phase 5: Encoding, Tabs & File-Change Safety** - Encoding/EOL detection and switching, multi-file tabs, external-change and truncation safety
- [ ] **Phase 6: Search, Replace & Tail** - Streaming find/replace with regex on huge files, results list, follow-a-growing-file mode
- [ ] **Phase 7: Syntax Highlighting & Markdown** - Viewport-only highlighting and a safe source/rendered Markdown toggle
- [ ] **Phase 8: OS Integration, Preferences & Release** - CLI, drag and drop, single instance, file associations, session restore, themes, shippable binaries

## Phase Details

### Phase 1: Toolkit & Footprint Gate
**Goal**: The GUI toolkit and footprint targets are proven on Windows, Linux and macOS with measured numbers, so the editor widget is not built on an unverified foundation
**Depends on**: Nothing (first phase)
**Requirements**: PERF-01, PERF-02, PERF-05, UI-04
**Success Criteria** (what must be TRUE):
  1. User can launch the skeleton app on Windows, Linux and macOS and see a window with a virtualized monospace view in about 200 ms from a cold start
  2. User can open a 20 GB sparse file in the skeleton and see its first page without the app reading the file into memory
  3. Idle RAM is in the tens of MB and idle CPU is near zero, with the numbers recorded for glow vs. wgpu
  4. Text is crisp on HiDPI and mixed-DPI displays, and IME composition is checked and its supported scope documented
  5. A written go/no-go on egui (with the fallback toolkit named) exists, backed by the measured numbers, and CI enforces startup, size and idle budgets
**Plans**: 5 plans

Plans:
**Wave 1**
- [x] 01-01-PLAN.md — Workspace scaffold (toolchain 1.99.0, glow/wgpu features, xtask stub) + tested byte-offset row model and pixel snapping

**Wave 2** *(blocked on Wave 1 completion)*
- [ ] 01-02-PLAN.md — eframe 0.37 walking skeleton: args, startup clock, read-only mmap, app shell, virtualized view, bench stdout contract
- [ ] 01-03-PLAN.md — xtask harness (sparse 20 GiB, check-size, measure) + 3-OS x 2-backend CI workflow + docs/measurements.md

**Wave 3** *(blocked on Wave 2 completion)*
- [ ] 01-04-PLAN.md — IME probe wiring (preedit/commit/IMEOutput) + glow/wgpu build and harness smoke

**Wave 4** *(blocked on Wave 3 completion)*
- [ ] 01-05-PLAN.md — Local measurements, manual IME/HiDPI/CI checkpoint, final go/no-go in 01-GATE.md
**UI hint**: yes

### Phase 2: Core Storage & Read-Only Viewers
**Goal**: User can open a file of any size and immediately scroll, jump and inspect it in text or hex, with the whole file never loaded into memory
**Depends on**: Phase 1
**Requirements**: CORE-01, CORE-02, CORE-03, CORE-04, CORE-05, CORE-06, CORE-07, EDIT-05, EDIT-06, HEX-01, HEX-03, HEX-05, UI-01, UI-05
**Success Criteria** (what must be TRUE):
  1. User opens a file of tens of GB and sees its first page in under 1 second, with RAM use independent of file size
  2. User scrolls smoothly through the file immediately, before line indexing finishes, sees indexing progress in the status bar, and line numbers become exact on completion
  3. User can jump to a line number or a byte offset and land at the right place, in both text and hex views
  4. User can open a file with a multi-GB single line (or no newlines) without a freeze, and can cancel any long background operation without the UI blocking
  5. User can view any file in a hex view (offset, hex and ASCII columns), and opening a binary file shows text with a banner offering hex, never auto-switching
  6. User can toggle line numbers and word wrap (wrap auto-disabled on huge files with a notice), and CJK text displays via font fallback; UI shows only tabs area, editor and status bar (line:col, offset, encoding, EOL, indexing progress)
**Plans**: TBD
**UI hint**: yes

### Phase 3: Editing, Undo & Hex Overwrite
**Goal**: User can edit a file of any size in text or hex, with working selection, clipboard, IME and bounded undo, with memory growing only with edits
**Depends on**: Phase 2
**Requirements**: EDIT-01, EDIT-02, EDIT-03, EDIT-04, HEX-02, HEX-04
**Success Criteria** (what must be TRUE):
  1. User can insert, delete and replace text anywhere in a tens-of-GB file with no perceptible lag and no whole-file load
  2. User can select with mouse and keyboard and copy/cut/paste, and gets a size cap or warning on huge clipboard operations
  3. User can undo and redo edits, and undo history stays bounded so memory does not grow without limit
  4. User can type CJK and other composed text through the OS input method
  5. User can overwrite individual bytes in hex view, switch between text and hex at the same byte position, and undo/redo across both views from one shared history
**Plans**: TBD
**UI hint**: yes

### Phase 4: Safe Save & Recovery
**Goal**: User can save edits to huge files without ever risking the original, and can recover unsaved work after a crash
**Depends on**: Phase 3
**Requirements**: SAVE-01, SAVE-02, SAVE-03, SAVE-04, SAVE-05, SAVE-08, SAVE-09
**Success Criteria** (what must be TRUE):
  1. User saves an edited huge file and the original stays intact until the new file is complete; free disk space is checked first, and the save shows progress and can be cancelled
  2. User saves same-length hex byte overwrites quickly, patching the file in place
  3. User can Save As to a new path, and saving works on Windows even though the file is memory-mapped
  4. After saving, unedited bytes are identical to the original, including line endings, BOM and invalid byte sequences
  5. User sees a clear message for read-only files and permission errors instead of a silent failure
  6. After killing the app mid-edit, user is offered recovery of unsaved edits from the journal on next launch
**Plans**: TBD

### Phase 5: Encoding, Tabs & File-Change Safety
**Goal**: User can work with many files in tabs in any common encoding and line-ending style, and the app stays safe when files change on disk
**Depends on**: Phase 4
**Requirements**: ENC-01, ENC-02, ENC-03, ENC-04, ENC-05, ENC-06, OPEN-01, SAVE-06, SAVE-07
**Success Criteria** (what must be TRUE):
  1. User sees detected encoding and line-ending style in the status bar, and can switch either, or convert and save in another encoding or EOL style
  2. User can view, index, jump and edit UTF-8, UTF-16 LE/BE and single-byte files of any size, and Shift-JIS, GBK, Big5 and EUC files, with jumps becoming exact once the background index covers the region
  3. Invalid byte sequences show visibly and round-trip unchanged on save
  4. User can have multiple files open in tabs with dirty indicators and a close-without-saving prompt
  5. User is warned and offered a reload when an open file changes on disk, and the app does not crash when another program truncates or deletes it
**Plans**: TBD
**UI hint**: yes

### Phase 6: Search, Replace & Tail
**Goal**: User can find and replace across files of any size with live results, and follow a growing file
**Depends on**: Phase 5
**Requirements**: SRCH-01, SRCH-02, SRCH-03, SRCH-04, SRCH-05, OPEN-08
**Success Criteria** (what must be TRUE):
  1. User can search plain text or regex, with case and whole-word options, in a tens-of-GB file with progress and cancel
  2. User sees matches stream into a clickable results list while the search runs, and clicking a result jumps to it
  3. User can find next/previous from the cursor, and matches are correct in the file's encoding including UTF-16 and legacy encodings and across chunk boundaries
  4. User can replace the current match or replace all; replace-all streams on huge files and is one undo step
  5. User can turn on tail mode and see new lines appear and auto-scroll as the file grows
**Plans**: TBD
**UI hint**: yes

### Phase 7: Syntax Highlighting & Markdown
**Goal**: User gets colored code and a safe rendered Markdown view without any cost on huge files
**Depends on**: Phase 2
**Requirements**: VIEW-01, VIEW-02, VIEW-03, VIEW-04
**Success Criteria** (what must be TRUE):
  1. User sees syntax highlighting for common languages, computed only for the visible area
  2. On very large files or very long lines, highlighting turns off automatically and the status bar says so
  3. User can toggle a `.md` tab between source and rendered Markdown
  4. A malicious Markdown file cannot execute raw HTML or load remote images by default, and only safe link schemes are clickable
**Plans**: TBD
**UI hint**: yes

### Phase 8: OS Integration, Preferences & Release
**Goal**: Whim behaves as a default file-opening app on all three OSes, remembers the user's session and preferences, and ships as small portable builds
**Depends on**: Phase 5
**Requirements**: OPEN-02, OPEN-03, OPEN-04, OPEN-05, OPEN-06, OPEN-07, UI-02, UI-03, PERF-03, PERF-04
**Success Criteria** (what must be TRUE):
  1. User can open files by dragging them onto the window or running `whim <file>`, and a second launch adds a tab to the running window
  2. User can register Whim for "Open with" / file associations on Windows, Linux and macOS, and double-clicking an associated file opens it
  3. User can reopen files from a recent-files list, and the last session's tabs and scroll positions are restored on launch
  4. User can choose light, dark or OS theme, pick editor font and size, and zoom
  5. Windows and Linux ship as a single portable binary under about 15 MB with no installer, and macOS ships as an unsigned `.app` bundle, with a final footprint audit passing
**Plans**: TBD
**UI hint**: yes

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 (Phase 7 depends only on Phase 2 and may slip without blocking others)

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Toolkit & Footprint Gate | 1/5 | In Progress|  |
| 2. Core Storage & Read-Only Viewers | 0/TBD | Not started | - |
| 3. Editing, Undo & Hex Overwrite | 0/TBD | Not started | - |
| 4. Safe Save & Recovery | 0/TBD | Not started | - |
| 5. Encoding, Tabs & File-Change Safety | 0/TBD | Not started | - |
| 6. Search, Replace & Tail | 0/TBD | Not started | - |
| 7. Syntax Highlighting & Markdown | 0/TBD | Not started | - |
| 8. OS Integration, Preferences & Release | 0/TBD | Not started | - |
