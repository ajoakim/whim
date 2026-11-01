# Requirements: Whim

**Defined:** 2026-10-31
**Core Value:** Any file, of any size, opens instantly and can be viewed and edited without the app ever loading the whole file into memory.

## v1 Requirements

Requirements for initial release. Each maps to roadmap phases.

### Large-File Core (CORE)

- [ ] **CORE-01**: User can open a file of tens of GB and see its first page in under 1 second
- [ ] **CORE-02**: App never reads a whole file into memory; RAM use grows with edits, not file size
- [ ] **CORE-03**: User can scroll smoothly through any file, including before line indexing finishes
- [ ] **CORE-04**: User sees background line-indexing progress in the status bar; line numbers become exact when it completes
- [ ] **CORE-05**: User can jump to a line number or a byte offset
- [ ] **CORE-06**: User can view files containing extremely long lines (multi-GB single line) without the app freezing
- [ ] **CORE-07**: All long operations (indexing, search, replace-all, save) show progress and can be cancelled without blocking the UI

### Editing (EDIT)

- [ ] **EDIT-01**: User can insert, delete and replace text anywhere in a file of any size
- [ ] **EDIT-02**: User can select text with mouse and keyboard and copy/cut/paste (with a size cap/warning for huge clipboard operations)
- [ ] **EDIT-03**: User can undo and redo edits; undo history is bounded so memory does not grow without limit
- [ ] **EDIT-04**: User can type using OS input methods (IME) for CJK and other composed scripts
- [ ] **EDIT-05**: User can toggle word wrap (automatically disabled on huge files, with a notice)
- [ ] **EDIT-06**: User can toggle line numbers

### Save & File Safety (SAVE)

- [ ] **SAVE-01**: User can save edits to a huge file safely: the original stays intact until the new file is complete, and free disk space is checked first
- [ ] **SAVE-02**: Saving same-length edits (e.g. hex byte overwrites) is fast and patches the file in place
- [ ] **SAVE-03**: User can Save As to a new path
- [ ] **SAVE-04**: Unedited bytes are preserved exactly on save: line endings, BOM, encoding and invalid byte sequences are never silently changed
- [ ] **SAVE-05**: Saving works on Windows even though the open file is memory-mapped
- [ ] **SAVE-06**: User is warned and offered a reload when an open file changes on disk
- [ ] **SAVE-07**: App does not crash if another program truncates or deletes an open file
- [ ] **SAVE-08**: Read-only files and permission errors are reported clearly instead of failing silently
- [ ] **SAVE-09**: After a crash, user can recover unsaved edits from a recovery journal

### Encoding (ENC)

- [ ] **ENC-01**: App detects file encoding (BOM, UTF-8 validation, statistical detection) and shows it in the status bar
- [ ] **ENC-02**: User can switch the encoding a file is interpreted as, and convert/save in another encoding
- [ ] **ENC-03**: App detects line-ending style (LF/CRLF/CR/mixed), shows it in the status bar, and user can convert it
- [ ] **ENC-04**: UTF-8, UTF-16 LE/BE and single-byte encodings are fully supported on files of any size (view, index, search, edit)
- [ ] **ENC-05**: Legacy multibyte encodings (Shift-JIS, GBK, Big5, EUC-*) are fully supported; random jumps into huge files become exact once the background index has covered that region
- [ ] **ENC-06**: Invalid byte sequences are displayed visibly (not dropped) and round-trip unchanged

### Search (SRCH)

- [ ] **SRCH-01**: User can find text (plain or regex, case/whole-word options) in files of any size, with progress and cancel
- [ ] **SRCH-02**: User can find next/previous match from the cursor
- [ ] **SRCH-03**: User sees all matches streaming into a clickable results list while the search runs
- [ ] **SRCH-04**: User can replace the current match and replace all (replace-all streams on huge files and is one undo step)
- [ ] **SRCH-05**: Search works correctly in the file's encoding (including UTF-16 and legacy encodings)

### Hex (HEX)

- [ ] **HEX-01**: User can view any file (including huge ones) in a hex view with offset, hex and ASCII columns
- [ ] **HEX-02**: User can overwrite individual bytes in the hex view and save
- [ ] **HEX-03**: User can jump to an offset in hex view
- [ ] **HEX-04**: Switching between text and hex view keeps the same byte position, and both views share one undo history
- [ ] **HEX-05**: Binary files open as text, with a banner offering to switch to hex (never auto-switches)

### Markdown & Highlighting (VIEW)

- [ ] **VIEW-01**: User can toggle a `.md` tab between source and rendered Markdown
- [ ] **VIEW-02**: Rendered Markdown is safe for untrusted files: no raw HTML execution, remote images off by default, only safe link schemes
- [ ] **VIEW-03**: Syntax highlighting for common languages, computed for the visible area only
- [ ] **VIEW-04**: Highlighting is automatically disabled on very large files or very long lines, with a status-bar notice

### Tabs, Session & Opening Files (OPEN)

- [ ] **OPEN-01**: User can have multiple files open in tabs, with dirty indicators and close-without-saving prompts
- [ ] **OPEN-02**: User can open files by drag & drop onto the window
- [ ] **OPEN-03**: User can open a file from the terminal with `whim <file>`
- [ ] **OPEN-04**: User can register Whim for "Open with" / file associations on Windows, Linux and macOS
- [ ] **OPEN-05**: Opening another file while Whim is running adds a tab to the existing window (single instance)
- [ ] **OPEN-06**: User can reopen recent files from a recent-files list
- [ ] **OPEN-07**: Last session's tabs (and scroll positions) are restored on launch
- [ ] **OPEN-08**: User can follow a growing file (tail mode) so new lines appear and auto-scroll

### UI & Preferences (UI)

- [ ] **UI-01**: Minimal UI: tabs, editor area, status bar (line:col, byte offset, encoding, EOL, indexing progress), menus for everything else
- [ ] **UI-02**: User can choose light or dark theme, or follow the OS
- [ ] **UI-03**: User can choose editor font and size, and zoom
- [ ] **UI-04**: Text is crisp on HiDPI and mixed-DPI displays
- [ ] **UI-05**: CJK text renders via font fallback (system or bundled), so IME-typed and opened CJK text is readable

### Performance & Distribution (PERF)

- [ ] **PERF-01**: App window appears in about 200 ms on a cold start
- [ ] **PERF-02**: Idle RAM is in the tens of MB and idle CPU is near zero
- [ ] **PERF-03**: Windows and Linux ship as a single portable binary under ~15 MB, with no installer or runtime
- [ ] **PERF-04**: macOS ships as an unsigned `.app` bundle (users open it via right-click → Open the first time)
- [ ] **PERF-05**: Runs on Windows, Linux and macOS

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Accessibility & Fonts

- **A11Y-01**: Screen readers can read editor text
- **A11Y-02**: Emoji and full all-script font fallback (basic CJK fallback is in v1 as UI-05)

### Hex

- **HEX-06**: Insert/delete bytes in hex view
- **HEX-07**: Hex pattern search
- **HEX-08**: Data inspector (int/float/etc. at cursor)

### Distribution

- **DIST-01**: Signed + notarized macOS build; signed Windows binary (removes SmartScreen/Gatekeeper warnings)

### Editing

- **EDIT-07**: Column selection / multi-cursor
- **EDIT-08**: Line filtering (show only lines matching a pattern)
- **EDIT-09**: CSV/table view and diff

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Auto-switching binary files to hex | User prefers text-first with a manual switch |
| Live split-view Markdown preview | Toggle view chosen; simpler and lighter |
| Embedded web view for Markdown | Breaks footprint targets and is a security risk with untrusted files |
| Plugins, LSP, file tree, command palette | Not an IDE; UI must stay minimal and lightweight |
| Minimap / full-file highlighting on huge files | Requires touching the whole file |
| Auto-update and telemetry | Lightweight, offline tool |
| Mobile / web versions | Desktop only |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| PERF-01 | Phase 1 | Pending |
| PERF-02 | Phase 1 | Pending |
| PERF-05 | Phase 1 | Pending |
| UI-04 | Phase 1 | Pending |
| CORE-01 | Phase 2 | Pending |
| CORE-02 | Phase 2 | Pending |
| CORE-03 | Phase 2 | Pending |
| CORE-04 | Phase 2 | Pending |
| CORE-05 | Phase 2 | Pending |
| CORE-06 | Phase 2 | Pending |
| CORE-07 | Phase 2 | Pending |
| EDIT-05 | Phase 2 | Pending |
| EDIT-06 | Phase 2 | Pending |
| HEX-01 | Phase 2 | Pending |
| HEX-03 | Phase 2 | Pending |
| HEX-05 | Phase 2 | Pending |
| UI-01 | Phase 2 | Pending |
| UI-05 | Phase 2 | Pending |
| EDIT-01 | Phase 3 | Pending |
| EDIT-02 | Phase 3 | Pending |
| EDIT-03 | Phase 3 | Pending |
| EDIT-04 | Phase 3 | Pending |
| HEX-02 | Phase 3 | Pending |
| HEX-04 | Phase 3 | Pending |
| SAVE-01 | Phase 4 | Pending |
| SAVE-02 | Phase 4 | Pending |
| SAVE-03 | Phase 4 | Pending |
| SAVE-04 | Phase 4 | Pending |
| SAVE-05 | Phase 4 | Pending |
| SAVE-08 | Phase 4 | Pending |
| SAVE-09 | Phase 4 | Pending |
| ENC-01 | Phase 5 | Pending |
| ENC-02 | Phase 5 | Pending |
| ENC-03 | Phase 5 | Pending |
| ENC-04 | Phase 5 | Pending |
| ENC-05 | Phase 5 | Pending |
| ENC-06 | Phase 5 | Pending |
| OPEN-01 | Phase 5 | Pending |
| SAVE-06 | Phase 5 | Pending |
| SAVE-07 | Phase 5 | Pending |
| SRCH-01 | Phase 6 | Pending |
| SRCH-02 | Phase 6 | Pending |
| SRCH-03 | Phase 6 | Pending |
| SRCH-04 | Phase 6 | Pending |
| SRCH-05 | Phase 6 | Pending |
| OPEN-08 | Phase 6 | Pending |
| VIEW-01 | Phase 7 | Pending |
| VIEW-02 | Phase 7 | Pending |
| VIEW-03 | Phase 7 | Pending |
| VIEW-04 | Phase 7 | Pending |
| OPEN-02 | Phase 8 | Pending |
| OPEN-03 | Phase 8 | Pending |
| OPEN-04 | Phase 8 | Pending |
| OPEN-05 | Phase 8 | Pending |
| OPEN-06 | Phase 8 | Pending |
| OPEN-07 | Phase 8 | Pending |
| UI-02 | Phase 8 | Pending |
| UI-03 | Phase 8 | Pending |
| PERF-03 | Phase 8 | Pending |
| PERF-04 | Phase 8 | Pending |

**Coverage:**
- v1 requirements: 60 total
- Mapped to phases: 60
- Unmapped: 0 ✓

---
*Requirements defined: 2026-10-31*
*Last updated: 2026-10-31 after roadmap creation*
