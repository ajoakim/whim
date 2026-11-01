# Phase 2: Core Storage & Read-Only Viewers - Research

**Researched:** 2026-10-31
**Domain:** Rust/egui 0.37 large-file viewer: positional-read storage, sparse line index, wrapped virtual text view, hex view, rfd dialogs, CJK fallback, CI gating
**Confidence:** HIGH on storage/APIs (read from crate sources and std docs), MEDIUM on throughput/memory numbers (not measured at 20 GB), MEDIUM on macOS rfd threading and Wayland DnD behaviour (not testable on this machine)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- D-01: Content access via positional reads (`read_at`/`seek_read`) into a small chunk cache, NOT direct `&[u8]` over mmap. Viewport reads a few KB; the indexer streams 1-4 MB blocks. A truncated/changed file surfaces as a recoverable "file changed on disk" error, never SIGBUS/UB. Resolves Phase 1 review WR-01. (memmap2 may stay only where it is provably safe, or be removed.)
- D-02: Read-only `TextSource` abstraction now (len, read_range, version); the piece table arrives in Phase 3 behind the same interface.
- D-03: Background, cancellable line indexer: one checkpoint (byte offset + line number) per 64 KB block, memchr-based. Until complete, line numbers are estimated from the average line length sampled so far and shown with a `~` marker; they become exact when indexing finishes.
- D-04: Lines longer than 10,000 chars are hard-broken into display rows; a multi-GB single line scrolls by rows and is never laid out whole. This replaces Phase 1's fixed 128-byte rows (fixes review WR-02 End key and WR-04 non-inverse scroll).
- D-05: Word wrap ON by default (wrap to window width), auto-disabled for files > 64 MB with a status-bar notice; user can re-enable. Toggle: View > Word wrap, Alt+Z.
- D-06: Line numbers ON by default, right-aligned gutter; estimated numbers rendered dimmed. Toggle: View > Line numbers.
- D-07: CJK fallback via lazily loaded system fonts (Yu Gothic / PingFang / Noto CJK) on first missing glyph; no bundled CJK font (size budget).
- D-08: Single document at a time in Phase 2; File > Open / drop replaces the current file. Tab strip shows the one open file. Real tabs in Phase 5.
- D-09: Hex layout 16 bytes/row: 64-bit hex offset, two groups of 8 hex bytes, ASCII column with `·` for non-printables. View > Hex view / Ctrl+H; switching keeps the same byte position.
- D-10: Binary heuristic: NUL byte or >30% control bytes in the first 8 KB -> open as text with a dismissible banner "This looks like a binary file - [View as hex]". Never auto-switch.
- D-11: Unified Ctrl+G box: `1234` = line, `0x1F00` or `#8192` = byte offset, `50%` = percent; in hex view a bare number is an offset. Jump to line before indexing completes uses the estimate and self-corrects.
- D-12: Long operations show progress in the status bar with cancel; Esc also cancels. Opening a new file cancels the previous file's indexing.
- D-13: Selection: mouse drag, Shift+arrows/click, double-click word, triple-click line, Ctrl+A; Ctrl+C copies. Hex view selects bytes and copies as hex text.
- D-14: Copy caps: <=16 MB copies immediately; above that a confirm dialog with size; hard cap 256 MB. Selection stored as offsets, so Ctrl+A on 20 GB is free.
- D-15: File > Open via `rfd` (Linux via xdg-portal, no GTK link); drag & drop takes the first file (multi-file waits for tabs); last directory remembered for the session.
- D-16: Status bar: `Ln 1,234 (~)` · `Col 5` · byte offset · `UTF-8` · `LF`/`CRLF` (detected, display-only) · indexing % with cancel. Startup ms / backend shown only in `--bench` mode.
- D-17: Toolkit egui/eframe 0.37 + glow; footprint gates: stripped size <= 10 MB, idle footprint (settled, per-OS comparable metric) <= 70 MB, startup <= 300 ms warm. Re-measure with `cargo xtask measure` and append to docs/measurements.md.
- D-18: Phase 2 setup task: hard-gate macOS CI (real GL) at idle footprint <= 70 MB and startup median <= 600 ms; software-GL runners stay report-only; size hard everywhere.
- D-19: File::open on a FIFO/device must not block the UI thread: open/stat off the UI thread or reject non-regular files with a clear message.
- D-20: With word wrap OFF, the text view scrolls horizontally: horizontal scrollbar, Shift+wheel and Left/Right past the edge scroll sideways; width bounded by the 10k-char row cap.
- D-21: Pull the minimal macOS `.app` bundle forward from Phase 8 (user report 2026-11-01: double-clicking the bare binary in Finder opens Terminal.app first). Add `cargo xtask bundle-macos` producing an unsigned `Whim.app` (Contents/Info.plist with CFBundleExecutable/Identifier/Name/Version, NSHighResolutionCapable=true, LSMinimumSystemVersion; Contents/MacOS/whim; icon optional). CI macOS jobs upload `Whim.app` (zipped, preserving the exec bit) as an artifact. Document in README: download, unzip, right-click > Open the first time. Phase 8 keeps file associations / CFBundleDocumentTypes / Apple Event open.

### Claude's Discretion
Module layout, chunk-cache size/eviction, exact estimation formula, keyboard shortcuts not listed above, banner/dialog styling within UI-SPEC.

### Deferred Ideas (OUT OF SCOPE)
- Multi-file drop / tabs -> Phase 5 (OPEN-01).
- Encoding / EOL switching -> Phase 5.
- Cold start, mixed-DPI drag, macOS/Linux IME manual checks -> open in 01-HUMAN-UAT.md.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| CORE-01 | First page of tens-of-GB file < 1 s | Positional read of first 64 KB block + 8 KB heuristic; open/stat on worker (Pattern 1, Pitfall 3) |
| CORE-02 | Never read whole file; RAM independent of size | Bounded chunk cache (1 MiB), sparse index 128 KiB/GiB, paragraph-window layout |
| CORE-03 | Smooth scroll before index done | Byte-offset scroll anchor, stateless paragraph boundaries (Pattern 3) |
| CORE-04 | Index progress; estimated then exact numbers | Block-cumulative index + estimator (Pattern 2) |
| CORE-05 | Jump to line / byte offset | Line<->byte mapping incl. partial-index estimate (Pattern 2), Ctrl+G grammar |
| CORE-06 | Multi-GB single line without freeze | Absolute-aligned hard-break rule bounds paragraphs < 2*CAP bytes (Pattern 3) |
| CORE-07 | Long ops show progress, cancellable | Worker + `AtomicBool` + mpsc + throttled `request_repaint` (Pattern 4) |
| EDIT-05 | Toggle word wrap, auto-off on huge files | `LayoutJob.wrap.max_width` vs `f32::INFINITY`; threshold 64 MB |
| EDIT-06 | Toggle line numbers | Gutter from line index + estimator |
| EDIT-10 | Select + copy in read-only viewer, size cap | Offset-based selection, `Galley::cursor_from_pos`, streamed worker copy (Pattern 6) |
| HEX-01 | Hex view of any file | Row = offset/16, virtual rows exact (no wrapping), Pattern 7 |
| HEX-03 | Jump to offset in hex | Ctrl+G grammar; row = offset >> 4 |
| HEX-05 | Binary banner, never auto-switch | Heuristic over first 8 KB, Pattern 7 |
| UI-01 | Minimal UI: tab strip, view, status bar | UI-SPEC layout; panels as in existing `app.rs` |
| UI-05 | CJK via font fallback | egui 0.37 `FontProvider` + `egui_system_font_provider`, installed lazily (Pattern 8) |
| OPEN-02 | Drag & drop open | `raw.dropped_files[i].path()`; never `.bytes()` (Pattern 9) |
| OPEN-09 | File > Open native dialog | `rfd` 0.17.2 on a worker thread (Pattern 5) |
</phase_requirements>

## Summary

Everything the phase needs is available with a small dependency delta, and one hidden trap was found: the repo's `eframe` dependency does **not** enable the `clipboard` feature, so `ctx.copy_text` currently only stores the string in an in-process fallback and never reaches the OS clipboard (verified: `cargo tree -i arboard` prints nothing; egui-winit `Clipboard::set_text` falls through to `self.clipboard = text` without arboard). Phase 2 must add `eframe` feature `"clipboard"`. A throwaway build in `/tmp/size-spike` (copy of the repo plus `rfd` with `xdg-portal`, `egui_system_font_provider`, `memchr`, eframe `clipboard`, all code-referenced so nothing is dead-stripped) measured **5,674,496 B (5.41 MiB) vs the 5,253,632 B (5.01 MiB) baseline: +0.40 MiB on Windows**, so the 10 MB size gate has ~4.5 MB headroom. Linux and macOS deltas were not measured (CI will).

The storage design is straightforward: replace `memmap2` entirely with a `FileSource` doing positional reads (`FileExt::read_at` on Unix, `seek_read` on Windows, wrapped in one `read_full_at` helper that loops on short reads and maps EOF to a typed `Changed` error), a 16-block (1 MiB) LRU of 64 KiB blocks, and a separate file handle per worker thread. The line index stores one `u64` cumulative-newline count per 64 KiB block (the block's byte offset is implicit): 128 KiB per GiB, 2.5 MiB for 20 GB. The single most important design call is the **scroll anchor**: store a plain absolute byte offset, and derive "paragraph" (display-line segment) boundaries with a *stateless, absolute-aligned* rule (newline OR a hard break at multiples of CAP bytes inside newline-free windows). That rule makes forward and backward row stepping exact inverses (fixes WR-04), needs no line index or line start (so scrolling works before indexing finishes and on multi-GB single lines), bounds every layout job to < 2*CAP bytes, and survives window resize, wrap toggle and hex<->text switching because the anchor is just a byte offset.

egui 0.37 changed several APIs that earlier research (01-RESEARCH) could not know about and that make D-07 much cheaper than expected: epaint has a built-in `FontProvider` trait (discovery on first missing glyph, called once per (family, char)), and the official `egui_system_font_provider` crate uses `fontique` to find OS fonts and memory-maps them (not copied). `DroppedFile` is now a handle with `path()`; its `bytes()` would read the whole file and must never be called. winit 0.30.13 emits no drag-and-drop events on Wayland, so DnD is X11/Windows/macOS only (Wayland users use File > Open or the CLI).

**Primary recommendation:** Build bottom-up in pure, egui-free modules (`source`, `lineindex`, `paragraph`/`scroll`, `hexrows`) with colocated unit and property tests; then the painters; then the app shell. Remove `memmap2`, add `memchr`, `rfd` (default-features off, `xdg-portal` only), `egui_system_font_provider` (default-features off) and eframe `clipboard`; make `eframe`'s `system_font_fallback: false` and install the font provider lazily.

## Architectural Responsibility Map

(This is a desktop app; "tiers" are process-internal layers.)

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| File open / stat / type check | Worker thread | UI (receives result) | `open` of FIFO/device can block (D-19); never on UI thread |
| Positional reads + chunk cache | Storage (`FileSource`) | — | One place owns I/O errors and "changed on disk" |
| Line index build | Worker thread (own file handle) | UI drains messages | CPU/IO bound; cancellable |
| Line<->byte mapping, estimate | Storage/index module (pure) | UI calls it | Needs cache reads (<= 64 KiB) per query, fine on UI thread |
| Paragraph boundaries, scroll anchor math | Pure `scroll` module | — | Testable without egui; the core correctness piece |
| Text layout (wrap, galleys) | UI thread (egui) | — | `Fonts` is UI-thread; layout only visible paragraphs |
| Selection model, hit test | UI thread | — | Offsets in state; hit test via `Galley::cursor_from_pos` |
| Copy of large selection | Worker thread | UI issues `ctx.copy_text` | Read off-thread; clipboard command must go through egui on UI thread |
| Native file dialog | Worker thread (rfd) | UI via mpsc | Must not block repaint |
| Font fallback | egui `FontProvider` | OS fonts (mmap) | Lazy, discovered per missing char |
| CI gating | GitHub Actions + `cargo xtask measure` | — | D-18 |

## Standard Stack

### Core (changes to Cargo.toml)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| eframe/egui | 0.37.0 (already) | Shell/painter | Locked D-17. **Add feature `"clipboard"`** (arboard + smithay on Linux) [VERIFIED: Cargo.lock lacks arboard; egui-winit-0.37.0/src/clipboard.rs] |
| memchr | 2.8.3 (already in lockfile transitively; verified in registry) | Newline count/scan, memrchr | `memchr::memchr_iter(b'\n', buf).count()` is SIMD-specialised (`Memchr::count` in memchr.rs:327) [VERIFIED: registry source] |
| rfd | 0.17.2, `default-features = false, features = ["xdg-portal"]` | Open dialog | Features: `default = [xdg-portal, wayland]`, `gtk3` optional, `common-controls-v6` optional [VERIFIED: rfd-0.17.2 Cargo.toml]. Linux deps with xdg-portal only: libc, log, percent-encoding, pollster, raw-window-handle; libdbus is `dlopen`ed at runtime (`libdbus-1.so.3`), no link-time GTK/dbus [VERIFIED: `cargo +1.99.0 tree`, src/backend/xdg_desktop_portal/portal/ffi.rs:199]. macOS: objc2, objc2-app-kit, block2, dispatch2 (already largely in eframe's tree). Windows: windows-sys (already a dep). |
| egui_system_font_provider | 0.37.0, `default-features = false` | CJK/system font fallback | Official, versioned with egui; uses `fontique` (`system`, `fontconfig-dlopen`), fonts are memory-mapped not copied, enumeration on a background thread [VERIFIED: crate source lib.rs docs] |

### Remove
| Library | Reason |
|---------|--------|
| memmap2 | D-01. Delete `src/file.rs` mmap path. It also removes the "locked file / SAVE-05" Windows mmap hazard for Phase 4. (fontique pulls its own memmap2 for fonts only, which is safe: system font files.) |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| std only | — | Threads (`std::thread`), `mpsc`, `AtomicBool`, `Mutex` | Per project guidance use std mpsc; no crossbeam, no tokio |
| (none new) | — | Test PRNG: tiny xorshift in test modules | Avoid adding `proptest`; keeps lockfile small |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `egui_system_font_provider` | `fontdb`/`font-kit` + hand-written `FontProvider` | More code, own locale/script fallback logic; official crate already does per-script, per-locale lookup. Use the official one. |
| rfd sync dialog on a thread | `AsyncFileDialog` | Async needs an executor; thread + sync `FileDialog::pick_file()` is the least code (see Pattern 5) |
| `Galley` selection painting | own per-glyph x math | `Galley` already gives rows + `pos_from_cursor`; reuse |

**Installation (Cargo.toml delta):**
```toml
eframe = { version = "0.37.0", default-features = false, features = ["default_fonts", "x11", "wayland", "clipboard"] }
memchr = "2.8"
rfd = { version = "0.17.2", default-features = false, features = ["xdg-portal"] }
egui_system_font_provider = { version = "0.37.0", default-features = false }
# remove: memmap2
```
Edition-2024/rust-version: the new crates require Rust 1.95 (matches the pin 1.99.0). NOTE: running `cargo` from outside the repo picks up a different (older) toolchain on this machine; the repo's `rust-toolchain.toml` pins 1.99.0, and the spike required `cargo +1.99.0` outside the repo.

**Version verification:** versions above were read from the registry (`rfd-0.17.2`, `egui_system_font_provider-0.37.0`, `memchr-2.8.3`) on 2026-10-31 [VERIFIED: ~/.cargo/registry and crates.io static downloads].

**Measured size impact (Windows, release, fat LTO, strip):** baseline 5,253,632 B; with rfd + system font provider + memchr: 5,612,032 B (+0.34 MiB); plus eframe `clipboard`: 5,674,496 B (+0.40 MiB total, 5.41 MiB) [VERIFIED: /tmp/size-spike build, 2026-10-31]. Linux/macOS: [ASSUMED] similar order (< 1 MiB); the CI `check-size` gate will confirm.

## Architecture Patterns

### System Architecture Diagram

```
 CLI path / File>Open (rfd thread) / drop (raw.dropped_files[].path())
                         |
                         v
              OpenRequest(path)  -- UI cancels previous generation (AtomicBool)
                         |
                         v
   [open worker] metadata(): reject dir/FIFO/device (stat never blocks)
        File::open -> (len, mtime) identity -> read first 64 KiB
                         |
            Opened{source, head sample, binary?, eol, enc} (mpsc + request_repaint)
                         v
 +-------------------- UI thread (egui frame) ----------------------+
 |  App state: Doc{ source: Arc<FileSource>, index: LineIndex,      |
 |    anchor: u64 (byte), sub_px, hscroll, selection(anchor,head),  |
 |    mode: Text|Hex, wrap, gutter, pending_jump }                  |
 |                                                                  |
 |  each frame:                                                     |
 |   drain mpsc (IndexBatch / Done / CopyDone / OpenResult / Dialog)|
 |   input -> anchor moves (rows) / jumps / selection              |
 |   TEXT: paragraphs(anchor, viewport) -> read_range (chunk cache) |
 |         -> decode(+tab/ctrl map) -> LayoutJob(wrap) -> Galley    |
 |         -> paint gutter, selection rects, glyphs, scrollbars     |
 |   HEX : rows = anchor>>4 .. ; read 16*n bytes ; paint columns    |
 +--------+------------------------------------+--------------------+
          | read_range(start,buf)              ^ mpsc batches (<=10/s)
          v                                    |
   FileSource (Mutex<File> UI handle,     [index worker: own File handle]
   LRU 16x64KiB blocks, Changed detect)    4 MiB reads -> per-64KiB
          ^                                 memchr count -> cum[] batch
          |                                 checks AtomicBool each read
   [copy worker: own handle] streams selection -> Vec<u8> -> String
          \--> CopyDone(String) --> UI: ctx.copy_text(string)
```

### Recommended Project Structure
```
src/
├── source.rs        # TextSource trait, FileSource, read_full_at, ChunkCache, SourceError
├── open.rs          # open worker: classify path, first-block sample, binary/EOL heuristics
├── lineindex.rs     # LineIndex (cum[]), IndexWorker, estimate, line<->offset
├── scroll.rs        # CAP rule: segment_start/next_boundary, row stepping (pure, no egui)
├── text.rs          # decode_paragraph(): bytes -> display String + byte_of_char map (tabs, ctrl, invalid)
├── view/
│   ├── mod.rs       # shared: scrollbar, focus/IME hookup, snapping (existing snap.rs)
│   ├── textview.rs  # gutter, wrapped galleys, selection, hscroll
│   └── hexview.rs   # 16-byte rows, hit test, selection
├── selection.rs     # Selection{anchor,head}, word/line bounds, size checks
├── copy.rs          # copy worker, caps, confirm state
├── dialogs.rs       # rfd thread wrapper, last_dir
├── fonts.rs         # lazy FontProvider install
├── app.rs           # shell: menus, tab strip, banner, goto, status bar, DnD overlay
└── (existing) args.rs backend.rs bench.rs ime.rs theme.rs
```
Replace `src/file.rs` and `src/view/rows.rs` (keep `format_offset`/`format_size` helpers where still useful).

### Pattern 1: Positional reads with typed change detection (D-01, D-19)

**Semantics (verified against std docs):**
- Unix `FileExt::read_at`: offset independent of cursor, "current file cursor is not affected"; short reads are not errors; `read_exact_at` returns `UnexpectedEof` if EOF before the buffer is full [CITED: doc.rust-lang.org/std/os/unix/fs/trait.FileExt.html].
- Windows `FileExt::seek_read`: offset is independent of the current cursor but "the current cursor **is** affected ... set to the end of the read"; short reads possible; reading past EOF returns 0; **the docs say nothing about thread safety** [CITED: doc.rust-lang.org/std/os/windows/fs/trait.FileExt.html].
- Therefore: never share one handle between the UI thread and workers. Each thread owns its own `File` (opened by path, then verified against the recorded `(len, modified)`). Within one thread's handle there is a single reader so the cursor side effect is irrelevant. (Sharing is "probably fine" on Windows since each call passes the offset, but the docs do not promise it, so don't rely on it.)
- Unix `try_clone`/dup shares nothing problematic for `pread`, but on Windows `try_clone` shares the cursor; open by path instead.

```rust
// source.rs - one helper hides the OS split
#[cfg(unix)]   fn read_at(f: &File, buf: &mut [u8], off: u64) -> io::Result<usize> { std::os::unix::fs::FileExt::read_at(f, buf, off) }
#[cfg(windows)]fn read_at(f: &File, buf: &mut [u8], off: u64) -> io::Result<usize> { std::os::windows::fs::FileExt::seek_read(f, buf, off) }

/// Fill `buf` or report how far we got. Short read before `want_end` => file shrank.
pub fn read_full_at(f: &File, mut buf: &mut [u8], mut off: u64) -> Result<(), SourceError> {
    while !buf.is_empty() {
        match read_at(f, buf, off) {
            Ok(0) => return Err(SourceError::Changed),            // EOF inside known length
            Ok(n) => { buf = &mut buf[n..]; off += n as u64; }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(SourceError::Io(e)),
        }
    }
    Ok(())
}
```
- `read_range(start, buf)` must clamp to `len` recorded at open; any `Ok(0)` inside it, or a metadata `len`/`modified` mismatch checked on each cache miss (one `File::metadata()` is ~microseconds), yields `SourceError::Changed`. The UI keeps painting the last good rows dimmed (UI-SPEC) and offers Reload. Growth of the file is also reported as Changed in Phase 2 (tail mode is Phase 6).
- Rust std opens files on Windows with `FILE_SHARE_READ|WRITE|DELETE`, so other programs can truncate/replace the file while we hold it; positional reads then return short/0 or an error, which we map to `Changed`. No access violations are possible because no memory is mapped [ASSUMED from std behaviour; consistent with the Phase 1 review WR-01 motivation].
- Optional on the indexer handle (Windows): `OpenOptionsExt::custom_flags(FILE_FLAG_SEQUENTIAL_SCAN)` (0x0800_0000) to hint read-ahead; the repo already depends on `windows-sys`, which exports the constant. Skip on Unix (would need `libc::posix_fadvise`; libc is a transitive dep, add only if measured useful).

**Chunk cache:** `Mutex<Cache>`/or plain `RefCell` since only the UI thread reads through it: `Vec<(block_idx, Box<[u8; 64KiB]>)>` of 16 entries (1 MiB total), true LRU by a counter. `read_range` copies out of up to N blocks (a viewport window is a few KB; a paragraph < 2*CAP = 20 KB spans at most 2 blocks). Workers (index, copy) do not use the cache; they read with large buffers (4 MiB index, 1-4 MiB copy) on their own handle.

**Open without blocking (D-19):**
1. Do **all** of `metadata()` + `File::open` + first read on a worker (`std::thread::spawn`); the UI shows the previous file/empty state and an "Opening <name>..." status message.
2. `std::fs::metadata(path)` does a `stat`, which does not block on a FIFO (only `open(O_RDONLY)` blocks until a writer appears). Reject anything where `!file_type().is_file()` (dir, FIFO, socket, char/block device, Windows named pipe) with the UI-SPEC message "Couldn't open {name}: it is not a regular file (folder, pipe or device)." *before* opening. Opening on a worker anyway covers the stat->open TOCTOU race and slow network paths; a stuck worker can simply be abandoned (generation id ignored) without freezing the UI.
3. Windows reserved names (`CON`, `NUL`, ...): `metadata` fails or reports non-file; same rejection path. `/proc`-style regular files that report `len == 0` show as empty (acceptable limitation, document).
4. `main.rs` currently opens the CLI file synchronously before the window: move this onto the same open worker so cold start does not depend on file latency (CORE-01, PERF-01).

**Can memmap2 be removed? Yes.** Nothing else in Phase 2 needs it; removal also shrinks the lockfile surface and eliminates the SAVE-05/SAVE-07 hazards for later phases. Phase 1 measurements showed mmap did not inflate the working set, but it makes Windows/macOS counters rise as pages are touched; with positional reads the OS cache is not attributed to the process working set [ASSUMED for macOS `phys_footprint`; verified only for the Windows mmap-no-effect case in docs/measurements.md].

### Pattern 2: Sparse line index with partial-index estimation (D-03, D-11)

**Structure** (block size `B = 65_536`):
```rust
pub struct LineIndex {
    /// cum[i] = number of '\n' in bytes [0, i*B).  cum.len() = blocks_done + 1, cum[0] = 0.
    cum: Vec<u64>,
    /// bytes covered so far (== min(len, cum.len()-1 blocks * B), or len when complete)
    covered: u64,
    complete: bool,
    total_newlines: u64,   // valid when complete
    cr_total: u64,         // for EOL label: 0 => LF, == total_newlines => CRLF, else Mixed
}
```
- Memory: 8 B per 64 KiB = 128 KiB per GiB = 2.5 MiB for 20 GiB (16,384 blocks per GiB) [VERIFIED: arithmetic]. The byte offset of a checkpoint is implicit (`i*B`), which satisfies "byte offset + line number per 64 KB block" (D-03) at half the storage.
- Worker loop: read 4 MiB into a reusable buffer with `read_full_at` (last read shorter), for each 64 KiB sub-slice `cum.push(cum.last() + memchr_iter(b'\n', slice).count())`; also add `memchr_iter(b'\r', slice).count()` into `cr_total` (second pass over L2-resident data, cheap, gives the exact `LF/CRLF/Mixed` label at completion; before completion show the label from the first-64 KiB sample). Check `cancel.load(Relaxed)` once per 4 MiB read. Send `IndexMsg::Batch{gen, new_cum: Vec<u64>, covered}` at most every 100 ms (track `Instant`), and `Done{gen, total_newlines, cr_total}` at the end; call `ctx.request_repaint()` only when a message is sent. After `Done` nothing runs: idle CPU stays 0 and `request_repaint` is never polled.
- Errors in the worker (Changed/Io) are sent as `IndexMsg::Failed`; the UI switches to the "File changed on disk" state.
- CRLF: count only `\n`; a trailing `\r` before `\n` belongs to the EOL and is not displayed. Lone `\r` (classic Mac) is not a line break in Phase 2 (ENC-03 handles CR in Phase 5) [ASSUMED acceptable; flag in Open Questions].

**Mapping when indexing is partial** (all O(64 KiB) per query, UI-thread safe through the chunk cache):
- `line_of(off) -> (u64 line0, Exact|Estimated)`: if `off < covered`: `b = off / B; line0 = cum[b] + count('\n' in [b*B, off))` -> Exact. Else Estimated: `avg = covered / max(cum.last(), 1)` bytes/line (if no newline seen yet use the first-block sample; if the sample has no `\n` the answer is line 0); `line0 = cum.last() + (off - covered) / avg`. When `complete`, everything is exact.
- `offset_of(line0)`: if `line0 <= cum.last()`: find block the block containing the `line0`-th newline is `b = cum.partition_point(|&c| c < line0) - 1` (cum is non-decreasing; use the first block whose next cumulative count reaches `line0`); scan that block (`memchr_iter(..).nth(k - cum[b])`), start = pos+1 (line 0 -> offset 0). Else Estimated: `off = covered + (line0 - cum.last()) * avg`, clamped to `len - 1`, then snap to the paragraph start with `scroll::segment_start`. Record `pending_jump = Jump::Line(line0)` and, when `Done` arrives, re-resolve exactly and re-anchor **only if the user has not scrolled since** (compare a `scroll_epoch` counter), per D-11/UI-SPEC.
- Percent: `off = len * p / 100` (u128 math), then `segment_start`.
- Gutter numbers for a viewport: compute `line_of(first paragraph start)` once (one 64 KiB cached read), then add 1 for each paragraph that ended with `\n`; hard-break continuations and soft-wrapped rows carry no number (the `›` glyph per UI-SPEC). Estimated rows get `~` and `MUTED_DIM`.
- Cumulative counts make the status bar `Ln` exact for any offset inside the covered prefix even while indexing continues (UI-SPEC "Rows inside the already-indexed prefix show exact").

**Time to index 20 GB:** disk-bound. Memory-bandwidth-bound scanning with memchr count is >= 10 GB/s per core on AVX2 hardware [ASSUMED, memchr docs claim bandwidth-class speeds]; sequential NVMe reads 3-7 GB/s -> ~3-7 s for 20 GB; SATA SSD ~0.5 GB/s -> ~40 s; HDD ~150 MB/s -> ~2.5 min [ASSUMED, typical device figures]. The 20 GB sparse test file on NTFS/APFS returns zeros without disk I/O, so it measures CPU/syscall cost only. Use 4 MiB reads (1 MiB is also fine; the syscall count at 4 MiB is 5,120 for 20 GB).

### Pattern 3: Stateless paragraph boundaries + byte-offset scroll anchor (D-04, D-05, D-20)

**Why a byte offset anchor:** it is the only coordinate that is always exact, cheap and independent of window width, wrap mode, index completeness and view type (hex<->text keeps position, D-09; resize/wrap toggle cannot invalidate it; row index or line number cannot be used because wrapped row counts are unknown without laying out the file and line numbers are estimates until indexing ends). Smooth scrolling keeps a fractional `sub_px` (< `ROW_HEIGHT`) beside it.

**Paragraph ("segment") rule.** Let `CAP = 10_000` bytes. A paragraph is the byte range between consecutive *boundaries*; boundary set = { 0, len } ∪ { n+1 : data[n] == b'\n' } ∪ { adj(p) : p % CAP == 0, p > 0, and no b'\n' in [p-CAP, p) }, where `adj(p)` advances `p` past UTF-8 continuation bytes (0x80..=0xBF) so a break never splits a character (<= 3 bytes).
- Guarantees: any paragraph is shorter than `2*CAP + 3` bytes (a line only gets a break at the first aligned multiple whose preceding CAP-window is newline-free, so a segment never contains a full newline-free aligned window); a line <= CAP bytes is never broken; a line > 2*CAP is always broken. So "lines longer than 10,000 are hard-broken" (D-04) holds with a 10-20 KB gray zone expressed in *bytes* (an upper bound on chars). Layout cost per paragraph is bounded (<= ~20k chars).
- Forward step `next_boundary(s)`: `m` = first `\n` at or after `s` (search only up to `s + 2*CAP + 4`); for each aligned multiple `q` in `(s, min(m+1, s+2*CAP+4)]` check `is_break(q)` (one `memchr` over `[q-CAP, q)`, <= 2 candidates); first hit -> `adj(q)`; else `m+1` (or `len`).
- Backward step `segment_start(a)` for any position `a`: `n` = last `\n` in `[max(0, a - 3*CAP), a)`; `n1 = n+1` if found else `max(0, a - 3*CAP)`; `p = (a / CAP) * CAP`; return `adj(p)` if `p >= n1 + CAP && p > 0` else `n1`. (Look-back 3*CAP is enough because `a - p < CAP` and a valid `p` needs a newline-free window of CAP before it.)
- Because both directions evaluate the same absolute predicate, **forward and backward stepping are exact inverses**, independent of where scrolling started. This is the property that fixes WR-04 and removes the End-key problem (WR-02): Ctrl+End = `anchor = segment_start(len-1)` then step back `visible_rows-1` rows.
- Must be property-tested: random byte files with long runs, no newlines, UTF-8 multibyte at aligned boundaries; walk forward collecting boundaries, walk backward from each, assert identical sequences.

**Rows inside a paragraph (soft wrap).** The paragraph's display string goes into one `LayoutJob` with `wrap.max_width = content_width` (wrap on) or `f32::INFINITY` (wrap off). The anchor may point into the middle of a paragraph; the top visible row is "the galley row containing the anchor char" (anchor -> char index via the paragraph's `byte_of_char` map -> `Galley::layout_from_cursor`/row search). Row stepping inside the paragraph walks galley rows; crossing the top/bottom moves to the adjacent paragraph (`next_boundary` / `segment_start(start-1)`). After a resize the anchor remains valid (the top row may start mid-row; acceptable).
- Cost control: layout only paragraphs overlapping the viewport plus one (<= ~60 jobs). egui memoises `LayoutJob -> Galley` by hash across frames and evicts unused entries each frame (`GalleyCache::flush_cache`, epaint galley_cache.rs) [VERIFIED: source], so redrawing the same scroll position re-uses galleys. Keep a small own `Vec<ParaLayout{start,end,galley:Arc<Galley>,byte_of_char}>` valid for `(width, wrap, font, ppp, source.version)` to avoid re-reading chunks and re-decoding between frames.
- Tessellation culls rows outside the clip rect (`coarse_tessellation_culling` defaults true, tessellator.rs:2285), so a 20 KB wrapped paragraph that is partly visible only emits visible rows. Horizontal culling inside a row does **not** exist: in wrap-off mode a 20k-char row emits all its glyphs each repaint. Risk is a minified/one-line file where *every* visible row is ~20k chars (50 rows x 20k glyphs ~ 1M quads per frame). Plan: implement straightforward per-row layout first; add the minified-file fixture to the benchmark; only if frame time is bad, slice the display string to the visible column window (exact for monospace; only CJK fallback glyphs make x non-linear) [ASSUMED risk; MEDIUM].
- Row height is fixed 20 pt (UI-SPEC). egui rows are laid out with the font's natural row height; set `LayoutJob.wrap` and use `TextFormat { line_height: Some(ROW_HEIGHT), .. }` so galley rows are exactly 20 pt (field exists on `TextFormat`; confirm name when implementing) [ASSUMED name; verify in epaint text_layout_types.rs].

**Scrollbar for unknown total rows.** Thumb position = `anchor / len` (byte-proportional); thumb length = `visible_bytes / len` where `visible_bytes` = bytes covered by the rendered rows, min 24 pt (UI-SPEC). Dragging maps the pointer fraction to `off = frac * len`, then `anchor = segment_start(off)`. Wheel/arrow/PageUp/PageDown always move by *rows* (exact via the inverse-stepping above); only the thumb is approximate, which is the standard trade-off. Do not try to show exact row totals.

**Horizontal scroll (D-20).** `hscroll_px: f32`, clamped to `[0, max_row_width_seen - content_width]` where `max_row_width_seen` is the widest galley width encountered so far in this document (grows monotonically; thumb size is therefore approximate and shrinks as wider rows are discovered; bounded by the paragraph cap). Shift+wheel and Left/Right-at-edge change `hscroll_px`. Paint with `x - hscroll_px`. In hex view use the same mechanism when narrower than ~600 pt (offset column fixed). Wrap on => `hscroll_px = 0` and bar hidden. Reset on file open, not on wrap toggle off->on.

**UTF-8 at chunk boundaries.** Because paragraph boundaries are char-aligned (`adj`), decoding never needs to carry state across the chunk cache: `read_range(para_start, para_len)` yields whole characters except where the *file* has invalid UTF-8. Decode with `<[u8]>::utf8_chunks()` (stable): valid chunks copied; each invalid sequence -> one `·` (`NON_PRINTABLE`) in the display string (UI-SPEC rule). The `byte_of_char: Vec<u32>` map (4 B per displayed char, <= 20k entries per paragraph) records the source byte offset of each display char, which solves tabs, control chars and invalid bytes uniformly: hit test returns a char index, selection stores `u64` byte offsets via the map; painting a selection converts the other way with a binary search. Tabs expand to 4 spaces (UI-SPEC "4 cells"; real tab stops optional later), `\r` before `\n` is dropped, other C0/C1 controls and DEL become `·`. The selection endpoints must be snapped to char boundaries (use the map; for hex selections no snapping is needed).

### Pattern 4: Workers, cancellation, repaint discipline (CORE-07, D-12)

```rust
struct Job { cancel: Arc<AtomicBool>, gen: u64 }       // one per index / copy
// worker
let mut last = Instant::now();
loop { if cancel.load(Ordering::Relaxed) { break; } ...read, count...
       if last.elapsed() >= Duration::from_millis(100) { tx.send(Batch{..}).ok(); ctx.request_repaint(); last = Instant::now(); } }
tx.send(Done{..}).ok(); ctx.request_repaint();
```
- `mpsc::Sender::send` failing means the UI dropped the receiver (file replaced): exit. Every message carries `gen`; the UI ignores stale ones. Opening a new file sets the old job's `cancel` and drops its receiver.
- UI drains with `try_recv` in a `while let` at the top of `App::ui`. Progress percent = `covered * 100 / len`.
- `ctx` is `Clone + Send`; clone once into the worker. Never call `request_repaint` in a loop without a message; never from an idle timer. Esc: priority Go-to box > dialog > cancel most recent job (UI-SPEC).
- A panic in a worker must not take down the UI silently: with `panic = "abort"` in release (Cargo.toml) a worker panic aborts the process. Treat all worker I/O as `Result`, no `unwrap` on I/O or index arithmetic; use checked/saturating math on `u64` offsets.

### Pattern 5: File > Open with rfd 0.17.2 (D-15, OPEN-09)

- `rfd::FileDialog::new().set_directory(last_dir).pick_file() -> Option<PathBuf>` is a blocking call; run it in `std::thread::spawn`, send the `Option<PathBuf>` over mpsc, `ctx.request_repaint()`. The UI keeps repainting (the thread blocks, not the UI).
- Parenting: pass the window as parent via `set_parent(&frame)` is possible because eframe's `Frame` implements `HasWindowHandle`; on macOS this attaches a sheet, on Windows it makes the dialog modal to the window. Without a parent the dialog is application-modal-less and may open behind. Recommended: set the parent (needs the window handle available on the UI thread, then moved into the thread as an owned value; `rfd` takes `&W` and extracts raw handles immediately, so call `set_parent` on the UI thread and then move the built `FileDialog` into the thread, which is `Send` [ASSUMED; verify compile]).
- macOS main-thread requirement: rfd's macOS backend runs the dialog through `run_on_main`, which dispatches to the main queue when called off-main and the `NSApplication` is running (our eframe/winit loop), and **panics** with "You are running RFD in NonWindowed environment" if the app is not running [VERIFIED: rfd-0.17.2/src/backend/macos/utils.rs]. Calling from a worker thread after the event loop started is therefore the supported path. Verification must be a manual macOS check (cannot be tested here) -> include in HUMAN-UAT.
- Linux: `xdg-portal` only (no `gtk3`): dialogs go through `libdbus-1.so.3` (dlopen) to `xdg-desktop-portal`; if libdbus or a portal implementation is missing, rfd logs "Can't connect to a portal" and returns `None` [VERIFIED: portal/libdbus.rs:77]. So on a bare WM without a portal the Open dialog silently does nothing: show a status message when a dialog thread returns `None` *quickly* is ambiguous with Cancel; Recommended: accept, and document that CLI and drag & drop remain available. (Open Question 3.)
- Remember `last_dir = picked.parent()` in app state (session only, D-15).

### Pattern 6: Selection, hit testing, copy (D-13, D-14, EDIT-10)

- State: `Selection{ anchor: u64, head: u64 }` (byte offsets, may be equal = caret). Ctrl+A = `(0, len)` costs nothing. Keyboard caret moves (Shift+arrows) operate on visible-paragraph layout; Shift+Ctrl+End etc. just set `head` (range may be 20 GB; never materialised).
- Hit test: pointer pos -> find paragraph by row y -> `galley.cursor_from_pos(pos - galley_origin).index` (**char index**, `CCursor.index: CharIndex`, "Character offset (NOT byte offset!)" [VERIFIED: epaint cursor.rs:11]) -> `byte_of_char[idx]` -> `u64`. `cursor_from_pos` clamps above/below the galley with a 5 pt margin, which gives drag-select beyond the paragraph for free. Drag past the viewport edge: autoscroll by rows each frame while `pointer.is_decidedly_dragging` (request repaint while dragging only).
- Painting: per visible paragraph with selection overlap, iterate `galley.rows` accumulating char counts, rect from `row.x_offset(col)` .. (`Row::x_offset` is used internally by `pos_from_layout_cursor`), full row height, one rect per row, clipped to viewport (UI-SPEC). `SELECTION` / `SELECTION_INACTIVE` by window focus.
- Double-click word: read up to 256 bytes each side of the click through the cache, scan by char class (alphanumeric/`_` vs whitespace vs other). Triple-click: the paragraph (logical line) bounds `[segment_start, next_boundary)` minus the trailing newline; for hard-broken lines this selects the segment (document).
- Copy flow: if `|head-anchor| == 0` nothing. `<= 16 MiB`: start the copy worker immediately (UI-SPEC says "copies immediately": the worker usually finishes within a frame or two; status shows the cancel only if it exceeds ~200 ms). `16 MiB..=256 MiB`: confirm dialog with size. `> 256 MiB`: error dialog, no copy. The worker reads with its own handle in 1-4 MiB steps into one pre-sized `Vec<u8>` (size known up front), checking `cancel`; result via `String::from_utf8(vec)` (zero-copy when valid) falling back to `String::from_utf8_lossy(&bytes).into_owned()` only for invalid data (invalid bytes -> U+FFFD in the clipboard text; document). Then UI thread calls `ctx.copy_text(string)` (`OutputCommand::CopyText(String)`, ownership moves into egui-winit; with the `clipboard` feature arboard receives it, otherwise it is silently kept in-process: see Pitfall 1).
- Peak memory at the 256 MiB cap is roughly 256 MiB (bytes->String, no copy) plus platform clipboard conversion (Windows CF_UNICODETEXT is UTF-16: up to +512 MiB transient; macOS/X11 copy the UTF-8 once) [ASSUMED]. It is transient and only for explicit user action; the idle footprint gate is unaffected. Hex copy: generate `XX XX ...` text (3 chars per byte) in the worker in chunks into a `String`, cap applies to selected *bytes* (256 MB of bytes -> 768 MB of text: apply the cap to the produced text size or lower the hex cap; Open Question 4).

### Pattern 7: Hex view and binary heuristic (D-09, D-10, HEX-01/03/05)

- Rows: `row = off >> 4`, total rows `(len + 15) >> 4` as `u64`; scrollbar is exactly proportional (no uncertainty). Anchor shared with text (`anchor & !15` for hex; entering text from hex: `segment_start(anchor)`).
- Read `16 * (visible_rows + 1)` bytes per frame through the chunk cache; paint offset column (16 hex digits), 2 groups of 8, ASCII with `·` for non-printables outside 0x20..=0x7E, per UI-SPEC. Cell geometry from the monospace cell width (`ui.fonts(|f| f.glyph_width(&font, 'M'))`; verify exact method name) so hit testing is arithmetic: `col = ((x - hex_x0) / cell_w)` mapped through the group gaps; ASCII column is `x - ascii_x0 / cell_w`. Hit test never needs `Galley`.
- Selection in hex = byte range, painted as per-row rects spanning both digits plus half gaps; mirrored in the ASCII column. Copy = uppercase hex pairs separated by spaces (UI-SPEC).
- Binary heuristic (D-10): on the open worker read the first 8 KiB (`min(len, 8192)`); `binary = contains NUL || control_count * 100 > 30 * n` where control = bytes `< 0x20` except `\t \n \r \x0c`(and optionally 0x1B), plus `0x7F`. Empty file: not binary. UTF-16 text contains NULs and will trigger the banner: acceptable in Phase 2 (never auto-switches; encoding is Phase 5) [ASSUMED acceptable; Open Question 5].
- Ctrl+G grammar parser (pure, unit-tested): trim; `0x`/`0X` + hex -> offset; leading `#` + decimal -> offset; trailing `%` -> percent (0..=100, decimals allowed); bare decimal -> line (text) or offset (hex); everything else -> "Not a valid position". Clamp past-end to last line/byte with the live preview.

### Pattern 8: CJK fallback in egui 0.37 (D-07, UI-05)

- egui 0.37/epaint 0.37 has `FontProvider` (trait: `fonts_for_family`, `font_for(&FallbackRequest{cluster,family}) -> Option<FontInsert>`), `Context::add_font_provider(Arc<dyn FontProvider>)`, `set_font_providers`, plus `Context::add_font(FontInsert)` and `set_fonts`. Discovery is invoked "after every installed font of the family missed", **at most once per (family, character), also when returning None**; found fonts append to that family's fallback chain [VERIFIED: epaint font_provider.rs, egui context.rs:865].
- eframe, by default, installs a provider on native when `NativeOptions::system_font_fallback` is true (default): with its `system_fonts` feature = `egui_system_font_provider::SystemFontProvider::new()`, which **starts enumerating system fonts on a background thread at startup** (cost at every launch, even for ASCII files) and blocks the UI on first use if enumeration is not done [VERIFIED: eframe epi_integration.rs:261, provider lib.rs:55,106]. Our Cargo uses `default-features = false` so without `system_fonts`, eframe installs a `MissingSystemFontsWarning` that returns `None` and, because results are cached per (family, char), would permanently poison CJK lookups.
- Recommended: set `NativeOptions { system_font_fallback: false, .. }` and call `ctx.add_font_provider(Arc::new(SystemFontProvider::new()))` **lazily**, only when the visible paragraph text contains a non-ASCII char for which `fonts.has_glyph(&font_id, c)` is false (check once per opened file's first screen and on each new paragraph decode that contains non-ASCII; install once with a `bool` guard). Install *before* the first layout that needs the glyph (provider becomes active at the start of the next pass; the text of that frame may show tofu for one frame; call `request_repaint()` once). This keeps startup and the ASCII idle footprint identical to Phase 1 (the enumeration thread and fontique tables are never created) and costs no repaint at idle.
- Per-OS discovery uses the OS font fallback via fontique: DirectWrite-class system fonts on Windows (Yu Gothic / Meiryo / Microsoft YaHei per locale), CoreText on macOS (PingFang/Hiragino), fontconfig via `dlopen` on Linux (Noto CJK if installed) [CITED: egui_system_font_provider README/lib docs; per-OS names ASSUMED]. Linux without any CJK font installed shows tofu: acceptable, report in status? (Open Question 6).
- Memory: fonts are memory-mapped, so on macOS (`phys_footprint`) clean file-backed pages are not counted; on Windows the working set counts touched mapped pages only (glyph outlines actually used + tables headers) rather than the whole 10-30 MB file; the egui glyph atlas grows by the rasterised glyphs (a 16-bit alpha atlas of a few MB at most for a screenful of CJK). Expected CJK cost: a few MB to low tens of MB on Windows [ASSUMED, MEDIUM]; the headroom is ~10 MB (59.6 of 70 MB measured). **The idle gate is measured with an ASCII file, so it stays green; add a CJK-file measurement to the Phase 2 measurements as an informational number** and a note that opening CJK files will exceed the ASCII footprint by the measured amount. Cannot unload a font once added.
- The IME cursor-rect plumbing (`ime.rs`) is untouched by this.

### Pattern 9: Drag & drop (OPEN-02)

- `ctx.input(|i| i.raw.hovered_files.clone())` while hovering (`HoveredFile{ path: Option<PathBuf>, mime }`) for the overlay; on drop `i.raw.dropped_files` is a `Vec<DroppedFileHandle>` (`Arc<dyn DroppedFile + Send + Sync>`) in egui 0.37, read the path with `.path()` (absolute on native). **Never call `.bytes()`** (reads the entire file into a `Vec`; this is the exact anti-pattern the project exists to avoid) [VERIFIED: egui dropped_file.rs].
- On Windows, drop support requires drag-and-drop not disabled in `ViewportBuilder` (default enabled; do not call `with_drag_and_drop(false)`) [CITED: raw_input.rs doc comment].
- Wayland: winit 0.30.x has no file drag-and-drop on its Wayland backend (HoveredFile/DroppedFile are not emitted); only X11, Windows, macOS work. Fix arrives with winit 0.31, which eframe 0.37 does not use [CITED: winit issue #1881 tracking, search results 2026-10; egui issue #8805]. Document in the app Help/README; File > Open and the CLI are the Wayland paths. Under XWayland (`WAYLAND_DISPLAY=` unset) DnD works. This is a known, accepted limitation, not a bug to chase in Phase 2.
- Take the first dropped file; a directory or non-regular file reuses the Pattern-1 classification and shows the UI-SPEC status message. Multiple files: first only (D-15).

### Pattern 10: Minimal unsigned macOS `Whim.app` (D-21)

**Why:** a bare Mach-O executable double-clicked in Finder is handed to Terminal.app; an `.app` bundle with an `Info.plist` is launched as a GUI app (the user's report). No Rust code changes are needed for this task.

**1. Info.plist keys** (`Contents/Info.plist`, XML plist):

| Key | Value | Required? |
|-----|-------|-----------|
| `CFBundleExecutable` | `whim` (must equal the file name in `Contents/MacOS/`) | yes |
| `CFBundleIdentifier` | reverse-DNS, e.g. `io.github.ajoakim.whim` (keep stable; Phase 8 associations/prefs key off it) | yes |
| `CFBundleName` | `Whim` | yes |
| `CFBundlePackageType` | `APPL` | yes |
| `CFBundleShortVersionString` | `CARGO_PKG_VERSION` | yes in practice |
| `CFBundleVersion` | build number, may equal the version | recommended (some tools warn without it) |
| `NSHighResolutionCapable` | `<true/>` | yes: without it macOS runs the window in scaled low-res mode (breaks UI-04 crispness) |
| `LSMinimumSystemVersion` | `11.0` for the arm64 build; x86_64 floor from the deployment target rustc actually used (check `otool -l whim` for `LC_BUILD_VERSION minos`) | yes; the x86_64 floor is [ASSUMED] |
| `CFBundleIconFile` | optional; needs `Contents/Resources/<name>.icns` | optional (skip until an icon exists) |
| `NSPrincipalClass` = `NSApplication` | emitted by cargo-bundle/Tauri | not required for winit: winit creates the `NSApplication` itself, so **no other key is needed** for a plain eframe/winit app [ASSUMED from winit's design; confirm by launching the produced bundle on the macOS runner / UAT] |
| `CFBundleDocumentTypes` / `UTImports` | file associations | NOT now (Phase 8, OPEN-04) |

**2. Build from an xtask without cargo-bundle (host-independent):** add `cargo xtask bundle-macos --bin <path-to-whim> --out <dir> [--version V]` using `std::fs` only:
- create `<out>/Whim.app/Contents/{MacOS,Resources}`, copy the binary to `Contents/MacOS/whim`, write `Contents/Info.plist` from a `format!` template (XML-escape values), optionally copy `Resources/AppIcon.icns`, optionally write `Contents/PkgInfo` (`APPL????`).
- exec bit: `#[cfg(unix)]` `set_permissions(.., Permissions::from_mode(0o755))` via `std::os::unix::fs::PermissionsExt`; on a Windows host the layout is produced without it (real bundles are built on the macOS runner after `cargo build --release`).
- Make `info_plist(version) -> String` and the layout builder pure and unit-testable on every OS (assert required keys, `CFBundleExecutable` matches the copied file, balanced tags); on the macOS runner additionally run `plutil -lint Contents/Info.plist`.
- No signing and no dependency on `cargo-bundle`/`cargo-packager`.

**3. Zipping in CI so the exec bit survives:** on the macOS runner, after assembling and signing: `ditto -c -k --sequesterRsrc --keepParent target/bundle/Whim.app Whim-macos-arm64.zip` (ditto stores Unix modes, symlinks and xattrs; `--keepParent` puts `Whim.app/` at the archive top level). Upload that `.zip` file with `actions/upload-artifact`. Caveat: upload-artifact zips its inputs and does not preserve file modes, so never upload the bare `Whim.app` directory; the downloaded artifact is then a zip containing `Whim-macos-arm64.zip` (unzip twice) [CITED: actions/upload-artifact README on permission loss; ASSUMED still true for the pinned v7]. Add a CI assertion: extract with `ditto -x -k Whim-macos-arm64.zip out/ && test -x out/Whim.app/Contents/MacOS/whim`.

**4. Gatekeeper/quarantine for an unsigned downloaded app (current macOS):**
- Browser downloads carry `com.apple.quarantine`; on first launch Gatekeeper wants a signed and notarized app. An ad-hoc-signed app is neither, so the first launch is blocked.
- **macOS 15 Sequoia and later removed the Control-click > Open override** for such apps. The supported route is: try to open once, then System Settings > Privacy & Security > "Open Anyway" next to the Whim message (admin authentication) [CITED: mjtsai.com/blog/2024/07/05 "Sequoia Removes Gatekeeper Contextual Menu Override"; Apple Discussions thread 255759797; WebSearch 2026-10, MEDIUM].
- On macOS 14 and earlier, right-click > Open still works [ASSUMED from long-standing behaviour].
- Terminal fallback valid on all versions: `xattr -dr com.apple.quarantine /path/to/Whim.app` (Gatekeeper only evaluates quarantined files). Files fetched via `curl`/`gh`/`git` are not quarantined and open directly.
- **The D-21 README wording "right-click > Open the first time" (and PERF-04's text) is outdated for macOS 15+.** Recommend the README list all three: right-click > Open (macOS 14 and earlier), Open Anyway in System Settings (macOS 15+), and the `xattr` command. Signing/notarization stays DIST-01 (v2).

**5. Ad-hoc `codesign -s -` for arm64: yes, as the last step before zipping.**
- Apple-silicon macOS refuses to run an arm64 Mach-O without a valid signature. The Apple linker emits a linker-signed ad-hoc signature automatically, so the raw `cargo build` binary runs (Phase 1 CI measured it). Any **post-link modification** invalidates it and the kernel kills the process at exec (exit 137, "Killed: 9"), e.g. non-Apple `strip`/`install_name_tool` or byte edits [CITED: github.com/cline/cline#14215, bendlang/bend#1025, Return-To-The-Roots/s25client#1998; WebSearch 2026-10].
- Our pipeline should normally keep the signature: rustc `strip = "symbols"` is applied through linker flags at link time, `cp` into the bundle preserves bytes, and `Info.plist` is a separate file. But rewriting the bundle (re-copying the binary, patching it, `lipo`) would silently break it.
- Run `codesign --force --deep --sign - target/bundle/Whim.app`, then `codesign --verify --deep --strict --verbose=2 target/bundle/Whim.app`, then `ditto`. The bundle-level ad-hoc signature also seals `Info.plist` and the identifier. It does **not** make Gatekeeper accept the app (still not notarized): it only guarantees launch validity.
- Never `strip`/`lipo`/`install_name_tool` after signing; sign after `lipo` if universal2 is ever built. Phase 2 ships per-arch (`macos-latest` = arm64). The xtask itself must not require `codesign` (non-mac hosts); only the macOS CI step runs it.

**CI wiring (small task):** in the `gate` job, macOS + glow only, after `Measure (macOS)`: `cargo xtask bundle-macos`, `codesign` + verify, `ditto` zip, exec-bit assertion, smoke-launch `Whim.app/Contents/MacOS/whim --bench --bench-exit-after 3` (reuses the Phase 1 bench contract), upload `Whim-macos-arm64.zip`. The size gate keeps measuring the stripped inner binary. Manual UAT item: double-click `Whim.app` in Finder and confirm no Terminal window opens, then File > Open a file (Dock-icon drop and "Open with" are Phase 8).

### Anti-Patterns to Avoid
- **`file.read_to_end` / `DroppedFile::bytes()` / `std::fs::read`** on user paths: violates CORE-02.
- **Sharing one `File` across threads on Windows** (`seek_read` moves the shared cursor; docs say nothing on concurrency).
- **`String::from_utf8_lossy` on whole selections for display**: loses the byte map; use `utf8_chunks` + map for display, lossy only for the clipboard string.
- **Storing line numbers per line** or laying out `Galley`s for lines outside the viewport.
- **Calling `request_repaint()` from a poll loop or on every batch** (use the 100 ms throttle).
- **Using row indices or line numbers as the scroll position.**
- **A single global `egui::TextEdit`** for the viewer (cannot virtualize; Phase 1 decision).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Newline counting | Byte loop | `memchr::memchr_iter(b'\n', buf).count()` / `memrchr` / `memchr_iter().nth()` | SIMD; `count` is specialised [VERIFIED] |
| UTF-8 validation/chunking | Manual state machine | `<[u8]>::utf8_chunks()`, `std::str::from_utf8` | std, correct for invalid sequences |
| Text wrapping, glyph x offsets, hit test | Own wrap algorithm | `LayoutJob{wrap: TextWrapping::wrap_at_width(w) / no_max_width()}`, `Galley::cursor_from_pos`, `pos_from_cursor` | egui handles shaping, kerning, color glyphs |
| Native file dialog | Own dialog | `rfd` | Per-OS COM/NSOpenPanel/portal complexity |
| System font discovery | Registry/CoreText/fontconfig code | `egui_system_font_provider` via `FontProvider` | Per-script, per-locale fallback logic done |
| Clipboard | Platform APIs | egui `ctx.copy_text` + eframe `clipboard` feature (arboard) | Already integrated, cross-platform |
| Atomic cancellation | Custom signalling | `Arc<AtomicBool>` + mpsc `Sender` drop | Sufficient and tiny |
| Sparse test files | Writing GBs | existing `cargo xtask sparse` | Already built in Phase 1 |

**Key insight:** the only genuinely custom algorithms in this phase are the paragraph-boundary rule and the line-index mapping; everything else has a std/egui/memchr answer.

## Runtime State Inventory

Not a rename/refactor phase, but it does replace `src/file.rs` and `src/view/rows.rs`: no persisted data, services or OS registrations exist yet (no session/config files are written in Phase 2). Build artifacts: none. Secrets/env: none. Nothing to migrate. (Verified by reading the repo; settings persistence is Phase 8.)

## Common Pitfalls

### Pitfall 1: Copy silently never reaches the OS clipboard
**What goes wrong:** `ctx.copy_text(..)` appears to work but pasting elsewhere gives nothing.
**Why it happens:** eframe `clipboard` feature is off in `Cargo.toml` (default-features=false); egui-winit then stores the text in a private `String` [VERIFIED].
**How to avoid:** add `"clipboard"` to the eframe features (+0.06 MiB measured on Windows); add a manual UAT step "copy then paste into another app" on all 3 OSes (Wayland needs `wayland-data-control` as pulled by eframe's feature).
**Warning signs:** no `arboard` in `cargo tree`.

### Pitfall 2: `seek_read` cursor race on Windows
**What goes wrong:** corrupted/misplaced reads when UI and worker share one handle.
**How to avoid:** one handle per thread (Pattern 1).

### Pitfall 3: Blocking open of FIFO/device on the UI thread (D-19)
**How to avoid:** stat first, reject non-regular files, open on a worker; the CLI path in `main.rs` currently opens synchronously before the window.

### Pitfall 4: Wrapped-row scroll mismatch (WR-04 class)
**What goes wrong:** Up then Down does not return to the same place; End key stops short.
**How to avoid:** the stateless boundary rule; property test forward/backward equivalence.

### Pitfall 5: Hard-break splitting a UTF-8 character or a CRLF
**How to avoid:** `adj(p)` skips continuation bytes; a `\r` immediately before `\n` is never a break point because breaks only occur in newline-free windows at multiples of CAP (a break exactly between `\r` and `\n` is possible if `\n` sits at p; `\r` then ends a segment and renders as a lone control `·` at the end of the last row: strip a trailing `\r` when the next byte is `\n`).

### Pitfall 6: CJK glyph lookup poisoned by the stub provider
Explained in Pattern 8. Disable eframe's fallback (`system_font_fallback: false`) and install ours lazily.

### Pitfall 7: Idle CPU regression
Indexer or copy worker leaving a repaint loop; `request_repaint_after` timers for notices. Keep: no repaint without a message; notices use one `request_repaint_after(6s)` as in UI-SPEC. Verify with `cargo xtask measure` idle CPU < 1 % **with a file open and indexing done**.

### Pitfall 8: Footprint measured only without a file
Phase 1 measured a 20 GiB file with mmap (same working set). Phase 2 must run `measure --file <20 GiB sparse>` after indexing completes (settled window +20..+30 s) and a no-file run; the sparse file indexes in seconds, so the window is clean.

### Pitfall 9: Estimated line number thrash
Estimates change as `avg` is refined. Gutter numbers for a *stationary* view will visibly change as batches arrive (<= 10/s). Accept (dimmed + `~`), but keep the anchor byte-stable; never re-anchor on estimate updates except the pending line jump (only if the user has not scrolled).

### Pitfall 10: Huge `u64 -> usize/f32` casts
`f32` holds integers exactly only to 2^24: never put byte offsets in `f32` (thumb math: compute `(anchor as f64 / len as f64) * track_h`); use `u128` for `len * pct / 100`.

### Pitfall 11: Cache invalidation on `version`
Own layout cache and chunk cache must be keyed/cleared by `(source.version, source.len)` so Phase 3 (edits) works; use `version = 0` constant for `FileSource` now.

### Pitfall 12: Windows files written while open for append (logs)
`Changed` will fire on growing logs. Phase 2: show the UI-SPEC error with Reload; Phase 6 adds tail mode. Mention in the open banner of the error text? (no change).

## Code Examples

### Counting newlines per 64 KiB block (index worker core)
```rust
// Source: memchr 2.8.3 (Memchr::count specialised) - verified in registry source
const B: usize = 64 * 1024;
fn index_buffer(buf: &[u8], cum: &mut Vec<u64>, cr_total: &mut u64) {
    for blk in buf.chunks(B) {
        let nl = memchr::memchr_iter(b'\n', blk).count() as u64;
        *cr_total += memchr::memchr_iter(b'\r', blk).count() as u64;
        cum.push(cum.last().copied().unwrap_or(0) + nl);
    }
}
```
(A final partial block pushes a cumulative entry too; track `covered` separately so the entry for a partial tail block is only used when `complete`.)

### Lazy font provider (Pattern 8)
```rust
// Source: epaint-0.37.0 font_provider.rs; egui_system_font_provider-0.37.0 lib.rs
if !self.font_provider_installed && display_text.chars().any(|c| !c.is_ascii() && !fonts.has_glyph(&font_id, c)) {
    ctx.add_font_provider(std::sync::Arc::new(egui_system_font_provider::SystemFontProvider::new()));
    self.font_provider_installed = true;
    ctx.request_repaint();
}
// NativeOptions { system_font_fallback: false, .. }   // otherwise eframe installs a stub provider
```
(`Fonts::has_glyph(&FontId, char)` exists at epaint fonts.rs:~236 on the view type; reach it through `ctx.fonts_mut(|f| f.has_glyph(..))`, per `Context::fonts_mut` at context.rs:1379.)

### Opening a dialog without blocking (Pattern 5)
```rust
let (tx, rx) = std::sync::mpsc::channel();
let ctx2 = ctx.clone();
let dir = self.last_dir.clone();
std::thread::spawn(move || {
    let mut d = rfd::FileDialog::new();
    if let Some(dir) = dir { d = d.set_directory(dir); }
    let _ = tx.send(d.pick_file());
    ctx2.request_repaint();
});
self.pending_dialog = Some(rx);   // poll with try_recv at the top of ui()
```

### Reading a dropped file path only
```rust
let dropped: Vec<std::path::PathBuf> = ui.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
if let Some(p) = dropped.into_iter().next() { self.request_open(p); }
```

### CI change for D-18 (macOS hard, others report-only)
```yaml
env:
  MACOS_MAX_STARTUP_MS: "600"
  MACOS_MAX_FOOTPRINT_MB: "70"
# step "Measure (macOS)" - glow only gets hard budgets, metric is phys_footprint via `--max-rss-mb`
env:
  BUDGET_FLAGS: ${{ matrix.backend == 'glow' && format('--max-startup-ms {0} --max-rss-mb {1} --max-idle-cpu {2}', env.MACOS_MAX_STARTUP_MS, env.MACOS_MAX_FOOTPRINT_MB, env.CI_MAX_IDLE_CPU) || '' }}
```
- Currently the macOS step uses `CI_MAX_STARTUP_MS 1300`/`CI_MAX_RSS_MB 180`; the existing flag is named `--max-rss-mb` but evaluates the footprint metric (`xtask/src/measure.rs: evaluate`), so no xtask change is needed for the gate itself. Observed macOS values (01-GATE): 37.6 MB footprint, 192 ms median (one earlier outlier at 501.6 ms): a 600 ms limit leaves 3x margin on the typical run but the earlier 501.6 ms outlier shows noise, so keep `--runs 10` (median) and consider a retry on failure rather than loosening.
- Leave the Windows (`continue-on-error: true`, Mesa llvmpipe) and Linux (Xvfb) steps and the wgpu legs unchanged (report-only). Size stays hard everywhere (`MAX_SIZE_MB: 10`).
- Add a second macOS measure step with `--file` pointing at a sparse 20 GiB file created by `cargo xtask sparse` (APFS sparse; xtask guards `MAX_ON_DISK`) so the hard gate also covers "file open + indexed". Windows local manual measurement covers the real-hardware Windows number.

### Test fixtures
`cargo xtask sparse` exists; add small deterministic generators (in test modules, not xtask): long single line (e.g. 50 MB without `\n` via a sparse-like repeating pattern into a temp file), CRLF file, invalid UTF-8, mixed CJK, 10k tiny lines. Temp files must use `tempfile`-style unique dirs (Phase 1 review WR-05 flagged a predictable shared temp path): use `std::env::temp_dir()` + pid + counter, as `file.rs` tests already do.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Manual `FontDefinitions` for CJK | `FontProvider` trait, on-demand system fonts | egui 0.37 | D-07 needs ~10 lines + one crate |
| `raw.dropped_files[i].path: Option<PathBuf>` / `.bytes: Option<Arc<[u8]>>` | `DroppedFileHandle` with `path()`/`bytes()` | egui 0.37 | Must use `path()`; `bytes()` loads everything |
| `ctx.output_mut(|o| o.copied_text = ..)` | `ctx.copy_text(String)` -> `OutputCommand::CopyText` | egui 0.3x -> 0.37 | Needs eframe `clipboard` feature to hit the OS |
| mmap-everything | positional reads + bounded cache | project decision D-01 | no SIGBUS, no Windows file-lock side effects |
| winit Wayland DnD | none in 0.30; arrives in 0.31 | winit 0.31 | Wayland drop unsupported for now |

**Deprecated/outdated:** `Painter::layout_no_wrap` per-line for the whole view (fine for the gutter, not for paragraphs); fixed 128-byte rows (`rows.rs`) are removed.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | memchr newline counting >= 10 GB/s and NVMe 3-7 GB/s => 20 GB indexed in ~3-7 s | Pattern 2 | Only affects progress UX, not correctness |
| A2 | Positional reads keep OS cache out of the process footprint on macOS/Linux | Pattern 1 | Footprint gate surprise; mitigated by macOS CI hard gate |
| A3 | Lone `\r` is not a line break in Phase 2 | Pattern 2 | Old-Mac files render as one huge line (hard-broken, still usable) |
| A4 | CJK font mmap adds only a few MB to working set | Pattern 8 | Windows CJK files may exceed the 70 MB figure; informational measurement planned |
| A5 | rfd `FileDialog` built with `set_parent` on the UI thread can be moved to a thread; macOS off-main works with running NSApp | Pattern 5 | macOS dialog may need main-thread dispatch instead; UAT on macOS |
| A6 | `TextFormat.line_height` exists in epaint 0.37 for fixed 20 pt rows | Pattern 3 | Need another way to fix row height |
| A7 | Per-row full-glyph emission in wrap-off mode is acceptable until measured | Pattern 3 | Frame spikes on minified one-line files; fallback = column-window slicing |
| A8 | Linux/macOS binary-size deltas are < 1 MiB | Standard Stack | Size gate failure on CI; gate will show it |
| A9 | Windows `CF_UNICODETEXT` conversion causes +2x transient on large copies | Pattern 6 | Higher transient peak at 256 MB cap |
| A10 | CAP = 10,000 bytes (segments < 20 KB) satisfies D-04 closely enough | Pattern 3 | If strictness wanted, CAP = 5,000 gives a hard 10 KB bound but splits 5-10k-char lines |
| A11 | `NSPrincipalClass` is not needed for winit; x86_64 `LSMinimumSystemVersion` floor | Pattern 10 | Bundle might misbehave at launch; fix by adding the key (UAT) |
| A12 | upload-artifact (pinned v7) still drops file modes; macOS 14 right-click > Open still works | Pattern 10 | Wrong packaging or README wording |

## Open Questions

1. **D-04 "10,000 chars" vs the byte-based absolute-aligned rule**
   - Known: lines <= 10,000 bytes never break; > 20,000 always break; between may or may not.
   - Unclear: whether the user wants an exact 10,000 limit.
   - Recommendation: accept CAP=10,000 bytes (documented gray zone). Exact limits would need per-line start knowledge, which breaks stateless scrolling.
2. **Row height API** (A6): verify `TextFormat::line_height` or fall back to `Galley` rows positioned manually at 20 pt.
3. **Linux without xdg-desktop-portal/libdbus**: Open dialog returns `None`. Recommendation: accept; mention in status text when dialog returns `None` within < 100 ms and Linux (heuristic), or do nothing. Not a blocker.
4. **Hex copy cap**: 256 MB of bytes becomes 768 MB of text. Recommendation: apply the 256 MB cap to the produced text (i.e. ~85 MB of bytes) in hex mode, show the same over-cap dialog with the byte limit, and note it in the dialog.
5. **UTF-16 files trigger the binary banner** (NULs). Recommendation: accept in Phase 2; Phase 5 encoding detection removes it (add a one-line known-limitation note).
6. **No CJK font installed (typical minimal Linux)**: tofu glyphs. Recommendation: accept, no status message in Phase 2.
7. **Windows `FILE_FLAG_SEQUENTIAL_SCAN` for the indexer handle**: measure on the real machine with the 20 GiB sparse file; adopt only if it improves throughput.
8. **Re-anchor after index completes**: define "user has not scrolled" as `scroll_epoch` unchanged; only for `Jump::Line` / percent estimates, never for plain scrolling.
9. **PERF-04 / D-21 README wording**
   - Known: macOS 15+ removed right-click > Open for unnotarized apps; Open Anyway or `xattr -dr com.apple.quarantine` is required.
   - Recommendation: README lists all three routes and PERF-04 text is amended; confirm with the user.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | build | yes | 1.99.0 (pinned via rust-toolchain.toml; outside the repo `cargo` is older) | — |
| cargo xtask (sparse/measure/check-size) | fixtures, gates | yes | in repo | — |
| memchr, rfd, egui_system_font_provider sources | build | yes (downloaded) | 2.8.3 / 0.17.2 / 0.37.0 | — |
| 20 GiB sparse file | CORE-01 checks | creatable via `cargo xtask sparse` | NTFS/APFS/ext4 | smaller sparse files for CI |
| macOS machine | rfd threading, DnD, clipboard, CJK checks | no (CI only) | — | HUMAN-UAT items |
| Linux desktop with portal | rfd Linux, Wayland DnD | no | — | CI cannot open dialogs; UAT item |
| CJK system font (Windows) | UI-05 | expected (Yu Gothic/Meiryo on Win11) | — | tofu |

**Missing with no fallback:** none blocking planning; macOS/Linux dialog, clipboard and DnD behaviours are manual UAT only.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` (cargo test), colocated `#[cfg(test)]` modules; no new dev-dependencies |
| Config file | none (workspace `Cargo.toml`); CI: `cargo test --workspace --locked`, clippy `-D warnings`, fmt on Linux |
| Quick run command | `cargo test -p whim --lib <module>::` |
| Full suite command | `cargo test --workspace --locked && cargo clippy --workspace --all-targets --locked -- -D warnings` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CORE-01/02 | `read_range` reads only requested bytes; first page of sparse 20 GiB < 1 s; RSS flat | integration + manual measure | `cargo test -p whim source::` ; `cargo xtask measure --file <sparse> --runs 3` | Wave 0 |
| CORE-02 | Source never allocates > cache size (assert cache len <= 16 blocks after random reads) | unit | `cargo test -p whim source::cache` | Wave 0 |
| D-01 | Truncating the file mid-read returns `Changed`, no panic | unit | `cargo test -p whim source::truncate` | Wave 0 |
| D-19 | FIFO / dir rejected as non-regular without blocking (unix `mkfifo`, `#[cfg(unix)]`); dir on all OS | unit | `cargo test -p whim open::` | Wave 0 |
| CORE-03/04 | `line_of`/`offset_of` exact for covered prefix and monotone/estimated beyond; index batches equal reference count | unit + property (xorshift) | `cargo test -p whim lineindex::` | Wave 0 |
| CORE-04 | Index worker: cancel stops quickly; Done after full scan; `cr_total` EOL label | unit | `cargo test -p whim lineindex::worker` | Wave 0 |
| CORE-06 | Boundary rule: forward/backward inverse; max paragraph < 2*CAP+3; char-aligned; no-newline 50 MB file stepping is O(1) per step | property | `cargo test -p whim scroll::` | Wave 0 |
| CORE-05, HEX-03 | Ctrl+G parser grammar table (line / 0x / # / % / hex-mode bare number / invalid / clamp) | unit | `cargo test -p whim goto::` | Wave 0 |
| CORE-07 | Cancel flag respected by index/copy workers (`join` within 100 ms of cancel) | unit | `cargo test -p whim workers::cancel` | Wave 0 |
| EDIT-05/06 | wrap auto-off threshold logic (64 MB), toggle state machine, gutter width monotonic | unit | `cargo test -p whim settings::` | Wave 0 |
| EDIT-10 | Selection offsets <-> char map; word/line bounds; copy size tier (<=16 MiB / confirm / >256 MiB); lossy conversion | unit | `cargo test -p whim selection:: copy::` | Wave 0 |
| UI-05 | `decode_paragraph`: tabs, controls, invalid bytes -> `·`, map monotone; font provider install guard (mock `has_glyph`) | unit | `cargo test -p whim text::` | Wave 0 |
| HEX-01 | Hex row formatting (offset/groups/ASCII), hit-test arithmetic, selection->text `4C 9A FF` | unit | `cargo test -p whim view::hexview::` | Wave 0 |
| HEX-05 | Binary heuristic table (NUL, 30 % threshold, empty, text) | unit | `cargo test -p whim open::binary` | Wave 0 |
| OPEN-02/09 | Path classification used by drop & dialog results; real DnD/dialog = manual | unit + manual UAT | `cargo test -p whim open::` | Wave 0 / UAT |
| UI-01 | Status-bar segment collapse order by width (pure fn) | unit | `cargo test -p whim status::` | Wave 0 |
| D-17/D-18 | Size <= 10 MB, startup/footprint budgets | CI | `cargo xtask check-size --max-mb 10`; `cargo xtask measure --runs 10 ...` | exists |

### Sampling Rate
- **Per task commit:** `cargo test -p whim --lib <touched module>::` + `cargo clippy -p whim --all-targets -- -D warnings`
- **Per wave merge:** full suite command above
- **Phase gate:** full suite green + CI `gate` job (size hard; macOS startup/footprint hard) + local `cargo xtask measure --file <20GiB sparse>` appended to `docs/measurements.md`, before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `src/source.rs` tests (+ temp-file helper with unique dir)
- [ ] `src/scroll.rs` property tests with a test-local xorshift generator (no new crate)
- [ ] `src/lineindex.rs` reference implementation (naive counter) for comparison
- [ ] Binary/CRLF/CJK/invalid-UTF-8/long-line fixture generators in a shared `#[cfg(test)] mod fixtures`
- [ ] Manual UAT checklist file (macOS dialog + clipboard + DnD, Linux portal dialog, Wayland limitation, CJK display, mixed-DPI carried over)

## Security Domain

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — |
| V3 Session Management | no | — |
| V4 Access Control | partial | Read-only open; never write to the user's file in Phase 2; OS permissions respected, errors shown |
| V5 Input Validation | yes | Files are untrusted bytes: bounds-checked `u64` arithmetic, typed errors, no `unwrap` on I/O, Ctrl+G parser is total (returns `Err`), drop/dialog paths classified before open |
| V6 Cryptography | no | — |
| V12 Files and Resources | yes | Regular-file check before open (FIFO/device/dir), no whole-file reads, caps (256 MiB copy, 16-block cache, paragraph < 20 KB), no execution of file content, no path-derived shell commands |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Hostile file to exhaust memory (multi-GB line, billions of lines) | DoS | Paragraph cap, sparse index (8 B/64 KiB), bounded caches |
| File truncated/replaced while open (TOCTOU) | Tampering/DoS | Positional reads + identity check; `Changed` error instead of UB (mmap removed) |
| Opening FIFO/device (`/dev/zero`, named pipe) hangs UI | DoS | stat + `is_file()` guard, open on worker |
| Malicious path from drop/CLI (symlink to device, huge) | Tampering | Same classification; symlinks are followed by `metadata` (stat), so a symlink to a FIFO is rejected correctly |
| Terminal/control characters in file content | Spoofing | Rendered as `·`, never interpreted; no ANSI handling |
| Bidi/RLO characters spoofing display order | Spoofing | Out of scope for a viewer in Phase 2 [ASSUMED]; note for Phase 7 Markdown |
| Clipboard exhaustion | DoS | 16 MiB/256 MiB caps with confirm; worker thread, cancellable |
| Integer overflow on offsets (`u64` near 2^63, 8 EiB sparse) | Tampering | `checked_*`/`saturating_*`; clamp all targets to `len` |
| Supply chain (new crates) | Tampering | `Cargo.lock` committed, CI uses `--locked`; crates from egui/rfd maintainers; review `cargo tree` delta (fontique, harfrust/vello_cpu are already in egui 0.37's tree) |
| Worker panic with `panic = "abort"` | DoS | no `unwrap`/`expect`/slice-index panics on file-derived data in workers |

## Recommended Plan / Wave Breakdown

**Wave 0 - Setup (2 plans, parallel)**
- 02-01: Cargo deps (+memchr, rfd, egui_system_font_provider, eframe `clipboard`; -memmap2), `NativeOptions.system_font_fallback=false`, theme consts from UI-SPEC, CI D-18 change (+ macOS `--file` sparse step), test fixtures module, size/startup re-measure placeholder in `docs/measurements.md`. Satisfies D-18, D-17 baseline.
- 02-01b (small task, may be its own plan, parallel with 02-01): D-21 macOS bundle: `cargo xtask bundle-macos` (std::fs only, pure `info_plist()` + layout unit tests on all OSes), macOS-only CI steps (`codesign --force --deep -s -` + verify, `ditto -c -k --keepParent` zip, exec-bit assertion, smoke launch, artifact upload), README/PERF-04 wording update for macOS 15+ (Open Anyway / `xattr -dr com.apple.quarantine`), Finder double-click UAT item. See Pattern 10.

**Wave 1 - Pure core, parallelizable (3 plans, no egui)**
- 02-02 Storage: `source.rs` (`TextSource`, `FileSource`, `read_full_at`, chunk cache, `Changed`), `open.rs` (classification, open worker, first-block sample, binary heuristic, EOL sample), remove `file.rs`. CORE-01/02, D-01/02/19, HEX-05 logic.
- 02-03 Line index: `lineindex.rs` (blocks, worker, cancel, throttle, mapping, estimate, goto parser incl. grammar). CORE-03/04/05/07, HEX-03 parsing.
- 02-04 Scroll/paragraph: `scroll.rs` boundary rule + inverse property tests, `text.rs` decode+map, `selection.rs`. CORE-06, EDIT-10 model.

**Wave 2 - Views (2 plans, parallel after Wave 1)**
- 02-05 Text view: gutter, wrapped galleys + own layout cache, hscroll, byte-proportional scrollbar, selection painting/hit test, keyboard nav; replaces `view/mod.rs`+`rows.rs`. EDIT-05/06, CORE-03/06, D-04/05/06/20.
- 02-06 Hex view: rows, columns, hit test, selection, text<->hex position mapping. HEX-01/03.

**Wave 3 - Shell and integration (2 plans)**
- 02-07 App shell: menus (File>Open, View items), tab strip, banner, Ctrl+G box, status bar segments with collapse, progress + cancel + Esc priority, DnD overlay, rfd thread, error/“changed on disk” states, copy confirm dialog + copy worker. UI-01, OPEN-02/09, CORE-07, EDIT-10, D-11..D-16.
- 02-08 Fonts + footprint: lazy font provider (UI-05), CJK fixture, 20 GiB measurements (startup, footprint, idle CPU, size) appended to `docs/measurements.md`, minified-file frame-time check, UAT checklist.

Rationale: Wave 1 plans touch disjoint files and are fully unit-testable on any OS; Wave 2 depends only on their interfaces (`TextSource`, `LineIndex`, `scroll`, `text`); the shell last since it wires everything. The CJK/footprint plan is last because it needs the final painters.

## Sources

### Primary (HIGH confidence)
- `~/.cargo/registry/src/.../epaint-0.37.0` (font_provider.rs, galley_cache.rs, text_layout_types.rs, cursor.rs, tessellator.rs) - FontProvider semantics, Galley/CCursor char-index, culling
- `.../egui-0.37.0` (context.rs `add_font_provider`/`copy_text`/`fonts_mut`; data/input/dropped_file.rs, raw_input.rs)
- `.../egui-winit-0.37.0/src/clipboard.rs` and `lib.rs` - clipboard fallback; DnD event mapping
- `.../eframe-0.37.0` (Cargo.toml features, epi.rs `system_font_fallback`, native/epi_integration.rs)
- `egui_system_font_provider-0.37.0` source (crates.io static download) - mmap, lazy thread, locale logic
- `rfd-0.17.2` source and Cargo.toml - features, macOS `run_on_main`, libdbus dlopen
- `memchr-2.8.3` source - specialised `count`
- https://doc.rust-lang.org/std/os/unix/fs/trait.FileExt.html and https://doc.rust-lang.org/std/os/windows/fs/trait.FileExt.html - read_at / seek_read semantics
- Local spike build `/tmp/size-spike` (Windows release sizes) and repo `cargo tree`

### Secondary (MEDIUM confidence)
- WebSearch 2026-10: winit 0.30 Wayland has no file DnD (rust-windowing/winit#1881; egui issue #8805; winit 0.31 adds it)

### Tertiary (LOW confidence)
- Throughput numbers (A1), CJK memory (A4), clipboard transient peak (A9): training knowledge, to be measured.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - versions/features read from sources; size delta measured on Windows only
- Architecture (storage, index, boundary rule): HIGH on design/logic (derivable, property-testable), MEDIUM on performance numbers
- egui layout/selection details: MEDIUM - API names verified in source for most, `TextFormat::line_height` and glyph-width helper names still to confirm
- Pitfalls: HIGH for clipboard feature, DroppedFile::bytes, stub font provider; MEDIUM for macOS/Wayland behaviours

**Research date:** 2026-10-31
**Valid until:** 2026-11-30 (egui 0.37 line is current; re-check if eframe/winit bump)
