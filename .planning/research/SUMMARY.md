# Project Research Summary

**Project:** Whim
**Domain:** Lightweight cross-platform desktop text and hex editor with a Markdown view, built for tens-of-GB files (Rust)
**Researched:** 2026-10-31
**Confidence:** MEDIUM. Crate versions were checked on crates.io. Nearly all claims about behavior, binary size, startup and OS integration come from training knowledge and are unverified.

## Executive Summary

Whim fills a gap. klogg and HxD open huge files instantly but are read-only or byte-only. Notepad++ and Sublime edit fully but load the whole file. The proven design is a byte-oriented, GUI-agnostic core:
- a read-only `ByteSource` (mmap by default)
- a balanced-tree piece table with an append-only add buffer
- a sparse line index built in the background
- decoding only for the visible viewport

Text and hex are two renderers over one shared document. Workers get immutable `Arc` snapshots and send back version-tagged results over channels. All long jobs (index, search, save, replace-all) need progress and cancel.

**Recommended stack:**
- egui/eframe 0.37 on the glow backend. Its painter API makes custom virtualized text and hex views easy, and it idles near zero CPU. Slint is the runner-up. The cost is that you own the entire text widget (caret, selection, IME), and IME, accessibility and shaping are weak in egui.
- memmap2, memchr, `regex::bytes`, encoding_rs with chardetng, syntect (fancy-regex), pulldown-cmark, interprocess, lexopt.
- Rope crates (ropey, crop) are rejected because they are in-memory only.

**Top risks, in order:**
1. Safe save of an edited huge file.
2. mmap fragility. External truncation causes SIGBUS or an access violation, and Windows blocks replacing a mapped file.
3. The unproven footprint targets (200 ms start, under 15 MB, low idle RAM).
4. Long lines and encoding fidelity.
5. macOS distribution.

## Conflict with PROJECT.md (needs a decision)

PROJECT.md says "single small portable binary, no installer". On macOS this cannot hold:
- A bare binary is blocked by Gatekeeper unless it is signed and notarized. That needs an Apple Developer account, the hardened runtime and stapling.
- "Open With" and file associations need an `.app` bundle with `CFBundleDocumentTypes`.
- Opened files arrive as Apple Events, not argv.

Recommendation: amend the constraint to "single portable binary on Windows/Linux, single signed `.app` bundle on macOS".

Smaller cousins of the same problem:
- Windows SmartScreen warns on unsigned exes.
- Windows won't let an app make itself the default. It can only register as a candidate in HKCU.
- Linux needs a `.desktop` file.
- A universal2 macOS build roughly doubles size.

The 15 MB budget should be measured per platform.

## Key Findings

### Recommended Stack (detail in STACK.md)

Versions were verified on crates.io on 2026-10-31. Startup, size, IME and glow-versus-wgpu claims are unverified.
- **eframe/egui 0.37 (glow, default-features off):** GUI shell and painter. Alternatives were rejected: Slint (the custom text view fights the toolkit, plus license friction), iced (wgpu-first, retained model), Xilem/Floem (pre-1.0), native wrappers (three UI codebases) and Tauri (breaks the footprint targets).
- **memmap2 0.9.12:** read-only mapping, contained in one small unsafe module.
- **Custom piece table:** balanced tree with byte and newline summaries, Arc-shared for O(1) snapshots.
- **memchr 2 and regex 1.13 (`bytes`):** SIMD line scanning, literal search and chunked regex search with overlap. grep-searcher is skipped because it is not piece-table aware.
- **encoding_rs 0.8.44 and chardetng 1.0:** viewport-only decode. Detection order is BOM, then UTF-8 validation, then chardetng on the first ~64 KB.
- **syntect 5.3 (fancy-regex, no onig):** viewport-only, lazy-loaded highlighting. tree-sitter is rejected for C grammars, size and full-tree memory.
- **pulldown-cmark 0.13 plus a small native renderer:** no webview. egui_commonmark 0.26 is optional, and its compatibility with egui 0.37 is unverified.
- **interprocess 2.4.7:** single-instance path forwarding.
- **Other crates:** lexopt (CLI), rfd (dialogs), directories and serde_json (config and session), winreg (Windows association, opt-in).
- **Release profile:** fat LTO, codegen-units 1, panic=abort, strip. Windows gets `+crt-static`. Linux uses dynamic glibc. No UPX.

### Expected Features (detail in FEATURES.md)

**Must have:**
- Instant open at any size, with a byte-offset scrollbar and jump to line or offset.
- Tabs, undo/redo, selection, clipboard, save and save as, and a status bar (line:col, offset, encoding, EOL, indexing progress).
- Streaming regex find/replace with progress and cancel.
- Encoding and EOL detection and switching, with a lossless round trip (unedited bytes stay byte-identical).
- Hex view with byte overwrite.
- Viewport-only syntax highlighting and a Markdown source/rendered toggle.
- Drag and drop, CLI, Open With, recent files and session restore.
- External-change and truncation safety.
- Theme and basic preferences.

**Should have:**
- Editing tens-of-GB files with a safe, atomic, cancellable save. This is the headline differentiator.
- A background line index with live progress, usable before it finishes.
- A text/hex toggle that keeps the cursor offset.
- A banner offering hex for binary files, with no auto-switch.
- One of tail/follow or a streaming results list in v1.x.

**Defer (v2+):**
- Hex pattern search, data inspector, line filter, column select and multi-cursor, CSV mode and diff.
- Markdown extras.
- Hot-exit recovery journal.

**Anti-features:** auto-switching to hex, live split preview, plugins/LSP/file tree/command palette, silent EOL/BOM normalization, unbounded undo, in-place shifting inserts, webview Markdown, auto-update and telemetry.

### Architecture Approach (detail in ARCHITECTURE.md)

Three layers with one-way dependencies:
- a GUI-agnostic `core` crate (storage, piece tree, index, search, save, encoding)
- an `app` layer (session, tasks, IPC, watcher)
- a `ui` layer

The main thread is the single writer of Documents. Workers use `Arc` snapshots, and results with a stale version are dropped. Scroll position is a byte offset, and the canonical position type is `u64` bytes. Per-frame cost depends only on screen size. There is no tokio.

**Major components:**
1. **ByteSource:** mmap with a pread fallback behind the same trait. This is the swap point for SIGBUS and network safety.
2. **PieceTree + AddBuffer:** O(log n) edits, newline summaries, cheap snapshots and undo roots.
3. **LineIndex:** sparse checkpoints every 64 KB to 1 MB, built in the background. Never a per-line array.
4. **Viewport/Layout:** shared by TextView and HexView (fixed 16-byte rows). Caps long lines.
5. **TaskManager:** cancel tokens, generations, progress and coalesced repaint wakeups.
6. **Saver:** picks between in-place patch, temp plus atomic replace with the Windows rebase, and save as.
7. **Leaf components over snapshots:** Searcher, Highlighter, FileWatcher, Session/Tabs, SingleInstance, MarkdownView.

### Critical Pitfalls (detail in PITFALLS.md)

1. **Unsafe save of huge files.**
   - Write a same-directory temp file, fsync, copy metadata, then atomically replace. Check free space first.
   - On Windows, rebase onto the temp mapping before the rename.
   - Use in-place pwrite only for same-length overwrites.
   - Test ENOSPC, kill -9 mid-save, read-only targets, symlinks and network drives.
2. **mmap as the only I/O path.**
   - `ByteSource` must exist from day one.
   - Stat-check before viewport fetches, prefer pread on network or removable drives, and use share-delete on Windows.
   - A SIGBUS guard is a last resort.
3. **Blocking or stale line index, and unbounded long lines.**
   - Open at offset 0 with no index and index in the background.
   - Scroll by byte offset.
   - Cap line scans (about 1 MB) and shaped width.
   - Keep a corpus with a 1 GB single-line file and a file with no newlines.
4. **Lossy encoding and EOL handling.**
   - Bytes are truth and decoding is a view.
   - Preserve invalid bytes, BOM, mixed EOLs and the trailing newline.
   - The line scan is encoding-parametrized, because UTF-16 needs aligned 2-byte matching.
   - Legacy multibyte encodings are best-effort for random access.
5. **Footprint regression and the toolkit-bound text widget.**
   - Choose the toolkit via a spike with measured gates.
   - Set CI budgets for size, cold start and idle CPU.
   - Plan for the custom widget: IME, HiDPI, a bundled font with lazy fallback.
   - Make the piece table a tree from the start and fuzz it against a `Vec<u8>` oracle.

Also: treat Markdown as untrusted (no raw HTML, remote images off by default, a link scheme allowlist, a size cap of about 10 MB). Chunked search needs overlap and boundary fuzzing. Throttle repaint wakeups so idle CPU stays near 0.

## Implications for Roadmap

The researchers' build orders differ mainly on where hex, encoding, save and OS integration go. This reconciled order puts the spike gate first. Hex comes early, as a simple viewport. Save is its own high-risk phase right after editing. Highlighting and Markdown are leaf features, and OS integration comes late.

### Phase 0: Toolkit, Packaging and Footprint Spike (gate)
**Rationale:** The footprint, IME and macOS claims are unverified, and the toolkit shapes everything.
**Delivers:**
- A walking skeleton on Windows, Linux and macOS that mmaps a 20 GB sparse file and paints the first page in a virtualized monospace view.
- Measurements: cold start, idle RSS, idle CPU, stripped size, and glow versus wgpu.
- Checks on IME composition and HiDPI.
- A macOS bundle, signing and notarization pipeline in CI.
- A check that the Apple Event file-open path works.
- A go/no-go on egui, with Slint as the fallback.

**Avoids:** pitfalls 11, 13, 14, 16.

### Phase 1: Core Storage and Read-Only Text Viewer
**Rationale:** Everything depends on it, and it proves the Core Value.
**Delivers:**
- A core crate with the `ByteSource` trait (mmap and pread), `u64` offsets and file-type checks.
- A tree-based PieceTree over a single Original piece, with a proptest oracle.
- A Viewport with UTF-8 decode, a long-line cap and visible control characters.
- Byte-offset scrolling and go-to-offset.
- A background LineIndexer with progress and go-to-line.
- A basic TaskManager.
- A minimal status bar.
- A benchmark harness and CI budgets.

**Avoids:** pitfalls 1 (design), 3, 4, 5, 12.

### Phase 2: Hex View
**Rationale:** Fixed rows need no index, and it validates the shared-document model before edits.
**Delivers:**
- A read-only hex view (offset, hex and ASCII columns).
- Go to offset.
- A text/hex toggle that keeps the cursor offset.
- A binary-file banner.

**Avoids:** f32 scroll precision and 4 GB offset overflow.

### Phase 3: Editing Model, Undo and Hex Overwrite
**Rationale:** Editing is the differentiator, and undo is a log of root swaps.
**Delivers:**
- Insert, delete and replace in the piece tree.
- AddBuffer coalescing.
- Caret and selection, plus the custom text input and IME handling.
- Clipboard with size caps.
- Bounded, coalesced undo/redo.
- Hex byte overwrite as a 1-byte piece edit.

**Avoids:** pitfalls 2, 7 and the large-clipboard problem.

### Phase 4: Safe Save Pipeline (high risk)
**Rationale:** The riskiest piece. Build and test it before more features pile on.
**Delivers:**
- Strategy selection by inspecting the piece list.
- In-place patch for same-length edits.
- Temp file, fsync, metadata copy and atomic replace.
- The Windows rebase-then-rename sequence.
- Free-space precheck, progress and cancel.
- Save as, symlink and read-only handling.
- An external-modification check before save.
- Edits blocked during save in v1.
- A decision on whether undo survives a rewriting save.

**Avoids:** pitfall 6 and the Windows locked-file case of pitfall 1.
**Test:** the failure matrix on all three OSes.

### Phase 5: Encoding, Line Endings, Tabs and File-Change Safety
**Rationale:** Decode and EOL must honor the Phase 4 save path. Tabs are needed before search UI and session work.
**Delivers:**
- BOM, UTF-8 and chardetng detection, and status-bar switching with no reload.
- A UTF-16 aligned line index that re-indexes on switch.
- Byte-exact preservation of BOM, EOLs and invalid bytes.
- Multi-document tabs with per-tab ViewState, dirty indicator and close prompts.
- Stat checks on focus and before save, and the truncation guard.

**Avoids:** pitfall 9 and the stale-file crash class.

### Phase 6: Search and Replace
**Rationale:** It reuses the snapshot, job and cancel infrastructure, and replace-all is the second consumer of the streaming-to-temp path.
**Delivers:**
- Chunked, overlapped search using memmem and `regex::bytes`.
- Cancel, progress and capped streamed hits.
- Find-next, whole-word and case options, and UTF-16 pattern transcode.
- Streaming replace-all via a temp file (one undo entry, with a confirmation).
- One v1.x differentiator: a streaming results list or tail/follow.

**Avoids:** pitfall 8. Boundary fuzz tests are required.

### Phase 7: Syntax Highlighting and Markdown View (parallel leaf features)
**Rationale:** Both depend only on the Viewport API.
**Delivers:**
- Lazy, viewport-only syntect highlighting with checkpoints. Off above about 100 MB or for long lines, with a status-bar notice.
- A size-capped Markdown toggle on `.md` tabs. No raw HTML, remote images off by default, link allowlist and nesting-depth cap.

**Avoids:** pitfalls 10, 15.

### Phase 8: OS Integration, Session and Release
**Rationale:** Per-OS and fiddly, and it depends on tabs and settings.
**Delivers:**
- CLI, drag and drop, and interprocess single-instance forwarding with stale-socket cleanup and a startup-race fix.
- Recent files and session restore (byte-offset scroll, identity validation, restore the active tab first).
- Windows opt-in HKCU registration, a Linux `.desktop` file, and the macOS bundle with Apple Event handling.
- Signed releases.
- Theme and preferences.
- A final footprint audit.

**Avoids:** pitfalls 16, 17.

### Phase Ordering Rationale
- Phases 0-2 prove "instant open and smooth scroll" cheaply and settle the toolkit before the editor widget is built.
- Phases 3 and 4 are deliberately adjacent. Edits are meaningless without a trustworthy save, and save shapes the piece table and undo.
- Encoding follows save, because the save path must preserve bytes. Phase 1 already fixes the bytes-are-truth decode contract.
- Search comes after tabs and encoding, because it must run on the logical piece-table view in the document's encoding.
- Highlighting and Markdown are leaves and can slip without blocking anything.
- OS integration is late, but the macOS signing pipeline is de-risked in Phase 0.
- Footprint budgets are a standing gate in every phase, not a phase of their own.

### Research Flags
**Needs `/gsd-research-phase`:**
- **Phase 0:** toolkit viability, glow versus wgpu numbers, IME and HiDPI on egui, the winit macOS file-open path, and eframe 0.37 feature names.
- **Phase 4:** Windows mapped-file replace semantics (`ReplaceFileW`, `ERROR_USER_MAPPED_FILE`), whether in-place writes stay coherent with a live mapping, the rebase-while-editing design, and crash safety of the in-place patch.
- **Phase 5:** SIGBUS guard versus pread fallback, network and cloud-placeholder file behavior, and the Shift-JIS/GBK random-access policy.
- **Phase 8:** macOS Apple Events and notarization CI, and Windows default-app limits.

**Standard patterns (skip research):**
- **Phases 1 and 2:** piece tree, sparse checkpoints and memchr scanning have good prior art. Hex is trivial.
- **Phase 3:** persistent-tree undo is well understood.
- **Phase 6:** the chunk-overlap search pattern is well known.
- **Phase 7:** pulldown-cmark is standard. Syntect only needs a size measurement.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM | Versions are HIGH (crates.io). Startup, size, glow versus wgpu, IME quality and eframe 0.37 feature names are unverified. egui_commonmark compatibility is unchecked. |
| Features | MEDIUM | Competitor knowledge with no live verification. Recheck vendor limits such as EmEditor's. |
| Architecture | MEDIUM | Synthesized from VS Code, xi, Zed, klogg and ImHex designs. The Windows rebase sequence and in-place write coherence are unverified. |
| Pitfalls | MEDIUM | mmap, POSIX and Win32 semantics are solid. Rust GUI IME, shaping and accessibility claims are LOW. |

**Overall confidence:** MEDIUM

### Gaps to Address
- **Footprint feasibility** (200 ms, under 15 MB, tens of MB RSS) is unproven. Phase 0 must measure it, and the targets may need renegotiating, especially on macOS and with syntect bundled.
- **macOS "single binary"** needs an explicit amendment to PROJECT.md.
- **IME and accessibility** in a custom egui editor are LOW confidence. Decide the supported scope. Accessibility is best-effort for the text area.
- **macOS file-open via Apple Events** on winit 0.30 is unknown. It may need an objc2 hook or a newer winit.
- **Windows save semantics** need a Phase 4 spike. Whether undo survives a rewriting save is an open design decision.
- **Encoding policy:** support UTF-16 indexing in v1, and treat Shift-JIS/GBK random access as best-effort.
- **v1.x differentiator:** tail/follow or the results list is not yet decided.
- **Large-file test infrastructure** needs a sparse multi-GB generator and a CI strategy given disk limits.

## Sources
- **Primary (HIGH):** the crates.io API, queried 2026-10-31, for all versions listed above (including the rejected alternatives), and standard POSIX mmap/SIGBUS semantics.
- **Secondary (MEDIUM):**
  - Training knowledge of the VS Code piece tree, xi-editor, Zed SumTree, Helix/ropey, klogg, ImHex/HxD, EmEditor and UltraEdit.
  - Windows mapped-file semantics and Apple notarization.
  - Crate behavior of regex, memchr, encoding_rs and pulldown-cmark.
- **Tertiary (LOW, needs validation):**
  - Rust GUI IME, shaping and AccessKit status.
  - winit macOS Apple Event support.
  - glow versus wgpu startup and size claims, and the estimated 6-10 MB stripped size.
  - Network and cloud-file mmap behavior.
- No researcher consulted Context7, official docs or live benchmarks.

---
*Research completed: 2026-10-31. Ready for roadmap: yes.*
