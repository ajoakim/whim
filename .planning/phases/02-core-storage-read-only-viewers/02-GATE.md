# Phase 2 Gate: Core Storage & Read-Only Viewers

**VERDICT:** CONDITIONAL GO (pending CI push and user decision on +51 ms local Windows warm startup delta)

## Decision rule

Standing footprint budgets carried from Phase 1 (`01-GATE.md`, `02-CONTEXT.md` D-17/D-18):
- Stripped binary size: `<= 10 MB` (project target `< ~15 MB`) — hard gate on all OSes
- Settled idle footprint (comparable per-OS metric: working set on Windows, `phys_footprint` on macOS, PSS on Linux, min over `+20..+30 s`): `<= 70 MB` — hard on real hardware and macOS CI
- Warm startup median: `<= 300 ms` local real hardware (`<= 600 ms` macOS CI hard gate)
- Idle CPU: `< 1 %` local (`< 2 %` CI)
- Software-GL CI runners (Windows/Linux VMs) are report-only for runtime numbers; size stays hard everywhere.

## Measured numbers

Local machine: AMD Ryzen 9 5950X, Intel Arc B580, Windows 11, 20 GiB sparse file (`target/big.sparse.bin`), warm (10 runs, settled window `+20..+30 s`), 2026-11-19.

| Metric | Budget | Phase 1 (Windows glow) | Phase 2 (Windows glow) | Phase 2 (Windows wgpu) | Status |
|--------|--------|------------------------|------------------------|------------------------|--------|
| Stripped size | <= 10.0 MB | 5.01 MB | **5.86 MB** (6,147,584 B) | 10.08 MB (10,566,656 B) | **PASS** (glow +0.85 MB for rfd + clipboard + font provider + viewers) |
| Startup median (warm) | <= 300.0 ms | 271.7 ms | **351.0 ms** (in-app 334.0 ms, max 559.3 ms) | n/a | **OVER (+51.0 ms)** |
| Idle footprint (settled WS) | <= 70.0 MB | 59.9 MB | **67.8 MB** (peak 101.4 MB; private 62.7 MB) | n/a | **PASS** (private dropped from 100.5 MB -> 62.7 MB after mmap removal) |
| Idle CPU | < 1.0 % | 0.35 % | **0.00 %** | n/a | **PASS** |

### Notes on the +51 ms startup delta
- Phase 2 adds `egui_system_font_provider` (disabled until missing glyph), `rfd`, `arboard` (`clipboard`), and spawns the background open + line-index workers on launch.
- The 20 GiB sparse file now opens via positional reads + background indexer instead of `mmap`; private memory dropped by ~38 MB (100.5 MB → 62.7 MB) because there is no 20 GiB file-backed commit charge.
- Idle working set settled at 67.8 MB (2.2 MB headroom under 70 MB) and idle CPU is 0.00%.
- Warm startup median rose from 271.7 ms to 351.0 ms (+79 ms vs Phase 1, +51 ms over the 300 ms gate). Likely contributors: first-frame layout of full real-line `TextView` + gutter + menu/tab/status chrome + worker thread spawn competing with GL init on launch.

## glow vs wgpu

| Metric | glow | wgpu | Delta |
|--------|------|------|-------|
| Stripped size | 5.86 MB | 10.08 MB | +4.21 MB |

`glow` remains the sole default backend; `wgpu` now exceeds the 10 MB size gate on Windows.

## CI

Pending push to `origin/main` (Task 3). Once pushed, this section will record the GitHub Actions run URL and per-job status for:
- `test` (windows-latest, ubuntu-22.04, macos-latest)
- `gate` (size hard on all 3 OSes; macOS glow runtime hard-gated at `<= 70 MB` footprint / `<= 600 ms` startup; `Whim-macos-arm64.zip` artifact upload)

## Cold start

Cold start not measured locally (warm-only series; run 0 = 559.3 ms).

## macOS Gatekeeper wording (PERF-04 / D-21)

Documented in `README.md` (`## macOS`):
- macOS 15+: System Settings › Privacy & Security › "Open Anyway"
- macOS 14 and earlier: right-click `Whim.app` › Open
- Terminal: `xattr -dr com.apple.quarantine /path/to/Whim.app`

## Risks carried forward

- **Warm startup median (351.0 ms vs 300 ms gate)**: profile first-frame open/indexer startup contention (e.g., deferring `spawn_indexer` until after frame 1 so it does not contend with GL init) if we want to pull median back under 300 ms.
- **Idle working set headroom (67.8 MB vs 70 MB)**: close to the 70 MB line on Windows Intel Arc B580; watch carefully in Phase 3 (piece table + undo).
- **Real-hardware Linux/macOS numbers**: CI-only so far.
- **IME on macOS/Linux** and **mixed-DPI drag**: remain open from `01-HUMAN-UAT.md`.
