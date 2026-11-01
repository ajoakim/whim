<!-- GSD:project-start source:PROJECT.md -->
## Project

**FastNotes**

A small, lightweight, cross-platform desktop file editor in the spirit of Notepad++, with a built-in Markdown renderer and hex view/byte editing. It is meant to be the user's go-to app for quickly opening and viewing (and editing) files — especially downloaded files and very large files (tens of GB) that conventional editors choke on.

**Core Value:** Any file, of any size, opens instantly and can be viewed and edited without the app ever loading the whole file into memory.

### Constraints

- **Tech stack**: Rust — no GC, mmap-friendly, small static binaries; Go GUI options are heavier or webview-based
- **Platforms**: Windows, Linux, macOS — all three are first-class
- **Performance**: < ~200 ms cold startup; < 1 s to first page of a 20 GB file; idle RAM in tens of MB
- **Footprint**: single portable binary on Windows/Linux, target < ~15 MB, no runtime/installer dependency; macOS ships as an unsigned `.app` bundle (Apple requires a bundle for "Open with"; signing deferred to v2)
- **Memory model**: never read a whole file into memory; all views (text, hex, search) must work over mmap/chunks
- **UI**: minimal — tabs + editor + status bar, menus for everything else
<!-- GSD:project-end -->

<!-- GSD:stack-start source:research/STACK.md -->
## Technology Stack

## Recommended Stack
### Core Framework
| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| Rust (edition 2024, stable) | current stable | Language | Fixed by project decision | HIGH |
| eframe + egui (+ egui-winit) | 0.37.0 | GUI shell, input, painting | Immediate-mode with a direct painter API. You draw only the visible rows of the text and hex views yourself, so virtualization is trivial and nothing is retained per line. It repaints only on events, so idle CPU is about 0 and idle RAM is small. It handles drag & drop (`raw_input.dropped_files` gives native paths), clipboard (arboard), a winit window and AccessKit. Small dependency tree compared with Slint or iced, and the egui ecosystem is the biggest of the Rust GUI options. | MEDIUM (size and startup need a spike) |
| eframe backend: **glow** (OpenGL), not wgpu | `default-features = false, features = ["glow", "default_fonts"?, "x11", "wayland", "accesskit"]` | Rendering | wgpu adds several MB of binary and slower startup (adapter and device init, shader compile). glow starts faster and is smaller. Plain 2D text and rects don't need wgpu. Caveat: OpenGL on old or VM Windows machines. Keep wgpu as a fallback behind a cargo feature only if needed. | MEDIUM |
| winit (transitive) | 0.30.x (via egui-winit) | Window/events | Comes with eframe. Do not depend on it directly. | HIGH |
- **Slint 1.18.1**: Strong startup and footprint, but the text editing widget is its own model. A custom virtualized 20 GB text view with selection and IME means fighting the toolkit. Its license is GPLv3, royalty-free, or commercial, which adds friction. Its declarative `.slint` layer is extra surface for a "tabs + canvas + status bar" app. Runner-up.
- **iced 0.14.0**: Retained Elm architecture, wgpu by default (larger, slower start). Custom canvas widgets are possible but heavier. Its text stack (cosmic-text) is good, but nothing there matches the need for a mmap-backed buffer.
- **Xilem 0.4.0 / Floem 0.2.0**: Pre-1.0, with API churn and weak accessibility and IME maturity. Reject for a daily-driver tool.
- **Native wrappers (win32/Cocoa/GTK per OS)**: Three UI codebases. This breaks "minimal and lightweight to build". Reject.
- **Tauri/webview**: Violates the footprint and RAM targets.
- IME: egui handles `Event::Ime` (preedit/commit), but candidate-window positioning needs `ctx.output_mut(|o| o.ime = Some(..))` set by hand. Because the editor view is custom, you own all text input handling (caret, selection, composition) and cannot use `TextEdit`. LOW-MEDIUM confidence that CJK IME works well. Test on Windows and macOS early.
- Accessibility: AccessKit support is built in, but a custom-painted text area exposes nothing to screen readers unless you add AccessKit nodes manually. Treat full accessibility as best-effort. Menus, tabs and the status bar get it for free.
- Text layout: egui's `Galley` layout is per line. Lay out only visible lines, and cap the per-line layout width (hard-wrap or truncate lines over about 10k chars). A 2 GB single-line file will otherwise kill you.
### Buffer / Storage (custom; no off-the-shelf crate fits)
| Technology | Version | Purpose | Why | Confidence |
|------------|---------|---------|-----|------------|
| memmap2 | 0.9.12 | Read-only mapping of the original file | The only maintained mmap crate. 64-bit address space maps 20 GB+ without loading it. | HIGH |
| **Custom piece table** (own module) | n/a | Edit model | Pieces are `(source, offset, len)`, where source is either the mmap'd original or an append-only "add" buffer. Edits are O(log n) in pieces, and RAM is proportional to the number of edits, not file size. Back the piece list with a balanced tree or a B-tree. Do not use a Vec if heavy edit counts are expected. | HIGH on approach, MEDIUM on implementation effort |
| memchr | 2.x | SIMD newline scan / literal search | Line indexing and literal find at memory-bandwidth speed | HIGH |
| Sparse line index (custom) | n/a | Line to byte offset | A background thread scans for `\n` and stores checkpoints every N lines (or every 64 KB). Jump-to-line reads from the nearest checkpoint. Never store one offset per line (a 20 GB log has billions). Piece-table edits adjust checkpoint deltas. | MEDIUM |
| tempfile (or manual) | 3.x | Save via temp + atomic rename | See pitfall below | HIGH |
- **ropey 1.6.1 and crop 0.4.3** store text as owned chunks in memory. They have no mmap-backed leaf. Loading a 20 GB file into a rope violates the core value. Either could serve only as a small-file fast path, but two buffer implementations double the bug surface. Skip them.
- **xi-rope 0.3.0**: Abandoned upstream, and also in-memory.
- **piece-table 0.1.0**: A toy crate. Don't use it.
### Encoding
| Library | Version | Purpose | When to Use | Confidence |
|---------|---------|---------|-------------|------------|
| encoding_rs | 0.8.44 (verify, latest stable on crates.io) | Decode/encode windows-1252, UTF-16, Shift_JIS, GBK etc. | Decode only the visible viewport slice (and encode on save). Use `Decoder` with chunk boundaries aligned. | HIGH |
| chardetng | 1.0.0 | Detect legacy encoding | Run on the first ~64 KB (plus maybe a middle sample). Check BOM and strict UTF-8 validation first, and only then use chardetng for non-UTF-8 content. | HIGH |
### Syntax Highlighting
| Library | Version | Purpose | Why | Confidence |
|---------|---------|---------|-----|------------|
| syntect | 5.3.0, `default-features = false, features = ["default-syntaxes", "default-themes", "regex-fancy", "parsing"]` | Viewport highlighting | Pure Rust with `fancy-regex` and no C dependency (no Oniguruma). It loads precompiled `.packdump` quickly, and Sublime syntaxes cover common languages. Highlight state is a small per-line `ParseState`/`HighlightState`, so you can cache a checkpoint every ~500 lines and re-parse from the nearest one. Expected binary cost is about 1-3 MB with the bundled syntaxes (MEDIUM). Load syntaxes lazily and cache the dump. | MEDIUM |
| two-face 0.5.2 | optional | Extra syntaxes (TOML, TypeScript, etc.) from bat | Add only if default syntaxes lack languages users want. Adds size. | LOW-MEDIUM |
### Search
| Library | Version | Purpose | When to Use | Confidence |
|---------|---------|---------|-------------|------------|
| regex (`regex::bytes`) | 1.13.1 | Regex find/replace | Run `bytes::Regex` over chunks of the piece-table stream (e.g. 1-8 MB) with an overlap region sized to the max match length (or to line boundaries). Disable unneeded Unicode features to cut size: `default-features = false, features = ["std", "perf", "unicode"]` (drop `unicode` pieces only if acceptable). | HIGH |
| memchr (memmem) | 2.x | Literal / case-sensitive plain search | Fast path when the query has no regex metacharacters | HIGH |
| regex-automata | 0.4.18 | Only if you need streaming/lazy DFA control | Not needed initially. `regex` is sufficient. | MEDIUM |
| grep-searcher | 0.1.17 | (Not recommended) | It is built around line-oriented, file/mmap sources and does not understand a piece table. Reimplementing the chunk loop is about 100 lines and you control cancellation and progress. | MEDIUM |
### Markdown
| Library | Version | Purpose | Why | Confidence |
|---------|---------|---------|-----|------------|
| pulldown-cmark | 0.13.6 (`default-features = false`) | CommonMark + GFM-ish event parser | Pull parser, small, fast, no AST allocation. Feed events into a small custom egui renderer (headings, emphasis, lists, code blocks, tables, links, images). | HIGH |
| egui_commonmark | 0.26.0 | Optional shortcut renderer | Built on pulldown-cmark. Verify it targets egui 0.37 before use (versions track egui releases and may lag by weeks). If it matches, use it to ship v1 faster, with `default-features = false` to avoid syntect, image and svg extras. Otherwise write the ~300-line renderer. | LOW-MEDIUM (compat unverified) |
### Hex View
### OS Integration
| Library | Version | Purpose | Why | Confidence |
|---------|---------|---------|-----|------------|
| interprocess | 2.4.7 (local sockets) | Single-instance + forward `open <path>` to running instance | Named pipe on Windows, Unix socket on Linux/macOS. Second launch connects, sends paths, exits in a few ms. `single-instance` 0.3.3 only holds a lock and cannot pass paths, so it is insufficient. | MEDIUM |
| lexopt | 0.3.2 | CLI args | Tiny (clap 4.6.7 costs several hundred KB). Args are just `[files...]`, plus `--help`/`--version`. | HIGH |
| rfd | 0.17.2 | Native open/save dialogs | Native dialogs. On Linux prefer the xdg-portal backend (no GTK link). | MEDIUM |
| directories | 6.0.0 | Config/data dirs | Standard paths for config and session | HIGH |
| serde + serde_json | 1.0.229 / 1.0.154 | Config, recent files, session (tabs, scroll, caret) | One format for all persisted state. Use `toml` 1.1.8 only if you want human-edited config. Write atomically (temp + rename). | HIGH |
| winreg | 0.5x (verify) | Windows file association | Register `HKCU\Software\Classes` keys at runtime from a menu item ("Set as default for ..."). This needs no installer or admin rights and suits a portable exe. Windows 10+ still requires the user to confirm the default app in Settings; you can only register as a candidate and open the Settings page. | MEDIUM |
- **Windows**: Registry ProgId entries under HKCU (above); the OS passes the path as argv[1]. Set `#![windows_subsystem = "windows"]` (no console) and embed the icon/manifest (`embed-resource` or `winresource` build dep).
- **Linux**: Ship a `.desktop` file with `MimeType=` and `Exec=fastnotes %F`. A "portable" single binary cannot self-register without writing to `~/.local/share/applications` (can do on request).
- **macOS**: Requires an `.app` bundle with `CFBundleDocumentTypes` in Info.plist. Files opened from Finder or the Dock arrive as Apple Events (`application:openURLs:`), **not argv**. winit 0.30 does not surface this event cleanly. Plan for a small `objc2` delegate hook or a newer winit. This is the highest-risk integration item (LOW confidence). Package with `cargo-packager` or `cargo-bundle`. "Single portable binary" on macOS in practice means a single `.app`.
### Infrastructure / Release
- Add `-C target-cpu` only for local builds. Do not ship `native`.
- Windows: `RUSTFLAGS=-C target-feature=+crt-static` so there is no VC++ redistributable dependency.
- Linux: link glibc dynamically (X11/Wayland/GL are dlopen'd at runtime by winit/glow). musl static builds break GL loading. Build on an old glibc baseline (e.g. an Ubuntu 22.04 runner) for portability.
- macOS: universal2 (aarch64 + x86_64) build is roughly 2x size. Consider shipping per-arch.
- Optional: `cargo-bloat` and `cargo bloat --crates` in CI to watch size. `upx` is not recommended (AV false positives, slower cold start).
- Expect roughly 6-10 MB stripped for eframe/glow + syntect (MEDIUM; confirm in Phase 0). The 15 MB budget is plausible but not guaranteed. Measure the walking skeleton first.
### Supporting Libraries
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| memchr | 2.x | SIMD scans | Newline indexing, literal search |
| crossbeam-channel (or std mpsc) | 0.5 | Worker to UI messaging | Use std mpsc unless you need select |
| ctx.request_repaint() | (egui) | Wake UI from workers | After background results |
| fs4 | 1.1.0 | Advisory locks | Optional; skip in v1 |
| arboard | 3.7.0 | Clipboard | Already pulled in by egui-winit. Don't add it directly. |
## Alternatives Considered
| Category | Recommended | Alternative | Why Not |
|----------|-------------|-------------|---------|
| GUI | egui/eframe 0.37 (glow) | Slint 1.18.1 | Custom virtualized text view fights the toolkit. License friction. |
| GUI | egui/eframe | iced 0.14 | wgpu-first, heavier, retained model |
| GUI | egui/eframe | Xilem 0.4 / Floem 0.2 | Pre-1.0 churn, immature a11y/IME |
| Renderer | glow | wgpu | Bigger binary and slower init |
| Buffer | Custom piece table over memmap2 | ropey 1.6.1 / crop 0.4.3 | In-memory only. Loading 20 GB violates the core value. |
| Syntax | syntect (fancy-regex) | tree-sitter 0.27 | Per-language C grammars, size, needs full tree |
| Syntax | syntect fancy-regex | syntect onig | C dependency, harder cross-compile |
| Search | regex::bytes + custom chunk loop | grep-searcher | Line-oriented, not piece-table aware |
| Markdown | pulldown-cmark | comrak 0.59 | Full AST and HTML-oriented, heavier |
| Single instance | interprocess | single-instance 0.3.3 | Lock only, can't forward paths |
| CLI | lexopt | clap 4.6 | Size for trivial needs |
| Hex | Custom | egui_memory_editor etc. | Limited control. The custom view is small. |
## Installation
# Windows only
# Dev / CI
## Sources
- crates.io API, queried 2026-10-31 for all version numbers (HIGH): eframe/egui 0.37.0 (published 2026-10-15), iced 0.14.0, slint 1.18.1, xilem 0.4.0, floem 0.2.0, memmap2 0.9.12, ropey 1.6.1, crop 0.4.3, xi-rope 0.3.0, syntect 5.3.0, tree-sitter 0.27.1, regex 1.13.1, regex-automata 0.4.18, grep-searcher 0.1.17, chardetng 1.0.0, encoding_rs 0.8.44, pulldown-cmark 0.13.6, comrak 0.59.0, egui_commonmark 0.26.0, interprocess 2.4.7, single-instance 0.3.3, rfd 0.17.2, directories 6.0.0, lexopt 0.3.2, clap 4.6.7, winit 0.30.13.
- Context7, official docs and benchmarks were NOT consulted in this pass. Claims about startup time, binary size, IME quality, wgpu vs glow, the macOS Apple Event path and egui feature names come from training knowledge and are MEDIUM/LOW. A Phase 0 spike must validate them: a walking skeleton that mmaps a 20 GB file, paints the first page, and records cold start, idle RSS and stripped size on all three OSes.
<!-- GSD:stack-end -->

<!-- GSD:conventions-start source:CONVENTIONS.md -->
## Conventions

Conventions not yet established. Will populate as patterns emerge during development.
<!-- GSD:conventions-end -->

<!-- GSD:architecture-start source:ARCHITECTURE.md -->
## Architecture

Architecture not yet mapped. Follow existing patterns found in the codebase.
<!-- GSD:architecture-end -->

<!-- GSD:skills-start source:skills/ -->
## Project Skills

No project skills found. Add skills to any of: `.claude/skills/`, `.agents/skills/`, `.cursor/skills/`, `.github/skills/`, or `.codex/skills/` with a `SKILL.md` index file.
<!-- GSD:skills-end -->

<!-- GSD:workflow-start source:GSD defaults -->
## GSD Workflow Enforcement

Before using Edit, Write, or other file-changing tools, start work through a GSD command so planning artifacts and execution context stay in sync.

Use these entry points:
- `/gsd-quick` for small fixes, doc updates, and ad-hoc tasks
- `/gsd-debug` for investigation and bug fixing
- `/gsd-execute-phase` for planned phase work

Do not make direct repo edits outside a GSD workflow unless the user explicitly asks to bypass it.
<!-- GSD:workflow-end -->



<!-- GSD:profile-start -->
## Developer Profile

> Profile not yet configured. Run `/gsd-profile-user` to generate your developer profile.
> This section is managed by `generate-claude-profile` -- do not edit manually.
<!-- GSD:profile-end -->
