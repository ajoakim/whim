# Feature Landscape

**Domain:** Lightweight desktop text editor + Markdown viewer + hex viewer for huge files (tens of GB)
**Researched:** 2026-10-31
**Confidence:** MEDIUM. Based on knowledge of Notepad++, Sublime, EmEditor, UltraEdit, klogg/glogg, HxD, ImHex, 010 Editor, Kate, Zed, Lapce, Typora and MarkText. No live verification this session, so vendor-specific claims (e.g. EmEditor's exact limits) should be rechecked before they become hard requirements.

## How large-file editors actually behave (key findings)

| Tool | Approach | Lessons |
|------|----------|---------|
| **klogg/glogg** | Read-only. mmap or chunked reads. Builds a line-offset index in the background on open, with a progress % in the status bar. Search runs in the background, streams matches into a separate results pane, and has cancel. Tail/follow mode. Regex via Hyperscan. | The best model for "open instantly, index in the background". The first page shows before indexing finishes. Search results are a filtered view, not an in-editor highlight. |
| **EmEditor** | Handles up to about 16 TB. Large-file mode is a "load as you go" design with a temp/virtual buffer. Shows a progress bar and allows cancel. Line numbers can be approximate until indexing completes. Offers a "Large File Controller" and splits big files. Read-only is the default for the very largest. Saves by writing a new file, with temp space. Has CSV mode, column select and a multi-file grep. | Edits are held as diffs over the original. Save rewrites the whole file, which takes minutes for 20 GB, so progress and cancel are needed. |
| **UltraEdit** | Opens huge files without loading them, using a temp-file strategy. Disables features such as syntax highlighting, wrap and undo for huge files. Warns on save and needs disk space for the temp copy. | Feature degradation is explicit. Users accept it if it is communicated. |
| **HxD** | Opens by direct disk access or mapping, so open is instant at any size. Edits are overwrite-only by default. Insert and delete are supported via a change list and applied on save. Includes a data inspector, checksums, a disk editor and a compare tool. | Overwrite in place is cheap (patching bytes). Insert/delete needs the rewrite. This matches the project's v1 hex scope. |
| **ImHex / 010 Editor** | Pattern language, data inspector, bookmarks, structure templates, diff. 010 uses a binary-template system. | This is the "full hex editor" territory that the project has put out of scope. |
| **Notepad++ / Sublime / Kate** | Full load into RAM. Notepad++ warns above about 2 GB. Sublime and Kate degrade past hundreds of MB. | These set the table stakes for everyday editing. |
| **Zed / Lapce** | Rope-based buffers, GPU rendering, fast startup. They are code editors with LSP and a tree. | Their rope/piece-table and virtualization techniques apply. Their IDE features are an anti-feature here. |
| **Typora / MarkText** | WYSIWYG or live-preview Markdown. | The project is choosing a source/rendered toggle, which is simpler. |

Key cross-cutting behaviors to copy:
1. **Display first, index later.** The first screen appears immediately. The line index builds in the background. Until it finishes, line numbers show as "~" or the status bar shows "indexing 43%".
2. **Everything long-running is async, shows progress and can be cancelled**: indexing, search, replace-all, save.
3. **Save of an edited huge file** is the hard part. Strategies, from safest to cheapest:
   - (a) Write a new temp file next to the original (piece-table walk), fsync, then atomic rename. This needs free disk space equal to the file size. It is the safest and the default.
   - (b) In-place patch when the edit length is unchanged (hex overwrite, same-length replace). It is fast, with no extra space.
   - (c) In-place shift for small edits near the end. This is risky, so skip it.
   Show a progress bar and cancel, check free space before starting, and keep the original intact until the rename.
4. **Graceful degradation** on huge files: turn off wrap, minimap and full-file syntax highlighting. Highlight only the visible window, or off by default beyond a size threshold. Tell the user in the status bar.
5. **Long lines.** A single 1 GB line (minified JSON) kills many editors. Virtualize horizontally, or cap the layout width per line.

## Table Stakes

Missing any of these and users go back to Notepad++.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Instant open of any size (mmap, no full load) | The core value | High | Depends on the buffer architecture. |
| Tabs, with close/reorder and a dirty indicator | Universal | Low | Reorder is optional at first. |
| Undo/redo, selection, clipboard, save / save as | Universal | Med | Undo on huge files must be bounded (an edit log, not snapshots). |
| Line numbers, with a correct jump-to-line (Ctrl+G) | Universal | Med | Needs the line index. The count is approximate until indexing finishes. |
| Smooth scroll and a draggable scrollbar on a 20 GB file | Core value | High | The scrollbar maps to a byte offset, not a line number, until the index is done. |
| Find / replace with regex, case, whole-word, wrap-around and find-next | Universal | Med-High | On huge files it must be streaming, chunked, async and cancellable, with a match count and progress. |
| Replace all on a huge file | The user wants to edit | High | Streams into a new piece table or output file. Needs progress. |
| Encoding detection and switching (UTF-8, UTF-8 BOM, UTF-16 LE/BE, Latin-1/Windows-1252, a few CJK) | Notepad++ users expect this | Med | Sample the head for detection. Reinterpret without reload. Shown in the status bar. |
| Line-ending detection (CRLF / LF / CR), shown and switchable | Universal | Low-Med | Converting on a huge file is a rewrite, so handle it at save. |
| Status bar: line:col, byte offset, encoding, EOL, size, indexing progress | Convention | Low | |
| Syntax highlighting for common languages | Requirement | Med | Viewport-only, with a size threshold to disable it. Use tree-sitter or syntect. |
| Progress UI and cancel for long operations | Every large-file tool has it | Med | Needs a task/worker architecture. |
| Hex view: offset, hex and ASCII columns, jump to offset, byte overwrite, save | Requirement | Med | Fixed row width, so virtualization is trivial. |
| Drag and drop, CLI argument, Open With, recent files, session restore | Requirement | Low-Med | Per-OS file association registration is fiddly. |
| Word wrap toggle | Universal | Med | Hard with huge files. Off by default, and disabled above a threshold. |
| Large-file-safe behaviors: warn on low disk space, handle files changing or truncating externally, handle locked files | Reliability | Med | Truncation under an mmap means SIGBUS or an access violation, so it must be handled. |
| Reload prompt when the file changes on disk | Universal | Low-Med | |
| Open read-only / handle permission errors | Downloaded files, system files | Low | |
| Unicode-correct cursor movement (grapheme clusters), invalid-byte tolerance | Correctness | Med | Invalid UTF-8 in a log file must still open. Show the replacement character and preserve the bytes on save. |
| Lossless round-trip: unedited bytes stay byte-identical on save | Trust | Med | No silent normalization of EOLs, BOM or encoding. |
| Markdown source/rendered toggle (headings, lists, code, tables, links, images) | Requirement | Med | |
| Light/dark theme (follow the OS) | Expected today | Low | |
| Basic preferences: font, size, tab width, theme | Expected | Low | Store them in a plain config file. |

## Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Editing tens-of-GB files (piece table over mmap) | klogg and HxD cannot edit text. Notepad++ cannot open the file at all. EmEditor can, but it is heavy and paid. | High | The headline feature. |
| Under-1-second open and a window in about 200 ms, with a tiny binary | Notepad++-like speed at any size | High | A constraint that is also a marketing point. |
| Background line index with live progress, usable before it finishes | klogg-style, but in an editor | High | |
| Streaming search results list (klogg-style filtered view), click to jump | Makes search on 20 GB files usable | Med-High | Extends find. Can follow table-stakes find. |
| Atomic, safe save with progress, cancel and disk-space check | Trust on large edits | Med-High | |
| Tail / follow mode for growing logs | The log-file use case, klogg's flagship | Med | Cheap with an mmap re-stat and re-index of the tail. |
| Text/hex toggle on the same buffer, with the cursor offset kept | Flows well with the text-first decision | Low-Med | Edits in either view share the buffer. |
| Seamless hex of a huge file with overwrite (patched in place) | HxD-class capability in a tiny app | Med | |
| Open-as-hex hint for binary files (non-intrusive banner, no auto-switch) | Fits the "manual switch" preference | Low | Banner only. |
| Go to byte offset in text view (0x... and decimal) | Useful for logs and dumps | Low | |
| Hex view extras: selection to clipboard as hex, copy as C array, bookmark offsets | Cheap high-value additions | Low-Med | Candidates for v1.x. |
| Hex search (hex pattern, ASCII/UTF-16 string) | Out of scope for v1 per PROJECT.md, but the search engine is the same | Med | v2. |
| Data inspector panel (int/float/time at the cursor) | HxD, ImHex and 010 all have it | Low-Med | v2. A sidebar, so it conflicts with the minimal UI. |
| Line filtering / "show only matching lines" | klogg's key feature | Med-High | v2. |
| Column/rectangular selection and multi-cursor | Sublime/Notepad++ power users | Med-High | v2. Hard on a piece table. |
| CSV/TSV column view | EmEditor's strength | High | Defer. |
| Compare/diff of two huge files | HxD, 010 | High | Defer. |
| Markdown extras: task lists, footnotes, syntax-highlighted code blocks, local relative images, scroll-position sync between the toggle states | Typora-like polish | Med | Use pulldown-cmark or comrak (GFM). |
| Portable mode (config beside the exe) | Fits the single-binary goal | Low | |

## Anti-Features

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| Loading the whole file for any operation (word wrap on all lines, a whole-file highlighter, a minimap, the full-file line count at open) | Breaks the core value | Viewport-only work plus an async index. |
| Auto-switching binary files to hex | The user said no | A banner offering the switch. |
| Live split-view Markdown preview | Out of scope | Toggle. |
| Plugin system, LSP, autocomplete, git, terminal, file tree, command palette | A code IDE would be heavy and is out of scope | Keep it minimal. |
| Notepad++-style toolbars and sidebars | UI must stay minimal | Menus and shortcuts. |
| Silent EOL, BOM or encoding normalization on save | Corrupts large data files | Preserve bytes unless the user converts. |
| Unbounded undo history that holds file content | Memory blowup on huge edits | An edit log capped by memory, with a warning. |
| In-place insert/delete shifting inside the original file | A crash corrupts a 20 GB file | Temp file plus atomic rename. |
| Full hex editor features in v1 (insert/delete bytes, templates, structure parsing, disk/RAM editing) | Scope | v2 candidates. |
| Auto-update, telemetry, cloud sync, accounts | Runs against lightweight and offline use | None. |
| Rich WYSIWYG Markdown editing | Heavy, and a different product | Rendered view is read-only. |
| An embedded web view for Markdown | Dependency, size and RAM | A native renderer on the toolkit. |
| Autosave of huge files, backup copies of the whole file | Disk and time cost | Optional small-file backup or session recovery only. |

## Feature Dependencies

```
mmap file source
  -> Piece table / rope buffer
       -> Text view (virtualized)  -> Undo/redo, selection, clipboard
       -> Hex view (virtualized)   -> byte overwrite
       -> Save pipeline (temp + atomic rename; in-place patch for same-length edits)
  -> Background line index
       -> Line numbers, Goto line, scrollbar mapping, status-bar position
  -> Async task system (progress + cancel)
       -> Indexing, Search, Replace-all, Save
Encoding detection -> text decoding in the view -> search (must search decoded or encoded bytes consistently)
Find (streaming) -> Replace all -> Results list (v1.x) -> Line filter (v2)
Find engine -> Hex pattern search (v2)
Syntax highlighting (viewport-only) -> size-threshold degradation policy
Markdown render -> tab-level view toggle (depends on tabs) -> scroll sync (later)
Tabs -> Session restore, Recent files, CLI/drag-drop/Open With
File watching -> Reload prompt, Tail mode (later), truncation safety
```

Notes:
- The buffer, line-index and async-task layers must exist before any editor feature. This forces the phase order.
- Encoding affects search, indexing (UTF-16 line breaks are 2-byte) and the hex/text offset mapping. Decide at the architecture level.
- Edits make the line index incremental. Keep a delta layer on top of the base index.

## MVP Recommendation

Prioritize:
1. mmap source, the piece-table buffer and a virtualized text view with a background line index (the foundation, and the highest risk).
2. Tabs, standard editing, status bar, and encoding/EOL detection.
3. Streaming find/replace with progress and cancel, and the safe save pipeline.
4. Hex view with byte overwrite.
5. Syntax highlighting (viewport-only), the Markdown toggle, and the open/session features (CLI, drag-drop, recent files, associations).

One differentiator to include in v1: tail/follow mode and the streaming results list are the cheapest, so add one in v1.x. The essential v1 differentiator is editing huge files with a safe save.

Defer: the data inspector, hex search, line filter, column select, multi-cursor, CSV mode, diff, and the Markdown extras. Each is either out of scope or expensive on a piece table.

## Sources

- Product knowledge of klogg, glogg, EmEditor, UltraEdit, HxD, ImHex, 010 Editor, Notepad++, Sublime Text, Kate, Zed, Lapce, Typora and MarkText. MEDIUM confidence. Not re-verified online this session.
- PROJECT.md (the project's constraints and out-of-scope list).
- Verify before committing to requirements: EmEditor's large-file limits and temp-file behavior, and UltraEdit's feature degradation on huge files.
