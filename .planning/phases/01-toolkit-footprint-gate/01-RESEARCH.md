# Phase 1: Toolkit & Footprint Gate - Research

**Researched:** 2026-10-31
**Domain:** Rust GUI skeleton (eframe/egui 0.37, glow vs wgpu), mmap of huge sparse files, footprint/startup/idle measurement harness, cross-OS CI
**Confidence:** MEDIUM-HIGH (eframe/egui APIs read directly from the 0.37.0 crate sources; CI GL behavior on Windows/macOS runners is MEDIUM/LOW and must be proven by the first CI run)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Linux/macOS numbers come from a GitHub Actions matrix (windows-latest, ubuntu-22.04, macos-latest) that records stripped size, time-to-first-frame, idle RSS and idle CPU as artifacts; Windows is also measured locally. The repo currently has no git remote - the user will push to GitHub for CI.
- Startup: the app self-reports time from process start to first painted frame; a harness runs N times and reports the median. CI measures warm start only (true cold cache is not reproducible there); local runs note cold vs warm.
- glow vs wgpu: both are cargo features of the same skeleton; measure both, glow is the default.
- Toolchain: pin current stable via `rust-toolchain.toml`, edition 2024. Local rustc is 1.69 and must be upgraded (`rustup update stable`) - eframe 0.37/edition 2024 need >= 1.85.
- The skeleton is a real seed crate (`whim`) that later phases grow, not a throwaway spike: eframe window, menu bar stub, status bar, virtualized monospace view painting only visible rows from a memmap2 mapping.
- Positioning: byte-offset virtual scroll with fixed row height; no sparse line index yet (Phase 2). First page reads only the visible bytes.
- Fonts: egui default fonts plus one bundled monospace font; no CJK fallback (UI-05 is later).
- A cross-platform sparse-file helper (xtask or test util) creates 20 GB sparse files via `set_len` + a few written markers.
- GO if: startup <= 300 ms (target 200), stripped binary <= 10 MB at skeleton stage, idle RSS <= 60 MB on all OSes. NO-GO triggers a Slint evaluation.
- CI fails on budget regressions: binary size is a hard limit; startup and RSS use generous thresholds because of runner noise.
- IME: wire `Event::Ime` preedit/commit and IME cursor positioning in the skeleton; manually test Windows Japanese IME; document supported scope, marking macOS/Linux unverified if untested.
- Go/no-go doc: `.planning/phases/01-toolkit-footprint-gate/01-GATE.md`, plus a `docs/` measurements table that later phases keep updating.

### Claude's Discretion
- Crate/module layout, harness implementation language (Rust xtask preferred), exact CI workflow structure, HiDPI verification method.

### Deferred Ideas (OUT OF SCOPE)
- CJK font fallback (UI-05) - later phase.
- Sparse line index - Phase 2.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PERF-01 | Window appears in ~200 ms on cold start | Startup measurement design (harness-side + in-app), hidden-until-first-paint behaviour of eframe, glow vs wgpu init cost |
| PERF-02 | Idle RAM tens of MB, idle CPU ~0 | Event-driven repaint policy, sysinfo RSS / accumulated CPU time sampling, mmap RSS caveats |
| PERF-05 | Runs on Windows, Linux, macOS | CI matrix, Linux Xvfb+Mesa setup, Windows/macOS GL caveats, per-OS sparse file creation |
| UI-04 | Text crisp on HiDPI / mixed-DPI | `pixels_per_point`, `ScaleFactorChanged` handling, pixel-snapping, manual verification procedure |
</phase_requirements>

## Summary

eframe/egui 0.37.0 (published 2026-10-15) is current. Its MSRV is **Rust 1.95** (not 1.85 as CONTEXT.md says) [VERIFIED: crates.io API rust_version=1.95 for eframe 0.37.0 and sysinfo 0.40.1]. The local machine's default `stable` toolchain is still 1.69, **but rustup already has a side-by-side `1.99.0` toolchain installed** (rustc 1.99.0, 2026-09-28) plus a nightly [VERIFIED: `rustup show`, `rustc --version` from the 1.99.0 toolchain dir]. So the toolchain "upgrade" is just a `rust-toolchain.toml` with `channel = "1.99.0"`; `rustup update stable` is optional. Pinning an exact version (not `stable`) keeps CI and local identical.

Several egui 0.37 API facts differ from what STACK.md and UI-SPEC.md assume, and the planner must use the new names: (1) `eframe::App` now has `fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame)` plus an optional `fn logic(&mut self, ctx, frame)`; there is no `update` method [VERIFIED: eframe-0.37.0 src/epi.rs]. (2) `TopBottomPanel`/`SidePanel` are replaced by `egui::Panel::top(id)` / `Panel::bottom(id)` with `.show(ui, ...)` (`show_inside` is deprecated); `CentralPanel::show(ui, ...)` likewise; menus use `egui::MenuBar::new().ui(ui, |ui| ...)` [VERIFIED: egui-0.37.0 containers/panel.rs, menu.rs]. (3) **eframe's `default` feature set includes `wgpu`, not glow**; with `default-features = false` you must list everything explicitly, and **if both `glow` and `wgpu` features are compiled in, `NativeOptions::renderer` defaults to Wgpu**, so the skeleton must set `renderer` explicitly [VERIFIED: eframe Cargo.toml + `impl Default for Renderer`]. (4) The IME events are `Event::Ime(ImeEvent::Preedit { text, active_range_chars })`, `ImeEvent::Commit(String)`, `ImeEvent::DeleteSurrounding{..}`; `Enabled`/`Disabled` are deprecated and no longer emitted. The output struct is `egui::IMEOutput { purpose, rect, cursor_rect, should_interrupt_composition }` set via `ctx.output_mut(|o| o.ime = Some(..))` - UI-SPEC's two-field literal is missing `purpose` and `should_interrupt_composition` [VERIFIED: egui-0.37.0 data/output.rs, ime_event.rs, egui-winit lib.rs].

The biggest non-API risk is CI, not the toolkit: GitHub's `windows-latest` runner has no GPU, so glow (WGL) falls back to Microsoft's GL 1.1 software renderer and egui_glow cannot run on it. CI needs either Mesa's llvmpipe `opengl32.dll` dropped beside the exe, or measuring wgpu (DX12 WARP / Vulkan lavapipe) there. Windows numbers from CI therefore measure a software renderer and must be labelled as such; the authoritative Windows glow-on-real-GPU number comes from the local machine. Second biggest: creating the 20 GB test file on NTFS without `FSCTL_SET_SPARSE` makes any write near the end force Windows to zero-fill the whole gap (tens of GB of disk writes, minutes) and consumes real disk, so the sparse flag is mandatory, not optional.

**Primary recommendation:** Build a single `whim` crate (+ `xtask` workspace member) on eframe 0.37 with `default-features = false, features = ["glow", "default_fonts", "x11", "wayland"]` and an opt-in `wgpu` cargo feature; select renderer explicitly at runtime from a compile-time cfg; measure startup in the harness (spawn timestamp vs in-app wall-clock first-frame timestamp), idle RSS/CPU via `sysinfo`; on Windows mark the file sparse via `DeviceIoControl(FSCTL_SET_SPARSE)` before `set_len`.

## Architectural Responsibility Map

Desktop single-process app; tiers are in-process layers rather than web tiers.

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Window, event loop, DPI events | eframe/winit (shell) | - | Owned by toolkit; app only reads `pixels_per_point` |
| Virtualized row painting | UI layer (`Painter::galley` per visible row) | Storage layer supplies bytes | Only visible rows laid out; nothing retained per line |
| File access (mmap, slice for visible rows) | Storage layer (`memmap2`) | - | Never read whole file; reads touch only visible pages |
| Byte-offset scroll model | UI layer (state = `u64` offset) | - | No line index in Phase 1 |
| IME composition handling | UI layer (reads `Event::Ime`, writes `IMEOutput`) | egui-winit (translates winit IME) | App owns caret/preedit rendering for the custom view |
| Startup/RSS/CPU measurement | xtask harness (separate process) | App `--bench` flag prints timestamps | Out-of-process sampling avoids perturbing the app |
| Budget enforcement | CI (GitHub Actions) calling xtask | - | Hard size limit, soft startup/RSS limits |
| Sparse test-file creation | xtask (per-OS code) | - | Needs OS APIs on Windows |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| eframe | 0.37.0 (2026-10-15) | Window + glow/wgpu integration | Locked by project; MSRV 1.95 [VERIFIED: crates.io] |
| egui | 0.37.0 | Immediate-mode UI, `Painter` | Same release train [VERIFIED: crates.io] |
| memmap2 | 0.9.12 | Read-only mmap of the 20 GB file | [VERIFIED: crates.io] |
| lexopt | 0.3.2 | CLI args (`<path>`, `--bench`, `--bench-exit-after`) | Locked by STACK.md |

eframe Cargo line (explicit; do not use default features because they pull wgpu, accesskit, web_screen_reader, system_fonts, links):
```toml
eframe = { version = "0.37.0", default-features = false, features = ["glow", "default_fonts", "x11", "wayland"] }
# optional wgpu comparison build:
# [features] wgpu = ["eframe/wgpu"]     # enables both backends; renderer must be chosen at runtime
```
Features that exist in 0.37.0: `accesskit, clipboard, default_fonts, glow, links, system_fonts, wayland, wgpu, wgpu_no_default_features, x11, persistence, inspection, experimental` [VERIFIED: crates.io features for eframe 0.37.0]. Notes:
- `clipboard` and `accesskit` are separate features now; STACK.md's list omitted them. Add `clipboard` only if Phase 1 needs copy (it does not); add `accesskit` later (costs size) - measure it as a delta in the size table.
- `wgpu_no_default_features` exists for a leaner wgpu build (skips egui-wgpu defaults); use `eframe/wgpu` for the comparison build as the "default" wgpu config users would get, and optionally `wgpu_no_default_features` as a second data point.
- `system_fonts` (new in 0.37) pulls `egui_system_font_provider`; leave OFF (UI-05 deferred; keeps size down).
- eframe unconditionally depends on `image` with `png` feature (icon decoding) and on `windows-sys`/`objc2*` on those OSes; unavoidable in size baseline.
- Because winit is `default-features = false` in eframe and `x11`/`wayland` are forwarded to egui-winit/glutin, a Linux build without those features has no window backend. Keep both.

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| sysinfo | 0.40.1 (MSRV 1.95) | Process RSS (`Process::memory()`), `accumulated_cpu_time()`, spawn/kill in xtask | Harness only; not a dependency of the app [VERIFIED: crates.io, src/common/system.rs] |
| windows-sys | 0.61.x | `DeviceIoControl` / `FSCTL_SET_SPARSE` in xtask (Windows only) | Sparse file creation. Features: `Win32_Foundation`, `Win32_System_IO`, `Win32_System_Ioctl`, `Win32_Storage_FileSystem` [ASSUMED feature names - confirm on first build; same crate version already pulled by eframe] |
| cargo-bloat | latest | `cargo bloat --release --crates` in CI for size breakdown | Informational artifact, not a gate |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| windows-sys FSCTL in xtask | Shell out to `fsutil sparse setflag <file>` on Windows | Zero deps, but depends on fsutil being present and (historically) elevated for some subcommands; `setflag` on a user file works unelevated [ASSUMED]. Use as fallback if the windows-sys path is troublesome |
| sysinfo in xtask | Raw OS APIs (/proc, GetProcessMemoryInfo, task_info) | More code per OS; sysinfo already abstracts all three |
| `JetBrains Mono` bundled | egui `default_fonts` Hack (monospace) | UI-SPEC allows swap "if binary-size budget demands"; Hack already in egui = 0 extra bytes. Recommend: start with Hack for the gate measurement, bundle JetBrains Mono as a separate measured delta (~200 KB) |

**Installation:**
```bash
# toolchain: 1.99.0 is already installed locally; otherwise `rustup toolchain install 1.99.0`
cargo new --vcs none whim   # (planner: create workspace manually; see structure below)
cargo add eframe@0.37.0 --no-default-features --features glow,default_fonts,x11,wayland
cargo add memmap2@0.9.12 lexopt@0.3.2
```

**Version verification:** versions above were read live from the crates.io API on 2026-10-31 (eframe/egui/egui-winit/egui_glow 0.37.0, memmap2 0.9.12, sysinfo 0.40.1).

## Architecture Patterns

### System Architecture Diagram

```
 CLI: whim <path> [--bench] [--bench-exit-after N]
        |
        v
 main(): record t0 (Instant + SystemTime) --> parse args (lexopt) --> open file + memmap2 (map only; no read)
        |
        v
 eframe::run_native(NativeOptions{renderer: Glow|Wgpu (explicit), viewport: 1024x720, min 480x320})
        |   (window hidden until first frame painted: eframe sets visible in post_rendering)
        v
 App::ui(ui, frame)  <----- input events (scroll, keys, Event::Ime) -----+
   |-- Panel::top  : MenuBar (File/View/Help stubs)                       |
   |-- Panel::bottom: status bar (Offset | Size | backend | Startup ms)   |
   |-- CentralPanel : allocate rect -> painter                            |
   |       visible_rows = rect.height/20 (+1)                             |
   |       for each row: slice = &mmap[off..off+128] -> map bytes->chars  |
   |                     painter.galley(row_pos_snapped, layout_no_wrap)  |
   |       IME: read Event::Ime -> preedit/commit probe; set IMEOutput    |
   v                                                                      |
 first ui() completes --> record first-frame timestamp --> print `startup_ms=` / `first_frame_unix_us=` (if --bench)
        |
        v
 idle: no request_repaint => event loop sleeps (0 CPU)  <-- harness samples RSS/CPU from outside

 xtask (separate process): create-sparse -> spawn whim --bench N times -> median startup -> sample idle RSS/CPU -> JSON -> compare against budgets
```

### Recommended Project Structure
```
Cargo.toml              # [workspace] members = ["xtask"], root package = whim
rust-toolchain.toml     # channel = "1.99.0", components = ["clippy","rustfmt"]
.cargo/config.toml      # cargo alias `xtask`; windows crt-static rustflags
src/main.rs             # arg parsing, t0 capture, run_native
src/app.rs              # App impl: panels, menu, status
src/view.rs             # virtualized monospace view (rows, scroll, gutter)
src/ime.rs              # IME probe state + IMEOutput
src/bench.rs            # --bench reporting (startup timestamps)
xtask/src/main.rs       # subcommands: sparse, measure, check-budgets
docs/measurements.md    # standing table (append per phase)
.github/workflows/ci.yml
```

### Pattern 1: Explicit renderer selection with optional wgpu
**What:** `glow` is the only backend feature by default; `--features wgpu` adds wgpu; the app picks renderer by cfg so the choice is never accidental.
**Example:**
```rust
// Source: eframe-0.37.0 src/epi.rs (Renderer enum, Default impl) [VERIFIED]
let renderer = {
    #[cfg(feature = "wgpu")]
    { eframe::Renderer::Wgpu }
    #[cfg(not(feature = "wgpu"))]
    { eframe::Renderer::Glow }
};
let options = eframe::NativeOptions {
    renderer,
    viewport: egui::ViewportBuilder::default()
        .with_title("Whim")
        .with_inner_size([1024.0, 720.0])
        .with_min_inner_size([480.0, 320.0]),
    ..Default::default()
};
eframe::run_native("whim", options, Box::new(|cc| Ok(Box::new(WhimApp::new(cc, ...)))))
```
(The `AppCreator` closure returns `Result<Box<dyn App>, ...>` in recent eframe; confirm exact signature in `cargo doc` on first compile [ASSUMED].) With both features compiled, an un-set `renderer` selects Wgpu. For a pure wgpu comparison build you may instead make `glow` optional too, which removes glow/glutin from that binary and gives a clean size number: define crate features `default = ["glow"]`, `glow = ["eframe/glow"]`, `wgpu = ["eframe/wgpu"]` and build wgpu as `--no-default-features --features wgpu`.

### Pattern 2: 0.37 app skeleton (new API)
```rust
// Source: egui-0.37.0 containers/panel.rs & menu.rs docs; eframe-0.37.0 epi.rs [VERIFIED names]
impl eframe::App for WhimApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::Panel::top("menu").exact_size(24.0).show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Quit").clicked() {
                        ui.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
            });
        });
        egui::Panel::bottom("status").exact_size(24.0).show(ui, |ui| { /* labels */ });
        egui::CentralPanel::default().show(ui, |ui| self.view.show(ui, &self.map));
    }
}
```
`exact_size` on `Panel` takes one f32 (height for top/bottom). The `ctx` is reachable via `ui.ctx()`. Use `logic()` only for non-painting work.

### Pattern 3: Virtualized byte-offset view
**What:** state is `top_offset: u64` and fixed row height 20 pt; per frame compute `rows = ceil(rect.height()/20)+1`, for each row slice `&mmap[off..min(off+128,len)]`, convert bytes to a `String` of printable chars with `·` for non-printables, lay out with `ui.painter().layout_no_wrap(text, font_id, color)` and draw with `painter.galley(pos, galley, color)`.
**Notes:**
- Scrollbar: do not use `ScrollArea` for a 20 GB range (f32 content height loses precision: 2^24 px exactly representable). Handle `input.smooth_scroll_delta` / `raw_scroll_delta` manually and draw a custom thumb mapped linearly to `[0, len)`; this matches UI-SPEC.
- Pixel snapping: `Painter::round_to_pixel_center` exists (painter.rs:189); `Pos2::round_to_pixels(ppp)` / `Rect::round_to_pixels` also exist in 0.37 [VERIFIED: used in area.rs/image.rs]. UI-SPEC's "round_to_pixels" is valid; the rounding method is on `Pos2`/`Rect`, not `Painter`.
- Memory: touching only ~40 rows x 128 B means mmap faults ~1-2 pages; a sparse file's holes read as zeros and cost no disk I/O.
- Cap row text at 4096 chars (UI-SPEC); 128-byte rows are well under.
- **Preload nothing at startup**: `Mmap::map` does not touch pages. Do not call `advise(Advice::WillNeed)` or `populate`.

### Pattern 4: First-frame timing
eframe keeps the window hidden until after the first render (`post_rendering` calls `set_visible(true)` on first frame) [VERIFIED: epi_integration.rs:443], so "window appears" == first frame presented. There is no public "first frame presented" callback; use:
1. `main()` start: `let t0 = Instant::now(); let wall0 = SystemTime::now()`.
2. In the first `ui()` call set a flag and `ctx.request_repaint()` once; in the **second** `ui()` call (frame 1 has been presented by then) compute `t0.elapsed()` and print `startup_ms=<n>` and `first_frame_unix_us=<n>` (wall clock). This is a slight upper bound (+~1 frame) and is deterministic.
3. The harness records its own `SystemTime` immediately before `Command::spawn()`, then `startup_total = first_frame_unix_us - spawn_unix_us`. This **includes process creation, dynamic loader, and pre-`main` runtime init**, which the in-app `Instant` misses (CONTEXT says "app self-reports from process start"; the harness-side figure is the honest one, report both). System clocks are shared on one machine so wall-clock subtraction is valid (sub-ms resolution on all three OSes; on Windows use `SystemTime`, resolution ~100 ns-1 ms).
4. The one extra repaint must not occur in non-bench mode if idle purity matters: it is a one-time request, so idle is unaffected.
Status bar shows the in-app figure; `--bench` prints both.

### Pattern 5: Idle behaviour
eframe is event-driven: it only redraws when egui requests it (input, `request_repaint*`, animations) [CITED: egui docs on `Context::request_repaint`]. Rules for zero idle CPU:
- Never call `request_repaint()` unconditionally from `ui()`.
- Menu hover/tooltip animations request repaints only while active.
- Do not enable a blinking caret timer outside IME mode (UI-SPEC already says so).
- Verify with `ctx.cumulative_pass_nr()` shown only in a debug build/`--bench` stdout, or simply by sampling `accumulated_cpu_time()` over 10 s.
- `IntegrationInfo::cpu_usage` (frame time) is available via `frame.info()`, useful for per-frame cost but not for idle CPU.

### Pattern 6: IME wiring (custom view)
```rust
// Source: egui-0.37.0 data/input/ime_event.rs, data/output.rs; egui-winit lib.rs L1259-1310 [VERIFIED]
for ev in ui.input(|i| i.events.clone()) {
    if let egui::Event::Ime(ime) = ev {
        match ime {
            egui::ImeEvent::Preedit { text, .. } => self.preedit = text,   // empty text = dismissed
            egui::ImeEvent::Commit(s) => { self.probe.push_str(&s); self.preedit.clear(); }
            _ => {}
        }
    }
}
ui.ctx().output_mut(|o| {
    o.ime = Some(egui::IMEOutput {
        purpose: egui::IMEPurpose::Normal,
        rect: view_rect,
        cursor_rect: caret_rect,           // thin rect at probe caret
        should_interrupt_composition: false,
    });
});
```
How it works: if `output.ime` is `Some`, egui-winit calls `window.set_ime_allowed(true)`, then `set_ime_cursor_area` with the rect scaled by `pixels_per_point`; `None` disables IME [VERIFIED lib.rs 1259-1310]. Important: `Event::Ime` events are only forwarded to the app when no egui widget owns them (`Memory::owns_ime_events` is for `TextEdit`); our custom view does not use TextEdit, so it receives them. IME is allowed only while `output.ime` is set, so set it only when the viewer "has focus" - a custom view needs an explicit `ui.interact(rect, id, Sense::click_and_drag())` + `request_focus` to define focus, otherwise IME stays disabled. `ImeEvent::Enabled/Disabled` are deprecated and not emitted - UI-SPEC's "On Event::Ime(Enabled)" must be implemented as "first non-empty Preedit".
Platform caveats to document in the gate doc: egui-winit has special handling for key events on Windows/macOS during composition (`try_on_ime_processed_keyboard_input`); Linux X11/Wayland candidate window placement depends on the input method (ibus/fcitx) and is untestable in CI. Mark macOS/Linux "unverified" unless manually tested. Manual Windows test script: Microsoft IME Japanese, type "nihongo" -> preedit shown with underline -> Space convert -> Enter commit -> text in probe line; Esc cancels; candidate window appears below the probe caret.

### Pattern 7: HiDPI / mixed DPI
- `ctx.pixels_per_point()` = native scale factor x egui zoom factor; egui-winit handles `WindowEvent::ScaleFactorChanged` and updates the native ppp automatically, and fonts re-rasterize at the new ppp [VERIFIED: egui-winit lib.rs L60, L380-387]. The app needs no handler; it must not cache pixel-dependent values across frames (derive row positions from `ui.pixels_per_point()` each frame).
- Crispness depends on: glyph raster at physical px (egui does this), snapped positions (`round_to_pixels`), and an integer-pixel-friendly row height. At ppp 1.5 a 20 pt row is 30 px (ok); at 1.25 it is 25 px (ok); at 1.33 it is fractional -> snap each row's y to physical pixels but keep row stride constant by computing `y = round_to_pixels(top + i*20)`.
- Verification (manual, documented, since it is visual): run at Windows scale 100/125/150/200%, drag between a 100% and 200% monitor if available, and include a screenshot in docs. Automatable proxy: unit-test that for ppp in {1.0,1.25,1.5,2.0} snapped row y positions are multiples of 1/ppp and strictly increasing. eframe has `__screenshot` internal feature only; do not rely on it. Optionally use `egui::ViewportCommand::Screenshot` + `Event::Screenshot` to dump a PNG for the docs [ASSUMED available in 0.37; verify].
- Env override for CI-less testing: `WINIT_X11_SCALE_FACTOR=2` on Linux; `Context::set_pixels_per_point` for a debug `--ppp` flag.

### Anti-Patterns to Avoid
- Using `ScrollArea` for the 20 GB view (f32 precision, allocates content size).
- Calling `request_repaint()` per frame "to be safe" (kills idle CPU).
- Enabling both glow and wgpu features in the shipping build (default becomes wgpu, doubles size).
- Reading the whole file for "Size" display: use `File::metadata().len()` / `mmap.len()`.
- Using `std::fs::File::set_len` alone on Windows to make a sparse file (see Pitfall 1).
- Measuring startup with a debug build or without `lto`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| mmap | custom `CreateFileMapping`/`mmap` wrappers | `memmap2` | Cross-OS, handles 64-bit offsets |
| Process memory/CPU sampling | per-OS `/proc`, `GetProcessMemoryInfo`, `task_info` code | `sysinfo` in xtask | Three OS APIs abstracted |
| Argument parsing | custom parser | `lexopt` | Already chosen |
| Median/percentiles of N runs | - | 10 lines of std (sort + middle) | No crate needed |
| GL software rendering on CI | custom shims | Xvfb+Mesa (Linux), Mesa llvmpipe DLL (Windows) | See CI section |
| Binary size analysis | manual | `cargo bloat --crates` | Attribution per crate |

**Key insight:** this phase's value is accurate measurement; the code to write is small (view + harness). Everything else is configuration.

## Runtime State Inventory

Not applicable (greenfield, no rename/refactor/migration).

## Common Pitfalls

### Pitfall 1: NTFS non-sparse "20 GB file" (Windows)
**What goes wrong:** `File::create` + `set_len(20 GiB)` on NTFS extends EOF and reserves clusters (consumes ~20 GB of free space; the call returns quickly because data between valid-data-length and EOF is lazily zeroed). A later write at or near the end (a marker) forces the OS to zero-fill the entire gap synchronously: minutes of disk writes. Also an mmap read of such a file forces on-demand zeroing but is fine for first-page reads.
**Why:** Windows files are not sparse unless `FSCTL_SET_SPARSE` has been applied to the handle [CITED: learn.microsoft.com FSCTL_SET_SPARSE; behaviour of VDL zero-fill is from training knowledge: ASSUMED].
**How to avoid:** in xtask on Windows: open file, `DeviceIoControl(handle, FSCTL_SET_SPARSE, NULL,0,NULL,0,..)` (a NULL input buffer sets the flag), then `set_len`, then write markers. Fallback: `fsutil sparse setflag`. Check free space first and verify with `GetCompressedFileSize` (or `fsutil sparse queryflag`) that on-disk size is tiny. Windows sparse files require NTFS or ReFS; exFAT/FAT32 and some network drives do not support it - fail with a clear message. GitHub windows runner work dir is NTFS.
**Warning signs:** helper takes > 2 s; disk usage jumps; `File size on disk` ≈ size.

### Pitfall 2: Linux/macOS sparse
`set_len` (ftruncate) creates holes on ext4/xfs/btrfs/APFS; no flag needed. Not sparse on HFS+, FAT, and some tmpfs/overlay edge cases; check with `metadata.blocks()` (Unix `MetadataExt`) that `blocks*512` is far below len. macOS and Linux runners have enough space for the apparent size only if sparse; if the filesystem rejects, fall back to a smaller size and mark the result "not 20 GB".

### Pitfall 3: RSS inflated by the mapping
Resident set counts mapped file pages only when touched. First-page paint touches a few pages; the harness must also check RSS stays low after scroll tests. On Windows, "Working Set" includes shared mapped pages; also record "Private Bytes"/commit via sysinfo (`virtual_memory()` is commit on Windows) separately. macOS may report larger RSS due to system frameworks (GL/Metal); the 60 MB threshold needs verifying per-OS - use measured baseline + margin, not the target, for CI thresholds.

### Pitfall 4: GL on CI runners
- Linux: needs a display and GL. Use Xvfb + Mesa llvmpipe (below). Without `+extension GLX` / working DRI, glutin fails with "no config".
- Windows: `windows-latest` has no GPU -> `opengl32.dll` is MS GDI generic 1.1 -> egui_glow fails (needs >= GL 2.0/3.x or GLES2). Mitigation: download a pinned Mesa llvmpipe build (pal1000/mesa-dist-win or mmozeiko/build-mesa, which provide `opengl32.dll` + `libgallium_wgl.dll`) next to `whim.exe`, set `GALLIUM_DRIVER=llvmpipe` [CITED: github.com/pal1000/mesa-dist-win, github.com/mmozeiko/build-mesa, github.com/openrndr/setup-opengl]. Because this swaps the renderer, startup/RSS from that job are not representative of a user's machine: label them "CI software GL" and keep generous thresholds; size check is unaffected. Alternative: measure the wgpu build on Windows CI (DX12 WARP) - also software. Plan for CI-Windows glow to be best-effort (`continue-on-error` for the runtime part, hard-fail for size).
- macOS: `macos-latest` is Apple Silicon (arm64) virtualized; OpenGL 4.1 via CGL is deprecated but works; display exists in the runner's GUI session [ASSUMED - verify on first run]. macOS Intel number needs `macos-13`/ x86_64 target (not required for the gate).
- Fail-soft design: if window creation fails in CI, the harness must report "no GL" distinctly from "budget exceeded" (exit code), so the first CI run's output is diagnosable.

### Pitfall 5: Startup includes driver init
glow startup is dominated by window+GL context creation and driver load (tens of ms on real GPUs, much more with llvmpipe). Cold start (first run after boot / evicted cache) can be 2-5x warm. Record both locally; CI = warm only (as decided). Run N>=10, report median and max, discard the first run separately as "cold-ish".

### Pitfall 6: Release profile left default
Without `lto`, `codegen-units=1`, `panic=abort`, `strip`, binary is 2-3x bigger. Also `debug = 0`. Note `panic = "abort"` is fine for the app; xtask is a separate profile-less tool (workspace profile applies to all members; set `[profile.release]` in root, xtask rarely built release).

### Pitfall 7: IME not enabled because no focus/`output.ime`
See Pattern 6: IME stays off unless `IMEOutput` is set every frame; setting it unconditionally while no widget has focus yields a candidate window anchored at stale coordinates.

### Pitfall 8: `Ime` events on macOS/Linux lost with dead keys; Windows composition with `Esc`
egui-winit has special-case code for keys during composition; on Windows, pressing Esc during preedit may deliver `Preedit` with empty text (dismissal), not a separate cancel event. Treat empty preedit as "composition ended".

## Code Examples

### Sparse file helper (xtask)
```rust
// Unix: ftruncate is already sparse.
// Windows: set the sparse flag BEFORE set_len. [windows-sys names ASSUMED; verify on build]
#[cfg(windows)]
fn mark_sparse(f: &std::fs::File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::{IO::DeviceIoControl, Ioctl::FSCTL_SET_SPARSE};
    let mut ret = 0u32;
    let ok = unsafe {
        DeviceIoControl(f.as_raw_handle() as _, FSCTL_SET_SPARSE,
            std::ptr::null(), 0, std::ptr::null_mut(), 0, &mut ret, std::ptr::null_mut())
    };
    if ok == 0 { Err(std::io::Error::last_os_error()) } else { Ok(()) }
}

fn create_sparse(path: &std::path::Path, len: u64) -> std::io::Result<()> {
    use std::io::{Seek, SeekFrom, Write};
    let mut f = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(true).open(path)?;
    #[cfg(windows)] mark_sparse(&f)?;
    f.set_len(len)?;
    for (off, text) in [(0u64, &b"WHIM-START\n"[..]), (len / 2, b"WHIM-MIDDLE\n"), (len - 16, b"WHIM-END\n")] {
        f.seek(SeekFrom::Start(off))?; f.write_all(text)?;
    }
    Ok(())
}
```
Also make the first page visibly textual (write several lines of ASCII at offset 0) so screenshots show something besides `·`.

### memmap2 on huge files
```rust
// Source: memmap2 0.9 docs [CITED]; unsafe is required because the file can change underneath.
let file = std::fs::File::open(path)?;
let map = unsafe { memmap2::Mmap::map(&file)? };   // lazy; nothing is read
let slice = &map[off as usize..(off as usize + 128).min(map.len())];
```
64-bit targets only (usize offsets). On Windows `Mmap::map` creates a file mapping of full length; a read-only mapping of a 20 GB sparse file is fine, but the file stays locked against deletion/truncation while mapped (relevant for xtask cleanup; drop the process first). Truncation of a mapped file by another process => SIGBUS/access violation: document as known risk (project-level pitfall, Phase 2 concern).

### Idle sampling with sysinfo (xtask)
```rust
// Source: sysinfo 0.40.1 src/common/system.rs (Process::memory, accumulated_cpu_time) [VERIFIED names]
use sysinfo::{System, Pid, ProcessesToUpdate, ProcessRefreshKind};
let mut sys = System::new();
let pid = Pid::from_u32(child.id());
sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
let rss = sys.process(pid).unwrap().memory();                        // bytes
let cpu0 = sys.process(pid).unwrap().accumulated_cpu_time();        // ms
std::thread::sleep(std::time::Duration::from_secs(10));
sys.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
let cpu_pct = (sys.process(pid).unwrap().accumulated_cpu_time() - cpu0) as f64 / 10_000.0 * 100.0;
```
Exact `refresh_processes` signature has changed between sysinfo versions; adjust on compile [ASSUMED]. Prefer accumulated CPU time deltas over `cpu_usage()` (needs two refreshes spaced >= `MINIMUM_CPU_UPDATE_INTERVAL` and is noisy). Wait ~2 s after the first-frame line before sampling, sample 10 s. Idle threshold: CPU < 1 % (generous), ideally 0.0.

### Release profile
```toml
[profile.release]
opt-level = 3          # measure "s" as an alternative; keep 3 unless size gate fails
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"      # or `strip = true`
debug = 0
```
`.cargo/config.toml`: `[target.x86_64-pc-windows-msvc] rustflags = ["-C", "target-feature=+crt-static"]` (no VC++ redist) per STACK.md. Also `[profile.release.package."*"] opt-level = "s"` as an experiment for dependencies. Optional later: `-Z build-std` (nightly) is out of scope. `cargo bloat --release --crates` as CI artifact. Stripped size for gate: measure the file after the release build with `strip=symbols` (Linux/macOS), PDB is separate on Windows. UI-SPEC and STACK both estimate 6-10 MB; the gate (<=10 MB) is plausible but must be measured for glow (eframe glow + glutin + egui + image/png + default fonts ~ 1.6-2 MB of font data).

### CI sketch (GitHub Actions)
```yaml
jobs:
  gate:
    strategy:
      fail-fast: false
      matrix:
        os: [windows-latest, ubuntu-22.04, macos-latest]
        backend: [glow, wgpu]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@master   # or rely on rust-toolchain.toml with rustup
        with: { toolchain: "1.99.0" }
      - uses: Swatinem/rust-cache@v2
      - if: runner.os == 'Linux'
        run: |
          sudo apt-get update
          sudo apt-get install -y xvfb libgl1-mesa-dri libglx-mesa0 libegl1 libxkbcommon-x11-0 \
            libx11-dev libxcursor-dev libxrandr-dev libxi-dev libxkbcommon-dev libwayland-dev mesa-vulkan-drivers
      - run: cargo build --release --locked ${{ matrix.backend == 'wgpu' && '--no-default-features --features wgpu' || '' }}
      - run: cargo xtask check-size            # hard fail
      - name: measure (Linux)
        if: runner.os == 'Linux'
        run: xvfb-run -a -s "-screen 0 1280x800x24 +extension GLX" cargo xtask measure --runs 10
        env: { LIBGL_ALWAYS_SOFTWARE: "1" }
      - name: measure (other)
        if: runner.os != 'Linux'
        run: cargo xtask measure --runs 10
        continue-on-error: ${{ runner.os == 'Windows' }}   # until software GL proven
      - uses: actions/upload-artifact@v4
        with: { name: measure-${{ matrix.os }}-${{ matrix.backend }}, path: target/measure/*.json }
```
Package names are for Ubuntu 22.04 (`libgl1-mesa-dri`, `libglx-mesa0`, `libxkbcommon-x11-0`); wgpu on Linux CI needs `mesa-vulkan-drivers` (lavapipe) [ASSUMED names, standard]. Pin action major versions; confirm current majors when authoring. `ubuntu-22.04` provides an old glibc baseline as STACK.md wants. Disk: runners have >14 GB free but sparse files need no real space; the measure job should create the 20 GB sparse file in `RUNNER_TEMP`. Add a step that asserts on-disk size is small to guard against filesystem not supporting sparse.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `App::update(ctx, frame)` | `App::ui(ui, frame)` + optional `logic(ctx, frame)` | by eframe 0.37 | All skeleton code uses `ui` [VERIFIED] |
| `TopBottomPanel`/`SidePanel` + `show_inside` | `egui::Panel::top/bottom` + `.show(ui, ..)`; `show_inside` deprecated | by egui 0.37 | UI-SPEC wording needs translating [VERIFIED] |
| `egui::menu::bar` | `egui::MenuBar::new().ui(ui, ..)` | by 0.37 | [VERIFIED] |
| `ImeEvent::Enabled/Disabled` | deprecated, unused; `Preedit` (empty = dismissed), `Commit`, `DeleteSurrounding` | by 0.37 | Detect composition via Preedit [VERIFIED] |
| eframe default = glow (older versions) | default feature set includes wgpu; both enabled => Wgpu | by 0.37 | Must disable defaults [VERIFIED] |
| MSRV ~1.85 | MSRV 1.95 | 0.37 | Pin 1.99.0 |

**Deprecated/outdated:** UI-SPEC references `TopBottomPanel`, `menu::bar`, `ImeEvent::Enabled`, and a 2-field `IMEOutput`; implement with the names above. STACK.md's `glow` feature line lacks `clipboard`/`accesskit` as separate features.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `windows-sys` feature names `Win32_System_IO`, `Win32_System_Ioctl` provide `DeviceIoControl`/`FSCTL_SET_SPARSE` | Stack, Code Examples | Compile error; fall back to `fsutil` |
| A2 | NTFS non-sparse write near EOF zero-fills the gap (minutes, real disk use) | Pitfall 1 | If milder, sparse flag still harmless |
| A3 | `fsutil sparse setflag` works unelevated on user-owned files | Alternatives | Fallback broken; use windows-sys |
| A4 | `windows-latest` has no GPU so glow needs Mesa llvmpipe | Pitfall 4, CI | If GL works natively CI gets simpler |
| A5 | `macos-latest` (arm64 VM) can create a GL 3.2+/4.1 context for glutin | Pitfall 4 | glow measurement on macOS CI fails; use wgpu/Metal or local Mac |
| A6 | Ubuntu 22.04 package names for Xvfb/Mesa and `mesa-vulkan-drivers` for wgpu | CI | apt install fails; adjust |
| A7 | `AppCreator` returns `Result<Box<dyn App>, _>`; `sysinfo::refresh_processes` signature as shown | Examples | Minor compile fixes |
| A8 | `ViewportCommand::Screenshot` available in 0.37 for HiDPI evidence | Pattern 7 | Use OS screenshots instead |
| A9 | Stripped skeleton binary ~6-10 MB (from STACK.md, unmeasured) | Release profile | Gate may fail; that is what the phase measures |
| A10 | OS startup of ~200 ms is achievable with glow on real GPU (unmeasured) | Summary | Drives go/no-go |

## Open Questions (RESOLVED)

1. **Windows CI renderer.** Which software GL path (Mesa DLL vs wgpu WARP) gives a stable job? Recommendation: Windows CI = size hard gate + best-effort runtime; real Windows glow numbers from the local machine are recorded in `docs/measurements.md` by hand (the user's machine is Windows 11).
   **RESOLVED:** Windows CI uses Mesa llvmpipe (hash-pinned `mesa-dist-win` DLLs next to the exe) with `continue-on-error` on the runtime step; size stays a hard gate (plan 01-03 Task 3). Real Windows numbers come from the local machine (plan 01-05).
2. **CONTEXT says "stripped binary <= 10 MB" vs project target 15 MB.** Fine as a skeleton budget; confirm that bundled JetBrains Mono is excluded from the first measurement.
   **RESOLVED:** the 10 MB gate applies to the default glow build; a no-font build (`--no-default-features --features glow`, label `windows-local-glow-nofont`) is also measured and the font delta is recorded in `## Size deltas` (plan 01-05 Task 1).
3. **Does the user want `wgpu` binary size reported even if it fails the 10 MB gate?** Recommendation: yes, reported only; gate applies to glow.
   **RESOLVED:** wgpu size and runtime are reported only (`windows-local-wgpu` and the CI wgpu matrix leg); the 10 MB hard gate applies to glow only (plans 01-03, 01-05).
4. **macOS bundle-less measurement:** run the raw binary on macOS for the gate; `.app` packaging is PERF-04 (later).
   **RESOLVED:** macOS CI measures the raw `target/release/whim` binary; no `.app` bundle in Phase 1 (plan 01-03 Task 3).

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| rustup | toolchain pinning | yes | present at `~/.cargo/bin` | - |
| rustc (default `stable`) | build | stale | 1.69.0 | use installed `1.99.0` via rust-toolchain.toml |
| rustc 1.99.0 (side-by-side) | build (MSRV 1.95) | yes | 1.99.0 (2026-09-28) | - |
| nightly-2026-08-18 | not needed | yes | - | - |
| MSVC build tools / linker | Windows build | not checked | - | `rustup show` reports msvc host; verify `cargo build` works first; install VS Build Tools if not |
| Network to crates.io | fetch deps | yes | reachable via curl | - |
| GitHub remote / Actions | CI | no remote | - | user pushes; CI workflow can be authored and verified only after push (plan a human-verify checkpoint) |
| Linux/macOS machines | local measurement | no | - | CI only (decided) |
| Japanese IME (Microsoft IME) | manual IME test | not checked | - | Windows 11 ships it; user adds Japanese language pack |
| Disk space for sparse file | 20 GB test | not checked | - | sparse needs ~0; verify NTFS (not exFAT) |

**Missing dependencies with no fallback:** none blocking. **Plan note:** first task should be creating `rust-toolchain.toml` (`channel = "1.99.0"`) and confirming `cargo --version` inside the repo reports 1.99.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | built-in `cargo test` (Rust 1.99) for unit tests; `cargo xtask` for budget/integration checks |
| Config file | none - Wave 0 creates workspace + `xtask` + `.cargo/config.toml` alias |
| Quick run command | `cargo test --workspace --lib` |
| Full suite command | `cargo test --workspace && cargo build --release && cargo xtask check-size && cargo xtask measure --runs 5` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| PERF-01 | median startup <= 300 ms (target 200); first-frame line printed | integration (needs GL) | `cargo xtask measure --runs 10 --max-startup-ms 300` | Wave 0 |
| PERF-02 | idle RSS <= 60 MB, idle CPU < 1 % over 10 s | integration | `cargo xtask measure --max-rss-mb 60 --max-idle-cpu 1.0` | Wave 0 |
| PERF-02 | opening 20 GB sparse file does not raise RSS beyond budget | integration | `cargo xtask sparse --size 20GiB out.bin && cargo xtask measure --file out.bin` | Wave 0 |
| PERF-05 | build + launch on 3 OSes | CI matrix | GitHub Actions job `gate` | Wave 0 |
| (gate) | stripped size <= 10 MB | unit-ish | `cargo xtask check-size --max-mb 10` | Wave 0 |
| PERF-02/core | view reads only visible bytes (row slicing, non-printable mapping, EOF clamp) | unit | `cargo test -p whim view::` | Wave 0 |
| UI-04 | snapped row y positions at ppp 1.0/1.25/1.5/2.0 | unit | `cargo test -p whim view::snap` | Wave 0 |
| UI-04 | crisp text on HiDPI / mixed DPI | manual | checklist in 01-GATE.md with screenshots | manual-only (visual) |
| IME | Windows Japanese IME compose/convert/commit/cancel | manual | checklist in 01-GATE.md | manual-only (needs real IME) |
| Sparse | helper produces 20 GB apparent / tiny on-disk file, markers readable | integration | `cargo test -p xtask sparse` (assert on-disk < 100 MB) | Wave 0 |
| Gate | 01-GATE.md exists with go/no-go + fallback (Slint) and numbers | doc check | `test -f .planning/phases/01-toolkit-footprint-gate/01-GATE.md` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --workspace --lib` and `cargo clippy --workspace -- -D warnings`
- **Per wave merge:** full `cargo test --workspace` + release build + `cargo xtask check-size`
- **Phase gate:** CI green on the three-OS matrix (size hard, runtime soft) + local Windows measurement table + manual IME/HiDPI checklists complete before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `rust-toolchain.toml`, workspace `Cargo.toml`, `.cargo/config.toml` (xtask alias, crt-static)
- [ ] `xtask/` crate with `sparse`, `measure`, `check-size` subcommands
- [ ] Unit test modules in `src/view.rs` (row slicing, snapping)
- [ ] `.github/workflows/ci.yml`
- [ ] `docs/measurements.md` template, `01-GATE.md` template

## Security Domain

`security_enforcement` not set in config (treated as enabled); this phase has a small attack surface (local read-only viewer, no network).

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | - |
| V3 Session Management | no | - |
| V4 Access Control | no | OS file permissions only |
| V5 Input Validation | yes | Clamp all mmap slice ranges to `len`; treat file bytes as untrusted; never `unwrap` on slicing; lossy byte->char mapping; cap row width |
| V6 Cryptography | no | - |
| V12 File handling | yes | Open read-only; no temp files outside xtask; sparse helper refuses to overwrite existing non-test files |

### Known Threat Patterns

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Out-of-bounds slice on huge offsets / usize overflow | Tampering/DoS | Use `u64` offsets, `checked_add`, `min(len)`; fuzz-ish unit tests at `len-1`, `len`, `u64::MAX` |
| File truncated while mapped (SIGBUS / access violation) | DoS | Document; Phase 2 handles via guarded reads; read-only mapping |
| Huge single line -> layout DoS | DoS | Fixed 128-byte rows, 4096-char cap |
| Malicious file bytes rendered as control chars | Spoofing | Replace non-printable with `·` |
| CI supply chain (actions, downloaded Mesa DLLs) | Tampering | Pin action versions and Mesa release by tag + SHA256 check |
| Unsafe mmap UB if file mutated | Tampering | Accept and document; `unsafe` confined to one function |

## Sources

### Primary (HIGH confidence)
- crates.io API (2026-10-31): eframe 0.37.0 features/dependencies/rust_version=1.95; egui, egui-winit, egui_glow 0.37.0; memmap2 0.9.12; sysinfo 0.40.1 (rust-version 1.95)
- Downloaded crate sources read directly: eframe-0.37.0 (`Cargo.toml`, `src/epi.rs`, `src/native/epi_integration.rs`, `src/lib.rs`), egui-0.37.0 (`containers/panel.rs`, `containers/menu.rs`, `data/output.rs`, `data/input/ime_event.rs`, `painter.rs`, `context.rs`), egui-winit-0.37.0 (`src/lib.rs` IME and scale-factor handling), sysinfo-0.40.1 (`src/common/system.rs`)
- Local environment: `rustup show`, `rustc --version`

### Secondary (MEDIUM confidence)
- [pal1000/mesa-dist-win](https://github.com/pal1000/mesa-dist-win), [mmozeiko/build-mesa](https://github.com/mmozeiko/build-mesa), [openrndr/setup-opengl](https://github.com/openrndr/setup-opengl) - Mesa llvmpipe on Windows runners (web search; no egui-specific guide found)

### Tertiary (LOW confidence)
- NTFS VDL zero-fill behaviour, GH runner GL capabilities, package names, binary-size/startup expectations: training knowledge, listed in Assumptions Log. Context7 was not queried (sources read from the crates directly instead).

## Metadata

**Confidence breakdown:**
- Standard stack / API names: HIGH - read from 0.37.0 sources
- Architecture / measurement design: MEDIUM - sound but unproven until run
- CI GL on Windows/macOS runners: LOW-MEDIUM - must be validated by the first pipeline run
- Pitfalls: MEDIUM

**Research date:** 2026-10-31
**Valid until:** 2026-11-30 (egui releases roughly every 1-3 months; re-check if 0.38 lands)
