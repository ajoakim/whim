# Phase 1: Toolkit & Footprint Gate - Context

**Gathered:** 2026-10-31
**Status:** Ready for planning

<domain>
## Phase Boundary

Prove egui/eframe (glow) meets the startup, RAM, binary-size, HiDPI and IME expectations on Windows, Linux and macOS using a walking-skeleton app that mmaps a 20 GB sparse file and paints its first page. Deliver measured numbers, a written go/no-go (fallback: Slint), and CI that enforces the budgets. Requirements: PERF-01, PERF-02, PERF-05, UI-04.

</domain>

<decisions>
## Implementation Decisions

### Measurement & Platforms
- Linux/macOS numbers come from a GitHub Actions matrix (windows-latest, ubuntu-22.04, macos-latest) that records stripped size, time-to-first-frame, idle RSS and idle CPU as artifacts; Windows is also measured locally. The repo currently has no git remote — the user will push to GitHub for CI.
- Startup: the app self-reports time from process start to first painted frame; a harness runs N times and reports the median. CI measures warm start only (true cold cache is not reproducible there); local runs note cold vs warm.
- glow vs wgpu: both are cargo features of the same skeleton; measure both, glow is the default.
- Toolchain: pin current stable via `rust-toolchain.toml`, edition 2024. Local rustc is 1.69 and must be upgraded (`rustup update stable`) — eframe 0.37/edition 2024 need >= 1.85.

### Skeleton Scope
- The skeleton is a real seed crate (`whim`) that later phases grow, not a throwaway spike: eframe window, menu bar stub, status bar, virtualized monospace view painting only visible rows from a memmap2 mapping.
- Positioning: byte-offset virtual scroll with fixed row height; no sparse line index yet (Phase 2). First page reads only the visible bytes.
- Fonts: egui default fonts plus one bundled monospace font; no CJK fallback (UI-05 is later).
- A cross-platform sparse-file helper (xtask or test util) creates 20 GB sparse files via `set_len` + a few written markers.

### Gate Criteria & IME
- GO if: startup <= 300 ms (target 200), stripped binary <= 10 MB at skeleton stage, idle RSS <= 60 MB on all OSes. NO-GO triggers a Slint evaluation.
- CI fails on budget regressions: binary size is a hard limit; startup and RSS use generous thresholds because of runner noise.
- IME: wire `Event::Ime` preedit/commit and IME cursor positioning in the skeleton; manually test Windows Japanese IME; document supported scope, marking macOS/Linux unverified if untested.
- Go/no-go doc: `.planning/phases/01-toolkit-footprint-gate/01-GATE.md`, plus a `docs/` measurements table that later phases keep updating.

### Claude's Discretion
- Crate/module layout, harness implementation language (Rust xtask preferred), exact CI workflow structure, HiDPI verification method.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- None — greenfield repository (only .planning/ and CLAUDE.md).

### Established Patterns
- Stack per CLAUDE.md/.planning/research/STACK.md: eframe/egui 0.37 with glow, memmap2, lexopt.

### Integration Points
- The skeleton crate becomes the root of the app; Phase 2 adds piece table, line index and hex view on top.

</code_context>

<specifics>
## Specific Ideas

- Measurements table in docs/ is a standing artifact re-checked every phase (footprint is a standing gate per roadmap).

</specifics>

<deferred>
## Deferred Ideas

- CJK font fallback (UI-05) — later phase.
- Sparse line index — Phase 2.

</deferred>
