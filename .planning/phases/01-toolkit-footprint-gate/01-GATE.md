# Phase 1 Gate: egui/eframe (glow)

**Verdict:** PENDING (local numbers in; CI, IME and HiDPI checks outstanding)

**Draft status of the local evidence:** size and startup pass on real hardware. Idle RSS as measured by the harness (92.8 MB) FAILS the 60 MB threshold; a diagnostic puts the steady-state working set at about 58-64 MB, which is borderline and not clearly a pass. See "Idle RSS finding". Under the literal decision rule below, the harness figure on real hardware is a NO-GO input; the final verdict must be decided with that in view.

## Decision rule

Thresholds (CONTEXT, locked): GO if startup <= 300 ms (target 200 ms), stripped binary <= 10 MB, idle RSS <= 60 MB on all OSes.

- NO-GO if the stripped glow binary exceeds 10 MB on any OS, or any threshold fails on real hardware (the local Windows machine).
- A runtime-budget failure on the macOS CI runner is also a NO-GO input (real, Metal-backed GL).
- Runtime failures seen only on Linux/Windows CI software-GL runners (llvmpipe, no GPU) are listed as open items with "GO (provisional)", because they do not measure user hardware (RESEARCH Pitfall 4).
- An unpushed CI caps the verdict at "GO (provisional)".
- Whenever the verdict is "GO (provisional)", the doc MUST state `PERF-05: unverified (provisional verdict)` and MUST list `Linux/macOS startup and idle RSS numbers` as an open item that blocks Phase 2 sign-off.
- Fallback toolkit: **Slint 1.18.1**. NO-GO triggers a Slint evaluation spike before Phase 2.

## Measured numbers

Machine: AMD Ryzen 9 5950X, Intel Arc B580 (driver 32.0.101.6790), Windows 11, 20 GiB sparse file, warm.

| Metric | Threshold | Windows local glow | Windows local wgpu | Linux CI glow | macOS CI glow | Pass? |
|--------|-----------|--------------------|--------------------|---------------|---------------|-------|
| Stripped size | <= 10 MB | 5.01 MB | 9.24 MB | pending | pending | glow: yes (Windows) |
| Startup median | <= 300 ms | 265.7 ms (max 435.5, first run 435.5; in-app median 254.0) | 341.1 ms (max 567.7) | pending | pending | glow: yes; wgpu: no |
| Idle RSS (harness) | <= 60 MB | 92.8 MB | 153.5 MB | pending | pending | glow: NO; wgpu: NO |
| Idle RSS (Get-Process steady state, +8 s) | <= 60 MB | WS 58.5-63.7 MB, private 57.9-60.2 MB | WS 89-108 MB, private 272-275 MB | n/a | n/a | glow: borderline |
| Idle CPU | < 1 % | 0.16 % | 0.00 % | pending | pending | yes |

Note: the Windows glow startup max equals the first run (435.5 ms); the median of 10 warm runs is within the 300 ms gate, but run-to-run spread (253-436 ms) means individual launches can exceed it.

## Idle RSS finding

- The harness reports 92.8 MB. A concurrent Get-Process poll on the same process shows WS 92 MB / commit 132 MB during the harness sample window, dropping to WS 61 MB / commit 101 MB shortly after (transient startup memory not yet trimmed). Harness samples at +2 s and +12 s and takes the max, so it captures the untrimmed value.
- Steady-state WS with no file: 63.7 MB; with the 20 GiB file: 58.5 MB; with `--ppp 1.0`: 57.7 MB. The mmap does not contribute (difference is noise).
- Peak working set at startup is 101-106 MB.
- GPU is Intel Arc B580, driver 32.0.101.6790. The Intel OpenGL ICD loaded into the process is the likely major contributor, but this was not isolated (no minimal eframe window experiment was run, and no product code was changed).
- Honest status: idle RSS does NOT pass by the harness metric; steady state sits at or just under the line (about 58-64 MB). The metric definition (working set sampled early vs after trimming) matters more than the toolkit here, and needs a decision from the user. No thresholds were changed.

## glow vs wgpu

| | glow | wgpu | Delta |
|---|------|------|-------|
| Size | 5.01 MB | 9.24 MB | +4.23 MB |
| Startup median | 265.7 ms | 341.1 ms | +75 ms |
| Idle RSS (harness) | 92.8 MB | 153.5 MB | +61 MB |
| Private bytes (steady) | ~58-60 MB | ~272 MB | +~213 MB |

glow is smaller, faster to start and much lighter in memory; wgpu fails startup and RSS and nearly exhausts the size budget. Recommendation: glow stays the default; wgpu remains an optional cargo feature only.

## IME supported scope

- Windows Japanese (Microsoft IME): pending manual test
- macOS: unverified
- Linux X11/Wayland (ibus/fcitx): unverified
- Known limits: probe caret only; real text editing and caret handling are Phase 3.

## HiDPI / mixed DPI

| Check | Result |
|-------|--------|
| 100% | pending |
| 125% | pending |
| 150% | pending |
| 200% | pending |
| Drag between monitors | pending |

Unit-test proxy: `view::snap` tests pass.

## CI

pending first push. Thresholds currently in ci.yml: MAX_SIZE_MB 10 (hard), startup 2000 ms soft, RSS 200 MB soft, idle CPU 2 % soft. The repo has no git remote yet.

## Cold start

pending (filled in Task 3). Run 0 of the warm series (435.5 ms) is only a cold-ish indicator, not a reboot cold start.

## Risks carried forward

- Mapped-file truncation by another process (Phase 2).
- macOS Apple Event open-file path (Phase 8).
- CI software GL is not representative of user hardware.
- Accessibility of the custom-painted view.
- accesskit size vs accessibility: glow + accesskit is 5.32 MB vs 5.01 MB default (+0.31 MB, `size-windows-local-glow-accesskit.json` minus `size-windows-local-glow.json`), so it still fits well under 10 MB. It is off by default to save size, so menus, tabs and the status bar get no screen-reader exposure until it is enabled. The cost is small enough that enabling it by default should be reconsidered.
- Idle RSS metric definition and GPU driver contribution (see Idle RSS finding).
