# Architecture Patterns

**Domain:** Large-file text + hex desktop editor (Rust, mmap-backed, tens of GB)
**Researched:** 2026-10-31
**Overall confidence:** MEDIUM. The design is synthesized from well-known public designs (VS Code piece tree, xi-editor rope, Zed SumTree, Helix/ropey, klogg, ImHex providers, OS mmap semantics) and my own knowledge. I did no live source verification in this pass. Items marked LOW or "verify" need a spike before the phase commits to them.

## Recommended Architecture

Three layers with one-way dependencies. The core is GUI-agnostic, so it is testable headless and the toolkit choice cannot leak into it.

```
+--------------------------------------------------------------------+
| UI layer (main thread, toolkit-specific)                           |
|  TabBar | TextView | HexView | MarkdownView | StatusBar | Menus    |
|  Input -> Commands ; Viewport requests ; Repaint on worker msgs    |
+---------------------------^----------------------------------------+
                            | Commands in / Snapshots + Events out
+---------------------------v----------------------------------------+
| App core (GUI-agnostic crate)                                      |
|  Session/Tabs -> Document { PieceTree, Undo, ViewState, EncodingCfg}|
|  TaskManager (jobs, cancel tokens, progress) | FileWatcher         |
|  SingleInstance IPC server | Settings/Recent/Session persistence   |
+---------------------------^----------------------------------------+
                            | Arc<Snapshot> (immutable) to workers
+---------------------------v----------------------------------------+
| Workers (thread pool, never touch UI)                              |
|  LineIndexer | Searcher | Saver | Highlighter | EncodingSniffer     |
+---------------------------^----------------------------------------+
                            |
+---------------------------v----------------------------------------+
| Storage: ByteSource trait -> MmapSource (default) / PreadSource    |
|          AddBuffer (append-only Vec<u8>, spill-to-temp if huge)    |
+--------------------------------------------------------------------+
```

Suggested crates: `core` (storage, piece tree, index, search, save, encoding; no UI deps), `app` (session, tasks, IPC, watcher), `ui` (toolkit). Keep the core as a separate workspace crate to enforce the boundary and keep the binary small.

### Component Boundaries

| Component | Responsibility | Communicates With |
|-----------|---------------|-------------------|
| ByteSource (mmap) | Read-only random access to the original file bytes. Owns the file handle and mapping. Trait, so a pread backend can replace it for network shares or SIGBUS-sensitive cases. | PieceTree, LineIndexer, Searcher, Saver |
| AddBuffer | Append-only store for all inserted or overwritten bytes. Never mutated in place, so snapshots and undo stay valid. | PieceTree |
| PieceTree | Ordered sequence of pieces `{src: Original or Add, start, len}` in a balanced tree (B-tree or SumTree style). Each node summarizes total bytes and newline count. Gives O(log n) offset lookup, edit, and snapshot (cheap clone via Arc structural sharing). | Document, all workers (as snapshot) |
| Document | Owns the PieceTree, undo stack, dirty flag, encoding and line-ending settings, and the base file identity (size, mtime, file id). Single writer, on the main thread. | UI, TaskManager, Saver |
| LineIndex | Sparse, lazily built table over the ORIGINAL file: newline count at every N-th byte checkpoint (for example every 1 MiB). It is not a per-line offset array. Edited regions are handled by the tree's newline summaries. | PieceTree (range newline count), TextView (line number to offset) |
| Viewport/Layout | Given a byte offset (the scroll position) and a pixel height, pulls roughly one screen of bytes from the snapshot, decodes, splits lines, wraps (capped), and returns display lines. | TextView, HexView, Highlighter |
| Highlighter | Viewport-only syntax highlighting. Line-based state with a bounded look-back, and checkpoints of lexer state per N lines near the viewport. Disabled above a size or line-length threshold. | Viewport |
| Searcher | Chunked scan of a snapshot on worker threads, with overlap between chunks, a cancel token, progress, and streamed hits. | TaskManager, UI (results panel and status) |
| Saver | Chooses the save strategy, writes, remaps, and rebases the document. | Document, ByteSource, FileWatcher |
| FileWatcher | Detects external modification, deletion, or truncation. | Document (stale flag), UI (prompt) |
| TaskManager | Spawns jobs, hands out cancel tokens and generation numbers, collects progress and results on a channel, and wakes the UI. | Everything async |
| Session/Tabs | Tab list, per-tab ViewState (scroll byte offset, cursor, selection, mode text/hex/md, encoding), restored last session, recent files. | Document, persistence |
| SingleInstance | Local-socket server in the first process. Later launches send args and exit. | Session/Tabs, UI window |
| MarkdownView | Renders the source with a CommonMark parser (for example pulldown-cmark). Only for files under a size cap. Above the cap, show "too large to render". | Document (reads a snapshot) |

### Data Flow

Edits (main thread only):
`Input -> Command -> Document.apply(Edit) -> PieceTree' (new root, old root kept for undo) -> dirty+version++ -> UI repaint`.

Reads (viewport pull):
`UI frame -> ViewState.scroll_offset -> Document.snapshot() -> Viewport.fetch(offset, rows) -> bytes from pieces (mmap slices or Add buffer) -> decode (encoding_rs) -> lines -> highlight -> draw`. Cost per frame depends only on screen size, not file size.

Async jobs:
`Command (open, search, save, index) -> TaskManager.spawn(job, Arc<Snapshot>, CancelToken, generation) -> worker -> channel(Progress | Result | Done) -> main thread drains channel each frame -> applies if generation still current -> request_repaint`. Workers never mutate the Document. A result carrying a stale document version or generation is dropped, or marked "from an older version".

Open:
`path -> stat + open + mmap -> PieceTree with one Original piece (len = file size) -> first viewport rendered immediately (offset 0) -> LineIndexer job starts in background -> status bar shows "indexing 37%"`. Nothing waits on the index.

Hex and text share one Document. Both views read the same PieceTree. Hex view is a different `Viewport` (fixed 16 bytes per row, so offset is `row*16`, no index needed). A hex byte overwrite is `replace(range of 1 byte, 1 byte)`.

## Patterns to Follow

### Pattern 1: Piece table over mmap plus append buffer (balanced tree, not a Vec)
**What:** The original file is piece 0. Every insert appends to AddBuffer and splits a piece. A delete trims or removes pieces. Bytes are never copied from the original.
**When:** Always. A flat `Vec<Piece>` is fine for small docs, but after millions of edits (replace-all, multi-cursor) it makes inserts O(n). Use a tree with summed metrics from the start. This is how VS Code (red-black piece tree with line-feed counts per node, ~64 KB chunks), Zed (SumTree), Helix (ropey), and xi (rope B-tree) get log-time edits.
**Key design points:**
- Metrics per node: bytes, newlines (and optionally UTF-16-unit or char counts if needed). Newline count for an Original piece is computed from the LineIndex (checkpoint plus partial scan). Until the index is ready it is "unknown". Fall back to approximate line numbers shown as "~".
- Make nodes Arc-shared (persistent structure). `snapshot()` is then an O(1) clone. This is what lets search and save run while the user keeps typing.
- Work in BYTE offsets internally. Convert to (line, col) only at the edges. Decoding is a view concern.
- Hex overwrite creates a 1-byte Add piece, so file length is unchanged. Adjacent overwrites should coalesce into a single piece. ImHex uses a similar "patch overlay" idea (offset to byte map over a Provider). A piece table subsumes it and also gives text and hex one consistent undo history.

### Pattern 2: Sparse, lazy line index with checkpoints (klogg-inspired but cheaper)
**What:** klogg indexes a read-only file in the background and stores line offsets with progress reporting. Storing one offset per line is 8 bytes times maybe billions of lines, which is tens of GB. Use checkpoints instead.
**Design:**
- Background worker scans the mmap in large chunks (for example 4-16 MiB) using `memchr`, SIMD-fast, I/O-bound after the first pass. It records `(byte_offset, cumulative_newlines)` every ~64 KiB-1 MiB. Memory is a few MB even for 50 GB.
- Line N to offset: binary-search checkpoints, then scan forward at most one checkpoint interval. Offset to line number: the reverse.
- Scrollbar is driven by BYTE offset (always correct and instant). Line numbers in the gutter become exact once the index passes that offset, or are exact from the start if the user has not jumped.
- Jump to line N beyond the indexed prefix: show progress and either wait or scan on demand, cancelable. Jump to offset is always instant.
- Incremental: indexer takes a priority hint so it indexes near the viewport first if needed (optional, v2).
- Edits do not rebuild the index. The tree stores newline counts for Add pieces, and Original piece counts are derived from checkpoints plus a partial-chunk scan.
- Pathological input: one 20 GB single line. Cap displayed line length (render a window of N columns around a horizontal byte offset). Never lay out an unbounded line.

### Pattern 3: Immutable snapshots to workers, results tagged with version
**What:** Workers receive `Arc<PieceTreeSnapshot>` plus `Arc<ByteSource>`. They never lock the live document. This avoids the xi-editor trap of tangled async state between front-end and core (the xi retrospective cited async/RPC complexity as a major cost; keep the app single-process with channels).
**When:** Search, save, index, replace-all, markdown render.

### Pattern 4: Chunked search with overlap and cancellation
**What:** Iterate the snapshot as byte chunks (several MiB), with an overlap of `max_match_len - 1` (literal) between chunks. Use `memchr::memmem` for literals and `regex::bytes::Regex` for regex. Check an `AtomicBool` cancel token and a generation counter each chunk. Send `Progress(bytes_scanned/total)` and `Hit(range)` batches over a bounded channel. Cap hits in memory (for example first 100k, with "more..." and a count-only continuation). Run chunks in parallel for the initial scan only if hits stay ordered (parallel scan with per-chunk results merged in order).
**Constraints to expose:** multi-line regex across chunks needs a bounded overlap; unbounded patterns such as `(?s).*` should be rejected or documented. Case-insensitive search over non-ASCII encodings should decode per chunk (see Pattern 5). Searching UTF-16 means transcoding the pattern to UTF-16 bytes, or decoding chunks. Prefer transcoding the pattern for literal searches.
**Replace-all on huge files:** do not create millions of pieces in the tree. Do a streaming transform to a temp file, then swap in as a save-like operation. The result becomes the new base, with a single undo entry ("restore previous file" is costly, so confirm with the user first or keep the old file as the undo base until the session closes).

### Pattern 5: Encoding decode at the viewport only
**What:** The storage layer is bytes. Decode only the visible window.
- Detect once at open: BOM, then a sniff of the first ~64 KB (valid UTF-8 check, `chardetng`, or `encoding_rs` heuristics). Cache the result per Document. User override re-renders, no reload.
- Chunk-boundary rule: when starting a decode at an arbitrary byte offset, resync. For UTF-8, skip continuation bytes (`10xxxxxx`). For UTF-16, align to even offsets and avoid starting in the middle of a surrogate pair. For legacy multibyte (Shift-JIS, GBK), start from a line start, because trail bytes can be ambiguous.
- Line splitting is encoding-dependent. UTF-8 and Latin-1 and most ASCII-superset encodings: scan for `0x0A`. UTF-16/32 need 2- or 4-byte-aligned newline matching. Parametrize the LineIndexer by `EncodingKind`. Changing the encoding to or from UTF-16 invalidates the index and re-indexes in the background.
- Invalid sequences: show U+FFFD in view but keep the underlying bytes untouched, so a save of unedited regions is byte-exact. Never re-encode untouched pieces. This is why the piece table matters for fidelity.
- Newly typed text is encoded into AddBuffer in the document's current encoding at insert time.

### Pattern 6: Undo as a log of piece-range swaps
**What:** Each edit records `{ at_offset, removed_pieces: Vec<Piece>/old subtree root, inserted_pieces }`. Undo swaps them back. Cost is proportional to the number of pieces, not bytes. Because AddBuffer is append-only and the original is immutable, no data is copied or invalidated. With a persistent tree, an undo entry can simply be the previous root (Arc), O(1).
- Coalesce typing runs (time or word boundary) into one entry. Group replace-all as one entry.
- Memory cap: AddBuffer grows with every edit including undone ones. For typical sessions this is fine. For huge pastes, spill AddBuffer to an anonymous temp file via a second mmap. Plan this behind the same trait.
- Undo/redo is linear in v1. Skip undo trees.
- Save does not clear undo. After save, the base changes (see Pattern 7), but undo roots can stay valid if the rebase maps offsets by position, since content identity is preserved. A simpler policy for v1 is to keep the undo stack by retaining the old mapping until the user closes the tab. That is expensive on Windows (old file stays mapped). Prefer to clear undo after a rewriting save if the rebase is hard, and document it. Decide in the Save phase.

### Pattern 7: Save strategies
Pick at save time by inspecting the piece list (a cheap tree walk):

1. **Nothing changed length and every Original piece is still at its original offset** (hex overwrites, same-length text replacements): **in-place patch**. Open the file for write and `pwrite`/`seek_write` each Add piece to its target offset, then `sync_data`. Cost is proportional to the changed bytes only, and it needs no extra disk space. Works on Windows even while the file is mapped, because writes through a file handle are coherent with the mapping on the same machine. Caveat: not atomic. A crash mid-save can leave a partial write. Mitigate by writing a small journal or undo-log sidecar for large patch sets, or accept it and document it. Verify the coherence claim on Windows and on network filesystems in a spike (MEDIUM confidence).
2. **Length changed, edits only affect a suffix** (for example appends or edits near the end): optional optimization: in-place rewrite from the first modified offset, only if no later Original piece would need to be read from a region that has already been overwritten. Source and destination overlap is the hazard. Skip in v1. Use strategy 3.
3. **General case: write-to-temp-then-replace.** Stream the pieces into a temp file in the SAME directory (same volume, so rename is atomic), `sync_all`, copy permissions/attributes (and ACLs/xattrs where feasible), then replace. Needs free space of about one file size. Check free space up front and fail early with a clear message. Offer "save as" elsewhere.
   - **Linux/macOS:** `rename(tmp, target)` works while the old file is mapped; the old inode lives until unmapped. After rename, open and map the new file, then rebase.
   - **Windows:** a file with an active mapped view cannot be truncated, and replacing or deleting it typically fails (sharing/`ERROR_USER_MAPPED_FILE`/access denied) while a view is open, even if share-delete is granted. Sequence: finish writing temp, **drop the mapping and handle for the original** (all pieces that referenced Original now need to read from the temp), then `MoveFileEx(REPLACE_EXISTING)` or `ReplaceFileW`, then map the new file. To avoid a window where Original data is unavailable, rebase first: map the TEMP file as the new Original (it has identical content to the post-edit document), swap the document to a single Original piece over the temp mapping, unmap the old file, then rename temp over target. A mapped file can be renamed on Windows if opened with `FILE_SHARE_DELETE`, but renaming over an existing target that is mapped is what fails. Since the old target is the one being replaced and it is unmapped by then, this works. Verify in a spike (MEDIUM: documented behaviour, exact error codes not re-verified).
   - Hold an in-process lock so the file watcher ignores our own write (compare against the expected new size/mtime).
   - Run the Saver on a worker against a snapshot. The UI stays live. Edits made during the save remain on the document as new dirty edits on top of the saved snapshot. This requires the rebase to apply to the live tree (map saved-snapshot pieces to the single Original piece, keep newer edits as pieces layered on it). That is easiest if the rebase is expressed as "replace all pieces that were part of snapshot S with one Original piece" and newer edits are limited to later versions. Simplest v1: block edits (modal progress) during save and keep non-blocking save for later.
4. **Save As:** same as 3 but target elsewhere. The document then re-points to the new file.
- Preserve: line endings as stored (do not normalize untouched lines), BOM, trailing newline state, file permissions, symlink target (write through the link, not over it), read-only attribute (prompt).
- Durability: fsync temp before rename; fsync directory on Linux for strictness (optional).
- Pre-save check: stat the target. If its size/mtime differ from the loaded identity, warn about an external modification (see below).

### Pattern 8: File-change detection
- Use `notify` (inotify / ReadDirectoryChangesW / FSEvents) on the parent directory, plus a stat compare `(len, mtime, file id)` when the window regains focus and before save. Watchers drop events and network shares do not deliver them, so the stat check is the source of truth. Debounce.
- **mmap safety is the real issue:** if another process truncates the file, reading past the new end is SIGBUS on Linux/macOS (process dies) and an access violation / `EXCEPTION_IN_PAGE_ERROR` on Windows. Mitigations, in order: (a) a stat check before each viewport fetch from Original (cheap, narrows the window but is not airtight); (b) mark the doc "stale", stop reading Original beyond known size, and prompt reload; (c) the PreadSource backend uses explicit reads that return errors instead of faults, an option for removable or network paths; (d) optional signal handler / structured exception handler guard (complex, last resort). This is the single biggest robustness risk of the mmap design. Plan a `ByteSource` trait from day one so (c) is a swap, not a rewrite.
- Appended-only change (log growing): optional "follow/tail" mode that extends the Original piece and index. This is a differentiator for log files, not v1.
- Deleted/moved: keep working from the mapping if the OS allows (Linux does, Windows usually blocks deletion of a mapped file), show a "file missing" banner, and offer Save As.
- Network drives and cloud placeholder files (OneDrive): mmap of a network file may fault slowly. Detect via drive type and prefer PreadSource with a read-ahead cache. LOW confidence, needs testing.

### Pattern 9: Threading model
- **Main/UI thread:** owns all Documents, ViewState, and the event loop. Never does I/O proportional to file size. Page faults on mmap reads of the viewport can still block it on cold disk or network. Prefetch with `madvise(WILLNEED)` / `PrefetchVirtualMemory` for the next screens, and keep viewport reads small.
- **Worker pool** (rayon or a small custom pool, plus dedicated threads for the indexer and save): jobs communicate only through `Arc` snapshots in and `crossbeam`/`std::mpsc` channels out. Every job has a `CancelToken` (`Arc<AtomicBool>`) and a generation. Cancel on new search, tab close, or a new edit that invalidates the job.
- Wake the UI from workers with the toolkit's repaint-request handle (for example egui `Context::request_repaint`), and coalesce wakeups.
- Avoid a heavyweight async runtime (tokio) in the binary. It costs size and startup, and nothing here needs it.
- Cold start: show the window first, then do session restore and recent files lazily. Do not index the other tabs on restore until they are activated.

### Pattern 10: Tab/session model
- `Tab { doc: Document, view: ViewState{mode, scroll_offset_bytes, cursor_byte, selection, wrap, hex_cols}, id }`. Scroll position is stored as a BYTE offset, not a line, so restore works without the index.
- Persist per-session JSON/TOML in the OS config dir: tab paths, mode, scroll offset, cursor, encoding override, window geometry. Recent files list in the same place (store with last-open offset).
- Unsaved edits across restarts ("hot exit"): v1 prompts on close. Persisting the edit journal (AddBuffer plus piece list plus base-file identity) is a possible later feature, and the piece table makes it natural. Large unsaved state is the risk. Defer.
- Restore validates that the file identity still matches (size, mtime). Otherwise open at offset 0.
- Memory: inactive tabs keep only the mapping (virtual, not resident) and the sparse index. Idle RAM stays low because mmap pages are reclaimable.

### Pattern 11: Single-instance IPC for "Open with"
- On startup, before creating the window, try to connect to a per-user local socket: a Unix domain socket in `$XDG_RUNTIME_DIR` (Linux/macOS: under the user's temp or app dir) or a named pipe on Windows. The `interprocess` crate provides local sockets with a cross-platform API (verify the current version and API). If connected: send `{cwd, args[]}` as one small line-delimited JSON message, wait for an ack, then exit. This keeps launch of the second process well under 50 ms and window-free.
- If no listener: become the primary, bind the socket (remove a stale socket file on Unix after a failed connect), and start a listener thread that forwards messages to the main thread via the channel and a UI wake-up, then raises and focuses the window.
- Race: multi-select "Open with" on Windows launches N processes at once. Use a named mutex or a lock file during primary election. Secondaries retry connecting for a short time. Order of tabs may vary; accept it.
- Resolve relative paths against the sender's cwd. Canonicalize paths, and if the file is already open, focus its tab.
- macOS differs: "Open with", dock-drop and Finder double-click deliver files through Apple Events (`application:openFiles:`), not argv. The windowing layer must surface that event. Check whether the chosen toolkit does (winit has had handling for this; verify for the chosen version). LOW confidence. Plan a spike. Also, macOS normally enforces a single instance already for app bundles, so IPC is mainly for Windows and Linux.
- File associations are packaging concerns (Windows registry keys written by an optional "Register as default" command, `.desktop` file on Linux, `Info.plist` document types on macOS). They are outside the core architecture but affect the "single portable binary" goal. Writing registry keys from the binary on demand keeps it portable.

## Reference Designs and What to Borrow

| System | What it is | Borrow | Avoid / Caveat |
|--------|-----------|--------|----------------|
| VS Code piece tree (TS) | Red-black tree of pieces over ~64 KB chunks, with line-feed counts per node | Per-node newline metrics, chunked buffers, tree-of-pieces edits | Needs the file loaded in memory (chunks). Its large-file handling has size limits, so it does not scale to tens of GB |
| xi-editor | Rope (B-tree with metrics), separate core process, JSON-RPC front-ends, async plugins | Rope metrics, persistent snapshots, delta-based edits | The project was wound down. The author's retrospective cites async/RPC split complexity and rope/line-wrap/render trade-offs as costly. Stay in-process |
| Zed | SumTree-based rope and buffer, CRDT fragments for collaboration, GPUI | SumTree is a good general B-tree-with-summaries model; snapshots are cheap | CRDT is unnecessary here. Zed is not designed for tens-of-GB files |
| Helix | ropey rope, selections-first editing, transactions for undo | Transaction-based undo, simple edit model | Rope needs chars/lines in memory (loads files) |
| klogg | Qt log viewer. Background indexing of line offsets with progress, fast regex search over huge files, read-only | Background index + progress UX, search in chunks with live results, no-load philosophy | Per-line offset storage is heavy. It is read-only, so there is no edit model |
| HxD | Windows hex editor with disk/RAM editing and big files | Section-based file view, in-place overwrite semantics | Closed source, so only observable behaviour |
| ImHex | Hex editor with a Provider abstraction (file, memory, process, disk) and a patch overlay | Provider/ByteSource trait with swappable backends, patch overlay concept | Heavy UI and feature set |
| Notepad++/Scintilla | Gap buffer, loads whole file | Counter-example for what to avoid | |

(Capabilities above are from my knowledge, not freshly verified.)

## Anti-Patterns to Avoid

### Anti-Pattern 1: A line-offset array for the whole file
**Why bad:** 8 bytes times line count. A 50 GB log with ~500M lines is ~4 GB of index, which breaks the low-RAM goal. Build time blocks features.
**Instead:** Sparse checkpoints (Pattern 2) plus byte-offset scrolling.

### Anti-Pattern 2: Decode or normalize the whole document
**Why bad:** It forces a full read, mangles invalid bytes, and changes line endings on save.
**Instead:** Bytes are the truth. Decode only the viewport. Never re-encode untouched pieces.

### Anti-Pattern 3: Workers share the mutable document behind a Mutex
**Why bad:** UI stalls while a search holds the lock, and edits during a job produce inconsistent results.
**Instead:** Persistent tree and `Arc` snapshots with version tags.

### Anti-Pattern 4: Flat Vec of pieces
**Why bad:** O(n) offset lookup per frame and per edit after heavy replace-all use.
**Instead:** Balanced tree with summed metrics.

### Anti-Pattern 5: Overwriting or truncating the file while it is mapped (Windows)
**Why bad:** Fails with sharing violations, or crashes readers with page faults.
**Instead:** Rebase-then-rename (Pattern 7) and keep an in-place path only for length-preserving patches.

### Anti-Pattern 6: Layout of unbounded lines and unbounded wrap
**Why bad:** A single multi-GB line hangs layout.
**Instead:** Cap line segment length per row, and let soft-wrap work on a window around the viewport.

### Anti-Pattern 7: Trusting the file watcher alone
**Why bad:** Events are dropped or not delivered, especially on network shares.
**Instead:** Stat on focus and before save, plus the watcher as a hint.

## Scalability Considerations

| Concern | 100 MB file | 10 GB file | 50+ GB file |
|---------|-------------|------------|-------------|
| Open | mmap, instant | mmap, instant | mmap, instant (address space is fine on 64-bit; 32-bit unsupported) |
| Line index | Under a second | Tens of seconds to minutes, disk-bound; background | Many minutes; background, resumable priority near viewport |
| Index RAM | KBs | A few MB (checkpoints) | Tens of MB at most, tune interval |
| Search | Seconds | Disk-bound minutes, streamed hits | Same, with cancel and progress essential; cap hits |
| Edit | Tree O(log n) | Same | Same |
| Save (length changes) | Temp rewrite, seconds | Temp rewrite needs 10 GB free and minutes; progress and cancel | Same; warn up front about disk and time; in-place patch for hex overwrite |
| Highlighting | Viewport | Viewport, long-line guard | Viewport, disable above threshold |
| Markdown render | Full parse OK | Refuse above a cap (for example 5-10 MB) | Refuse |

## Suggested Build Order (dependency-driven)

1. **ByteSource + mmap open + PieceTree (read-only, single Original piece) + Viewport fetch** with a bare text/hex dump renderer. Proves the < 1 s first page on a 20 GB file. Everything depends on this. Choose the GUI toolkit here by building a spike of a virtualized text view for it.
2. **Hex view** over the same snapshot. It is cheap (fixed rows, no index) and validates the shared-document model early.
3. **Text view basics:** UTF-8 decode at viewport, byte-offset scrollbar, line splitting, long-line cap. Then the **LineIndexer** (background, checkpoints, progress), jump to line/offset, line-number gutter.
4. **Edit model:** tree-structured PieceTree with insert/delete/replace, AddBuffer, cursor/selection, clipboard, **undo/redo**. Hex overwrite lands here too.
5. **Save:** in-place patch, temp-and-rename, and the Windows rebase. The riskiest piece, so put it right after edits and test it against huge files and on all three OSes. Add "Save As" and the dirty/close prompts.
6. **Encoding and line endings:** detection, status bar, switching, UTF-16 index variant, byte-exact preservation. (Can move before step 5 if the file format matters earlier, but the save path must honor it.)
7. **TaskManager + Searcher** (find/replace, regex, cancel, progress, streamed hits). Replace-all streaming variant.
8. **Tabs/Session:** multiple documents, per-tab ViewState, session restore, recent files.
9. **FileWatcher + stale-file handling** (and mmap truncation mitigation). PreadSource fallback if the spike shows it is needed.
10. **Single-instance IPC, CLI, drag and drop, file associations** (mac Apple Events).
11. **Syntax highlighting** (viewport-only with size and line-length guards) and **Markdown toggle** (size-capped). Both are leaf features that depend only on the Viewport API, so they can be built in parallel with 7-10 by a second track.

Reasoning: 1-3 prove the Core Value (instant open, smooth scroll). 4-5 prove the editing claim and carry the main architectural risk. Anything async (6+) reuses the snapshot and job infrastructure, which should be introduced no later than the indexer in step 3 (so TaskManager's basic form arrives with the LineIndexer, not with search).

## Sources

- VS Code piece tree write-up: "Text Buffer Reimplementation" (code.visualstudio.com/blogs/2018/03/23/text-buffer-reimplementation). From knowledge, not re-fetched. MEDIUM.
- xi-editor docs and retrospective (github.com/xi-editor/xi-editor, "Rope science" docs). From knowledge. MEDIUM.
- Zed SumTree and Buffer (github.com/zed-industries/zed, crates/sum_tree, crates/text). From knowledge. MEDIUM.
- Helix / ropey (github.com/cessen/ropey). From knowledge. MEDIUM.
- klogg (github.com/variar/klogg). Indexing and search design from knowledge. MEDIUM.
- ImHex provider/patch design (github.com/WerWolv/ImHex). From knowledge. MEDIUM.
- Windows mapped-file semantics (Microsoft Docs, "CreateFileMapping", "MapViewOfFile", `ERROR_USER_MAPPED_FILE`). From knowledge. MEDIUM. Verify in a spike.
- POSIX mmap SIGBUS on truncation. Standard behaviour. HIGH.
- Crates (`memmap2`, `memchr`, `regex`, `encoding_rs`, `chardetng`, `notify`, `interprocess`, `crossbeam`, `pulldown-cmark`). Names and roles from knowledge; check current versions and APIs when adding. MEDIUM.
- macOS Apple Event file-open delivery and toolkit support. LOW. Spike needed.
