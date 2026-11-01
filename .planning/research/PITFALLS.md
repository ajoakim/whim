# Domain Pitfalls

**Domain:** Rust cross-platform text + hex editor for tens-of-GB files, with Markdown rendering
**Researched:** 2026-10-31
**Overall confidence:** MEDIUM. Based mostly on established systems knowledge (mmap semantics, OS behaviors, editor-architecture literature). No live web verification was done in this session. Items marked [verify] need a spike or doc check in their phase.

## Critical Pitfalls

### 1. mmap as the only I/O path (SIGBUS / access violation / locked files)
**What goes wrong:** If another process truncates the file, or the file sits on a network drive, USB stick or cloud-sync folder that goes away, touching a mapped page raises SIGBUS (Unix) or EXCEPTION_IN_PAGE_ERROR (Windows). Rust cannot catch this, so the app crashes and the user loses unsaved edits. On Windows, a mapped file cannot be truncated, replaced or deleted. This breaks the usual "write temp, rename over original" save, and other tools (log rotation) fail against the open file. The `memmap2` docs state that mapping is `unsafe` because of exactly this.
**Why it happens:** mmap is attractive because it is simple and fast, so it becomes the sole abstraction. The safety contract (file must not change) is ignored.
**Consequences:** Crashes on live logs, which is a primary use case. Data loss. Unreplaceable files on Windows. Slow, hanging page faults on SMB/NFS.
**Prevention:**
- Put a `ByteSource` trait between the document model and the I/O. Implement it with both mmap and `pread`/`ReadFile` chunked reads with a small LRU cache. Make mmap an optimization, not an assumption.
- Detect network or removable filesystems (`GetDriveType`, `statfs` f_type) and default to pread there.
- Use `FILE_SHARE_READ|WRITE|DELETE` on Windows so other tools can still write and rotate the file.
- Store edits only in memory (piece table add-buffer) and never write into the mapped file in place.
- Install a stat check (size and mtime) before and after each read burst. Treat shrinkage as "file changed externally" and show a banner.
- Optionally add a SIGBUS guard via sigsetjmp or a segv handler for reads. This is fragile, so prefer pread fallback [verify].
**Detection:** Test that truncates the file while it is open, on every OS. Test that opens a file from a network share. Test that tries to save over a file that is open in the app.
**Phase:** Phase 1 (storage abstraction). Retrofitting is a rewrite.

### 2. Piece table that doesn't scale (piece count and line index)
**What goes wrong:** A `Vec<Piece>` with O(n) insertion and O(n) offset-to-piece lookup degrades after tens of thousands of edits (find-replace-all on a GB file creates millions of pieces). A line-start index kept as a plain Vec shifts on every edit.
**Why it happens:** The textbook piece table works for small files. Replace-all and paste-heavy sessions were never tested.
**Consequences:** Typing lag after a big replace. Memory growth that violates the "low RAM" goal.
**Prevention:**
- Store pieces in a balanced tree (B-tree or rope-like) augmented with subtree byte length and newline count. That gives O(log n) offset-to-line, line-to-offset and edit.
- Coalesce adjacent pieces on contiguous typing.
- Make replace-all stream into a new file instead of generating millions of pieces (see pitfall 6).
- Fuzz the document model against a naive `Vec<u8>` oracle with proptest from day one.
**Detection:** Benchmark of 1M edits. Memory after replace-all.
**Phase:** Phase 1/2 (document model).

### 3. Line indexing that blocks open or goes stale
**What goes wrong:** Scanning 20 GB for newlines takes roughly 5-20 s from NVMe and far longer from HDD or network. Done synchronously, it breaks the "<1 s first page" goal. Showing exact line numbers or "line N of M" forces the full scan. The scan also pulls the whole file through the page cache and evicts other data.
**Prevention:**
- Open at byte offset 0 with no index. Run the index build in a background thread. Use `memchr` with SIMD, and report progress.
- Display "line ~N" or byte offset until the index completes. Make the scrollbar byte-based, not line-based.
- Use a sparse index (a checkpoint every 64 KB or 1 MB) plus on-demand scanning in the viewport, so jump-to-line works progressively.
- Make the index cancellable and invalidated by file-change detection.
- Jump to an arbitrary byte offset must land on a line start by scanning backward a bounded distance (see pitfall 4).
**Detection:** Time-to-first-paint on a 20 GB file in CI or a manual benchmark. Jump-to-line during indexing.
**Phase:** Phase 2 (viewport and navigation).

### 4. Extremely long lines (multi-GB single-line JSON or minified files)
**What goes wrong:** Almost every editor and GUI text layout assumes a line fits in memory. Shaping a 2 GB line allocates gigabytes. Soft wrap needs a full layout pass. Scanning backward for the previous newline never ends. Horizontal scrolling width needs the max line length.
**Consequences:** Hang or OOM on exactly the files this tool exists for.
**Prevention:**
- Design the viewport around visible byte ranges, not lines. Layout only the visible window of a long line (e.g. N columns around the scroll column), cut at a char boundary.
- Cap the backward and forward scan for line boundaries. Beyond the cap (e.g. 1 MB), treat it as a "virtual line break" for display purposes.
- Soft wrap should be computed per visible chunk only, using fixed-width fallback. Disable wrap and highlighting above a line-length threshold.
- Never hand a whole line to the shaping engine. Cap the shaped slice to the visible columns.
- Keep a test corpus: a 1 GB single-line file, a file with a 100 MB line followed by small lines, and a file with no newlines at all.
**Phase:** Phase 2, with the rendering contract fixed in Phase 1.

### 5. Offset, char and grapheme confusion
**What goes wrong:** Byte offsets are mixed with char indices and columns. Cursor movement splits UTF-8 sequences, ZWJ emoji or combining marks. Slicing a `&str` panics at non-boundaries. Selections, find results and hex offsets drift apart. A viewport that starts at an arbitrary byte offset (from a scrollbar drag) lands mid-codepoint.
**Prevention:**
- The canonical position type is the byte offset (`u64`). Never use `usize` for file offsets (32-bit builds and clarity).
- Convert to columns or graphemes only inside the visible window, with `unicode-segmentation`. Resync to a char boundary by scanning at most 3 bytes back.
- Work with `&[u8]` plus lossy or incremental decoding, not `&str`, for the file content. Validate per chunk with `simdutf8` or `from_utf8` handling `valid_up_to` and `error_len`.
- Define one type for each unit (newtype `ByteOff`, `Col`, ...) so the compiler catches mixing.
- Property tests with multi-byte, combining and ZWJ strings.
**Phase:** Phase 1 (types), Phase 2 (cursor and selection).

### 6. Saving huge files unsafely
**What goes wrong:**
- Writing in place to a mmapped source while the piece table still references the old bytes corrupts the file, since bytes shift under pieces that have not been copied yet.
- A save that needs a full copy of 30 GB fails with ENOSPC halfway. If it wrote over the original, the file is destroyed.
- Rename-over-original loses permissions, ownership, ACLs, xattrs, hard links, symlink identity, and on macOS resource forks and quarantine flags. On Windows, rename fails while the file is mapped or open.
- Crash mid-save leaves half a file.
**Prevention:**
- Default save path: write a temp file in the same directory (same filesystem), stream pieces in order, `fsync`, copy permissions and metadata, then atomic replace (`rename` on Unix; `ReplaceFileW` on Windows, which keeps attributes). Before this, check free space (`fs2` or `GetDiskFreeSpaceEx`) and tell the user the required space up front.
- Release or remap the source mapping before the replace on Windows. After the save, reopen the new file and rebase the piece table onto it.
- Special in-place fast paths: (a) pure byte overwrite (hex edit, same length) can be written directly to the file with pwrite. Use an undo journal if crash safety matters. (b) Appends. Offer these because they avoid the 30 GB copy.
- If the temp dir lacks space, offer "Save As" elsewhere. Do not silently fall back to in-place.
- Handle symlinks (resolve and write to the target), read-only files, and files that changed on disk since opening (compare mtime and size, prompt).
- Show save progress and make it cancellable, with the temp file cleaned up.
- Test the failure matrix: ENOSPC (small tmpfs/VHD), kill -9 mid-save, read-only target, symlink, network drive.
**Phase:** Phase 3 (editing and save), but design the piece table with this in mind in Phase 1.

### 7. Undo history memory growth
**What goes wrong:** Storing the deleted text for each edit (including a 5 GB selection delete, or replace-all across the file) holds gigabytes in memory. Unlimited history grows indefinitely.
**Prevention:**
- With a piece table, undo records piece-list snapshots or deltas, not text. Deleted text stays in the original file or add-buffer, so deletion is cheap.
- Keep the add-buffer append-only. Bound its growth, and compact or spill it to a temp file when above a threshold.
- Coalesce typing into groups. Cap history by count and bytes. Be explicit when replace-all is undoable (store as one composite op, or make it non-undoable with a warning).
- Hex overwrite undo stores (offset, old byte), which is tiny. Do not keep the old bytes of a huge range paste in RAM.
- Important: undo only works while the source file is unchanged underneath. Tie this to external-change detection.
**Phase:** Phase 3.

### 8. Search that breaks at chunk boundaries or stalls the UI
**What goes wrong:** Matches spanning two chunks are missed or double-counted. `regex` crate matches can be arbitrarily long, and `^`/`$` and `\b` behave wrongly at chunk edges. Multi-line regex over a 30 GB file with unbounded `.*` buffers everything. Searching the piece table (edited content) differs from searching the on-disk file. Replace-all on a huge file may be an O(n^2) trap.
**Prevention:**
- Use `memchr::memmem` for literals, with chunk overlap of (needle length - 1).
- For regex, use `regex::bytes` (invalid UTF-8 safe), or `regex-automata` streaming/hybrid engines. Use overlapped windows aligned on line boundaries, and document that multi-line matches are limited to a window (e.g. 1-16 MB) and say so in the UI. Search per-line by default. Chunks end at a newline, with a fallback cap for long lines.
- Search runs on a background thread with cancellation and with results streamed to the UI. Count matches lazily. Cap stored results.
- Searching must operate on the piece-table view (an iterator over logical bytes), not the raw file.
- Replace-all streams to a new file (same as save). Keep it a single pass.
- Encoding: UTF-16 files must be searched in their native encoding or transcoded per chunk. Searching UTF-16 bytes with a UTF-8 pattern silently finds nothing.
- Test with boundary fuzzing: place matches at every offset relative to chunk size and compare with a whole-buffer search.
**Phase:** Phase 4.

### 9. Encoding and line-ending handling
**What goes wrong:**
- Detecting encoding from the whole file is impossible; reading all of it violates the model. Heuristics on the first N KB mislabel files (Latin-1 vs Windows-1252 vs UTF-8, and UTF-8 with an invalid byte at 100 MB).
- UTF-16 requires BOM sniffing, surrogate pairs and an endian decision. Chunks may split a surrogate pair or a CRLF. Line indexing on UTF-16 must search for 0x0A00/0x000A at the correct alignment. A naive memchr for `\n` is wrong. Odd byte offsets are invalid.
- Invalid UTF-8 handled by `String::from_utf8_lossy` irreversibly rewrites bytes on save (U+FFFD). Data corruption on a "view then save".
- Mixed line endings get normalized on save, silently modifying every line.
- BOM dropped on save. Trailing newline added or removed.
**Prevention:**
- The file's bytes are the source of truth. Decoding is a view layer only. Invalid sequences display as a placeholder glyph but keep their raw bytes, and are written back unchanged unless edited.
- Preserve line endings per line (do not normalize). The status bar shows the dominant one, and "convert" is an explicit user action.
- Detect from BOM then a sample of the first ~64 KB (plus, optionally, the tail) with `chardetng` or `encoding_rs` and offer manual override. Display that detection is a guess.
- Use `encoding_rs` for streaming decode (it is stateful across chunk boundaries). Check [verify] that it fits the text view's needs with variable-width encodings (Shift-JIS, GBK), where seeking to a random offset can land mid-character and cannot always resync. Consider restricting random access seeking for legacy multi-byte encodings, or documenting it as best-effort.
- Treat CR-only (old Mac) and lone CR in CRLF-heavy files carefully. A CRLF split by a chunk boundary must not count as two line breaks.
- Test matrix: UTF-8/16LE/16BE with and without BOM, CRLF split across a chunk, invalid byte in the middle, NUL bytes in a "text" file.
**Phase:** Phase 1 (decode contract), Phase 5 (detection and switching UI).

### 10. Syntax highlighting stalls and state dependence
**What goes wrong:** Highlighting is stateful (multiline strings and comments), so the highlight at byte 10 GB depends on everything before it. Running tree-sitter or syntect on a 5 GB file hangs. Starting from an arbitrary offset gives wrong colors. Minified single-line files make a regex-based highlighter go catastrophic. `syntect` (onig or fancy-regex) is large and slow to load syntaxes (hurts startup and binary size). tree-sitter grammars each add hundreds of KB.
**Prevention:**
- Highlight only the visible window plus a small lookback, on a background thread, with a result cache keyed by checkpoint. Accept "approximate" color after a far jump, and say so.
- Disable highlighting by default above a size threshold (e.g. 50-100 MB) and above a line-length threshold. Make the threshold configurable.
- Never block the paint. Render unhighlighted text first and then update.
- Pick the highlighter with the binary budget in mind: a hand-rolled line-based lexer for a handful of languages, or `tree-sitter-highlight` with a curated, small grammar set, or syntect with pre-compiled dumps. Lazy init at first use, not at startup. Measure the size cost per grammar.
- Time-box each highlight job and cancel when the viewport moves.
**Phase:** Phase 6 (after core is stable). Do not let it block earlier phases.

### 11. GUI toolkit mismatch: custom virtualized text on egui/iced/Slint
**What goes wrong:** Immediate-mode (egui) re-lays-out text every frame unless carefully cached. Built-in text widgets (`TextEdit`) are built for small strings and are O(n) in the content. A virtualized editor means writing your own widget: cursor, selection, scrolling, clipboard, key repeat, drag-select, and so on. That is the bulk of the work and is routinely underestimated. Known weak points of the Rust GUI ecosystem (as of my knowledge, [verify against current versions]):
- **IME:** composition support (CJK, dead keys, Windows/macOS candidate window positioning) is partial or buggy in several toolkits. A custom widget must report the caret rectangle to the IME and handle preedit text. Test with Japanese/Chinese IMEs on all three OSes.
- **Text shaping and fallback:** egui's default text stack (ab_glyph/epaint) has no complex-script shaping or bidi, and weak font fallback. Arabic, Devanagari, Thai, and emoji ZWJ look broken. `cosmic-text` (used by iced and others) or `parley` are better, but heavier. CJK and emoji need system font fallback (fontdb, font-kit) and loading a 20 MB CJK font eagerly will blow startup and RAM.
- **Accessibility:** egui uses AccessKit, but custom-painted text editors expose little to screen readers unless explicitly implemented. Say honestly in scope what is supported.
- **High-DPI:** fractional scaling on Windows/Linux, moving windows between monitors with different DPI, and pixel-snapped glyph rendering look blurry if scale-factor events are mishandled. Monospace column alignment breaks with fractional glyph advances.
- **Hex view:** needs fixed-pitch column math independent of the proportional font fallback.
**Prevention:**
- Make the toolkit decision with a spike: a 100k-line virtualized monospace view with selection + IME + HiDPI on all three OSes before committing. Treat "custom text widget cost" as a major roadmap item.
- Isolate the editor core (document, viewport model, layout) from rendering in a pure-Rust crate with no GUI dependency, so the toolkit is swappable.
- Cache shaped lines (by line content hash and width). Shape only the visible rows.
- Lazy font fallback: load primary monospace at startup, load fallback fonts only when an uncovered glyph is hit.
**Phase:** Phase 0/1 (toolkit spike is a gate), Phase 2 (editor widget), IME and a11y in a dedicated hardening phase.

### 12. Idle CPU from continuous repaint
**What goes wrong:** Immediate-mode toolkits redraw on every event or on a timer. A caret blink, a progress spinner, or `request_repaint()` called every frame burns CPU and battery, and contradicts "low idle RAM/footprint". Background index and search threads call `request_repaint` too often (thousands per second), starving the UI.
**Prevention:**
- Repaint only on input or on explicit state change. The caret blink should use `request_repaint_after(500ms)` only when focused, and nothing when the window is unfocused or minimized.
- Throttle worker-to-UI notifications to ~30-60 Hz, coalesced.
- Measure: idle CPU should be ~0%, checked in CI or manually with the profiler on each OS. Treat >1% idle as a bug.
**Phase:** Phase 2, and a standing check in every phase.

### 13. Startup-time killers (200 ms cold target)
**What goes wrong:**
- GPU init: wgpu adapter and device selection (especially Vulkan on Linux, and shader compilation) can take 100-500 ms, and on bad drivers more. The wgpu backend also adds binary size (several MB) and RAM (tens of MB for driver state). glow (OpenGL) is typically faster to initialize and lighter but has driver quirks and is a legacy path on macOS (OpenGL is deprecated and capped at 4.1) [verify].
- Font loading: enumerating system fonts (fontdb scanning every font file) can take 100+ ms. Loading CJK or emoji fonts eagerly.
- Restoring the previous session synchronously (opening all tabs, rebuilding line indexes).
- Loading syntax definitions, themes, or the Markdown parser at startup.
- Antivirus real-time scanning on Windows slows first launch of an unsigned binary.
**Prevention:**
- Show a window immediately (hidden until the first frame is ready, but under a budget) and defer all else. Lazy-load fonts, syntaxes and the session. Restore only the active tab first.
- Benchmark cold start from the first commit, in CI per OS, with a budget gate. Compare glow and wgpu in the toolkit spike and measure time-to-first-frame and idle RSS.
- Bundle one small monospace font (subset) and skip system font enumeration until fallback is needed.
- Consider a software-rendering fallback for broken GPU drivers (e.g. tiny-skia, or softbuffer-based) to avoid crash-on-launch on VMs/RDP. [verify cost]
**Phase:** Phase 0 (toolkit spike), then a continuous budget.

### 14. Binary bloat (target < ~15 MB)
**What goes wrong:** Default release builds with wgpu, tree-sitter grammars, syntect dumps, regex Unicode tables, image decoders for the Markdown renderer and embedded fonts easily reach 20-40 MB.
**Prevention:**
- Set `[profile.release]` with `opt-level="s"` or `3` (measure both), `lto="fat"`, `codegen-units=1`, `panic="abort"`, `strip=true`. Track size per commit with `cargo-bloat` and `cargo-bloat --crates`, and fail CI on regressions.
- Audit features: `regex` with `unicode-*` features trimmed, `image` with only needed codecs, `default-features=false` everywhere.
- Fonts: subset the embedded font, and avoid embedding CJK/emoji fonts.
- UPX is not recommended (antivirus false positives, slower start).
- On Windows, statically link the CRT (`+crt-static`) to avoid the VC++ redistributable dependency, consistent with "no runtime required".
**Phase:** Phase 0 (set up measurement), continuous.

### 15. Markdown rendering of untrusted files
**What goes wrong:** Files are "downloaded files", so they are untrusted. Rendering can: fetch remote images (tracking pixels, IP leak, SSRF-like to local network, slow hangs), render raw HTML (script, iframes, if a webview is used), follow `file://` or relative links to arbitrary local files, open `javascript:`/custom-scheme links via the OS handler, or blow up on pathological input (deeply nested lists/blockquotes causing stack overflow, huge tables, ReDoS in extensions). A multi-GB `.md` is rendered whole.
**Prevention:**
- No webview. Render natively from a `pulldown-cmark` event stream (or comrak, which is heavier) to the toolkit's primitives. Raw HTML blocks are shown as literal text, or ignored. Never executed.
- Remote images: off by default, show a placeholder with "load image" click-to-allow per file. Local images only from the same directory tree, after canonicalizing the path (no `..` escape), with a size cap, decoded off-thread, with an image-format allowlist and decode-size limit (decompression-bomb guard).
- Links: allow only `http(s)` and `mailto`, and show the target URL before opening via the OS handler (use `open`/`opener` with an explicit scheme check). Block `file:`, `javascript:` and unknown schemes. Confirm on click.
- Rendering limits: refuse or truncate rendering above a size (e.g. 5-10 MB), render only on toggle, do the parse off the UI thread, cap nesting depth. pulldown-cmark is iterative for blocks but check recursion in your own renderer.
- Make "rendered" read-only to avoid the edit-position-mapping problem.
**Phase:** Phase 7 (Markdown). Security decisions should be written into the requirements before implementation.

### 16. macOS signing, notarization and distribution
**What goes wrong:** An unsigned or un-notarized app downloaded from the web is blocked by Gatekeeper ("damaged and can't be opened"). The "single portable binary" idea collides with macOS, which expects a `.app` bundle with Info.plist, and file associations and "Open With" require the bundle's `CFBundleDocumentTypes`. Notarization needs an Apple Developer account ($99/year), the hardened runtime, and the right entitlements. Rust GUI apps may need JIT-less settings but typically OK. Universal binary (arm64 + x86_64) needs `lipo`. Stapling must be done after notarization.
**Prevention:**
- Decide early: distribute as a signed, notarized `.app` in a zip/dmg on macOS (a bare binary is not viable for double-click use). The "single binary" requirement is per-OS adapted. Update PROJECT.md accordingly.
- Set up CI signing with `cargo-bundle` or `cargo-packager`, `codesign --options runtime`, `notarytool`, and `stapler`. Do this in an early "packaging spike" phase, not at the end. Secrets handling in CI.
- Windows: SmartScreen reputation warns on unsigned binaries. Authenticode signing (cert cost) reduces this. Plan for it, or document the warning.
**Phase:** Packaging phase early (spike) and final release phase.

### 17. File associations, single-instance and CLI per OS
**What goes wrong:**
- **Windows:** "default app" cannot be set programmatically since Windows 8; the user must choose in Settings. A portable exe can register ProgID keys in HKCU (`Software\Classes`) but that is a side effect on a "no installer" app, and needs an explicit opt-in. Command-line quoting and paths over MAX_PATH (use long path prefix `\\?\`) and UNC paths need handling.
- **macOS:** Opened files arrive as Apple Events (`application:openFiles:`), not argv. A winit-based app must handle this event. Double-clicking in Finder with the app already running delivers it to the running instance. Missing this makes "Open With" silently do nothing. [verify winit support in the chosen version]
- **Linux:** needs a `.desktop` file with `MimeType=` and `%F`/`%U`, installed in `~/.local/share/applications` and `update-desktop-database`. A raw binary cannot self-register without writing these files. Wayland drag-and-drop and app_id (for icon/taskbar matching) differ from X11.
- **Single-instance:** the second launch must forward its paths to the running instance. Use a named pipe (Windows), Unix domain socket (Linux/macOS) with a lock file, or the `single-instance`/`interprocess` crate. Pitfalls: stale socket after a crash, race when two instances start at the same time, security (another user connecting to the socket, so use per-user paths and permissions), and startup latency from the forwarding handshake. Without it, every "Open With" spawns a new full process, which hurts the "instant" feel.
- Dropped files: handle many files, folders, and non-UTF-8 paths (use `OsString`/`PathBuf`, never `String`, for paths).
**Prevention:** Write per-OS integration as a dedicated phase with a manual test checklist. Keep paths as `PathBuf`/`OsString` end-to-end.
**Phase:** Phase 8 (OS integration), single-instance design before session restore.

## Moderate Pitfalls

### Hex view on edited and huge files
**What goes wrong:** Overwrite-only hex edits must coexist with text-mode piece-table edits in the same tab. If both exist there are two edit models. Switching views mid-edit shows inconsistent data. Row offsets beyond 4 GB overflow `u32`. Scrollbar precision: an f32 scroll position cannot address 30 GB at byte precision (f32 has 24 bits of mantissa). Use u64 plus a fractional position.
**Prevention:** One document model, with the hex view as another renderer over the same piece table. Overwrite = replace one byte with a one-byte piece. Use `u64` offsets everywhere. The scrollbar maps through u64/f64 and snaps to rows. Test at offsets > 4 GB and > 2^53 is irrelevant but > 2^24 rows is relevant.
**Phase:** Phase 5.

### Scrollbar and jump-to-line on huge files
**What goes wrong:** Scroll thumb dragging maps proportionally to bytes and lands mid-line. Mouse wheel scrolling by "lines" when line lengths vary wildly causes jitter. Thumb size becomes sub-pixel.
**Prevention:** Position = byte offset snapped to line start. Minimum thumb size. Smooth scroll by row within the current anchored line. Fixed anchor + relative scrolling avoids needing global line numbers.
**Phase:** Phase 2.

### Page-cache thrash and read-ahead
**What goes wrong:** Random access over 30 GB on HDD or network is slow. A full-file scan (search, index) evicts the OS cache and makes the rest of the system sluggish. Mmap page-fault stalls occur on the UI thread, which causes frozen frames.
**Prevention:** Never touch uncached data on the UI thread. Use a prefetch thread for the visible ±N pages. Use `madvise(SEQUENTIAL/RANDOM)` or `posix_fadvise`, and `FILE_FLAG_SEQUENTIAL_SCAN`, appropriately. Throttle background scans.
**Phase:** Phase 2.

### External modification and live-tail
**What goes wrong:** Logs grow while open. Without a watcher, the view silently shows stale data. With a naive watcher (`notify`), network FS events are missing, and the event storm from a hot log floods the UI. The user's edits conflict with external changes.
**Prevention:** Check size/mtime on window focus and on a slow poll. Offer "reload" and optionally a "follow" mode as a later feature. Never auto-reload a dirty tab.
**Phase:** Phase 3.

### Large clipboard and drag operations
**What goes wrong:** Copying a 5 GB selection allocates 5 GB (and the OS clipboard often copies again). Select-all on a huge file does the same.
**Prevention:** Cap the copy size with a prompt. Represent selection as a range, never as a string. Paste of huge clipboard content should stream to the add-buffer in chunks.
**Phase:** Phase 3.

### Crash recovery and session restore
**What goes wrong:** Dirty edits on a 30 GB document exist only as the piece table and add-buffer in RAM. A crash loses them. A restored session refers to files that moved or changed.
**Prevention:** Persist the add-buffer and piece list to a recovery journal in the app data dir (periodically, cheap because edits are small). Validate size/mtime/hash-of-prefix on restore. Never block startup on restore.
**Phase:** Phase 8.

### Binary / NUL-heavy files opened as text
**What goes wrong:** Because the user wants text-first for binaries, control characters, NULs and enormous "lines" (no newlines in a binary) hit the long-line path. Rendering NUL or bidi override characters can corrupt the display or spoof text (Trojan Source).
**Prevention:** Render control chars and NUL as visible glyphs (e.g. `␀`, `·`). Show bidi controls explicitly. Treat a binary with no newlines through the long-line path (pitfall 4). Make the hex toggle prominent and quick.
**Phase:** Phase 2.

### Cross-platform path and filesystem quirks
**What goes wrong:** Case-insensitive duplicate tabs (same file by two spellings), symlinks, Windows 8.3 names, UNC, non-UTF-8 Unix names, 2 GB/4 GB limits on FAT32 USB sticks, sparse files and device files (`/dev/sda`, `\\.\PhysicalDrive0`) whose `metadata().len()` is 0 or wrong. Opening a FIFO or `/proc` file hangs forever.
**Prevention:** Canonicalize for tab identity. Check file type before mmap (regular files only; special files get a clear message or a streaming read path). Handle `len()==0` files that actually have content.
**Phase:** Phase 1 and Phase 8.

## Minor Pitfalls

### Rust-specific
- `usize` is 32-bit on 32-bit targets. Use `u64` for offsets, and cast deliberately.
- `unsafe` mmap code spread across the codebase. Contain it in one small module with documented invariants. Run Miri/loom where practical and fuzz.
- Panics in worker threads silently kill indexing. Propagate errors to the UI. Use `panic=abort` with a crash handler that tries to write the recovery journal first (note: abort skips unwinding, so install a panic hook that dumps first).
- Threading: sharing the mmap and the piece table across UI and workers needs immutable snapshots (Arc'd piece tree) so a search never sees a half-edited document.
- Debug builds are unusably slow on large-file code. Use `opt-level=3` for dependencies in the dev profile.

### Testing blind spots
- Tests only on small files. Generate sparse multi-GB test files cheaply (`truncate`, sparse files, and a generator for 20 GB logs) and keep a perf regression suite. Note CI disk limits.
- No tests for non-ASCII paths and for Windows/macOS-specific behavior.

### UX
- No progress or cancel for long operations (index, search, save) feels like a hang. Always show status-bar progress.
- Undo of save-time encoding or line-ending conversion is expected by users.
- Drag-and-drop of the file onto the executable (argv) vs onto the window (event) are separate code paths.

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Toolkit spike (Phase 0) | Picking a toolkit by hello-world; custom text widget, IME, shaping and idle-CPU costs discovered late | Gate: virtualized 100k-line view, IME, HiDPI, cold-start and idle-CPU numbers on all 3 OSes; glow vs wgpu measured |
| Storage/document model (Phase 1) | mmap-only I/O; offset type confusion; O(n) piece list; save design deferred | `ByteSource` trait, u64 offsets, tree-based piece table, oracle fuzzing, design save/atomic replace now |
| Viewport and navigation (Phase 2) | Sync line indexing; long lines; stale scrollbar; UI-thread page faults | Async sparse index, byte-based scrolling, bounded line scans, prefetch thread |
| Editing, undo, save (Phase 3) | In-place writes, ENOSPC, lost permissions, Windows locked file, undo RAM | Temp + atomic replace, free-space check, metadata copy, delta undo, external-change detection |
| Search/replace (Phase 4) | Chunk-boundary misses, regex buffering, UTF-16 search, UI stalls | Overlapped windows, `regex::bytes`, cancellable background streaming, boundary fuzz tests |
| Encoding and hex (Phase 5) | Lossy decode corrupting bytes; line-ending normalization; two edit models; >4 GB offsets | Bytes are truth, decode is a view, one document model, u64 everywhere |
| Syntax highlighting (Phase 6) | Stalls, stateful ambiguity, binary size and startup cost | Visible-window only, size/line-length thresholds, lazy load, size budget per grammar |
| Markdown (Phase 7) | Remote image fetch, raw HTML, dangerous links, huge-file render | Native render, no webview, images opt-in, scheme allowlist, size cap |
| OS integration/packaging (Phase 8) | macOS bundle/notarization, Windows association limits, Linux .desktop, single-instance races, AV false positives | Early packaging spike with CI signing, per-OS checklist, per-user IPC with stale-socket handling |
| Footprint (continuous) | Binary > 15 MB, cold start > 200 ms, idle CPU | CI budgets for size, startup and idle CPU; `cargo-bloat`; lazy init |

## Roadmap Flags (needs deeper research)
- Toolkit choice and custom text widget (IME, shaping, a11y): HIGH research need. Spike required.
- SIGBUS guard viability vs pread fallback: needs a spike.
- Legacy multi-byte encoding random access (Shift-JIS/GBK): needs a decision on supported behavior.
- macOS Apple Events file opening in the chosen windowing layer: verify.
- Highlighter choice under the size budget: measure.
- Standard patterns, low research need: Markdown via pulldown-cmark, memchr-based scanning, atomic-save pattern.

## Sources
- memmap2 crate documentation (unsafe contract, file-modification hazards) - training knowledge, MEDIUM
- POSIX/Win32 documentation for mmap semantics, `ReplaceFileW`, SIGBUS on truncation - training knowledge, MEDIUM
- Piece table / rope literature and prior art (VS Code piece tree, xi-editor, Helix/ropey) - training knowledge, MEDIUM
- `regex`, `regex-automata`, `memchr`, `encoding_rs`, `unicode-segmentation`, `pulldown-cmark` crate behavior - training knowledge, MEDIUM
- Apple Developer docs on notarization and Gatekeeper; Microsoft docs on default-app changes since Windows 8 - training knowledge, MEDIUM
- Rust GUI toolkit IME/shaping/accessibility status (egui, iced, Slint, cosmic-text, AccessKit) - training knowledge, LOW; must be re-verified against current releases during the toolkit spike
- No live web or Context7 verification was performed for this file.
