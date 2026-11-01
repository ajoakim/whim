# Phase 1 Gate: egui/eframe (glow)

**Verdict:** GO — egui/eframe 0.37 with glow is confirmed as the toolkit (user decision, 2026-11-01)

**Basis:** with the comparable per-OS footprint metric (CI run 41270819589), whim is within the 70 MB idle budget on Windows real hardware (59.9 MB working set) and macOS (37.6 MB phys_footprint), adds only ~3 MB over a bare eframe window on every OS, passes size (5.0 MB) and startup (~270 ms warm median) budgets, and passed manual IME and HiDPI checks. The earlier NO-GO (macOS 102.7 MB RSS) was an artefact of counting shared framework pages. Only the two software-GL CI VMs exceed 70 MB, and the bare toolkit does too; these are not NO-GO inputs. Slint is not needed: the idle floor is toolkit + GL driver, not whim code.

**Superseded:** the initial literal NO-GO (see Rationale and Review note below) is kept for the record.

**Open items carried forward (non-blocking):** cold start not measured (warm-only); dragging between mixed-DPI monitors untested; macOS/Linux IME unverified; no real-hardware Linux numbers (CI only).

## Decision rule

Thresholds (CONTEXT, locked, amended once by the user, see Decisions): GO if startup <= 300 ms (target 200 ms), stripped binary <= 10 MB, idle RSS <= 70 MB on all OSes, measured as the settled working set (originally 60 MB with early samples).

- NO-GO if the stripped glow binary exceeds 10 MB on any OS, or any threshold fails on real hardware (the local Windows machine).
- A runtime-budget failure on the macOS CI runner is also a NO-GO input (real, Metal-backed GL).
- Runtime failures seen only on Linux/Windows CI software-GL runners (llvmpipe, no GPU) are listed as open items with "GO (provisional)", because they do not measure user hardware (RESEARCH Pitfall 4).
- An unpushed CI caps the verdict at "GO (provisional)".
- Whenever the verdict is "GO (provisional)", the doc MUST state `PERF-05: unverified (provisional verdict)` and MUST list `Linux/macOS startup and idle RSS numbers` as an open item that blocks Phase 2 sign-off.
- Fallback toolkit: **Slint 1.18.1**. NO-GO triggers a Slint evaluation spike before Phase 2.

## Measured numbers

Local machine: AMD Ryzen 9 5950X, Intel Arc B580 (driver 32.0.101.6790), Windows 11, 20 GiB sparse file, warm. CI columns are from run 41269609008 (settled-RSS metric); Linux runs under Xvfb + llvmpipe, macOS is the GitHub macos-latest arm64 runner.

| Metric | Threshold | Windows local glow | Windows local wgpu | Linux CI glow | macOS CI glow | Pass? |
|--------|-----------|--------------------|--------------------|---------------|---------------|-------|
| Stripped size | <= 10 MB | 5.01 MB | 9.24 MB | 6.60 MB | 4.07 MB | glow: yes on all OSes (Windows CI 5.0 MB) |
| Startup median | <= 300 ms | 271.7 ms (max 403.1, in-app median 259.5) | 341.1 ms (max 567.7) | 144.6 ms (llvmpipe) | 192.2 ms (earlier run: 501.6 ms) | glow: yes on Windows; macOS passed in the later run but failed (501.6 ms) in the earlier one, so it is not stable |
| Idle RSS (settled min, +20..+30 s) | <= 70 MB | 59.6 MB (startup peak 90.7) | not re-measured (old metric 153.5 MB) | 100.7 MB (llvmpipe, not a NO-GO input) | 102.7 MB (peak 105.9) | Windows glow: yes; macOS glow: NO |
| Idle CPU | < 1 % | 0.35 % | 0.00 % | 0.00 % | 0.00 % | yes |

CI table, all jobs (run 41269609008 = NEW settled metric; run 41268608993 = OLD metric: max of +2 s/+12 s samples, 60 MB budget at the time):

| Job | Size MB | Startup median ms (new / old) | Idle RSS MB (new / old) | Conclusion |
|-----|---------|-------------------------------|-------------------------|------------|
| ubuntu-22.04 glow | 6.60 | 144.6 / 145.9 | 100.7 / 101.1 | green |
| ubuntu-22.04 wgpu | 10.36 | 233.3 / 232.0 | 118.0 / 117.9 | green (report only) |
| macos-latest glow | 4.07 | 192.2 / 501.6 | 102.7 / 106.2 | green (soft CI thresholds) |
| macos-latest wgpu | 6.58 | 273.9 / 260.8 | 90.5 / 90.9 | green |
| windows-latest glow | 5.0 | 355.2 / n.a. | 81.6 / n.a. | old run: size gate step failed (CI bug, see CI); new run green |
| windows-latest wgpu | 9.28 | 494.4 / 631.4 | 54.2 / 54.4 | green (llvmpipe) |

The CI soft thresholds (startup/RSS) are generous runner limits, so a green job does not mean the 300 ms / 70 MB gate passed.

Note: the Windows glow startup max equals run 0 (403.1 ms); the median of 10 warm runs is within the 300 ms gate, but run-to-run spread (257-403 ms) means individual launches can exceed it.

## Idle RSS finding

Resolved by a user decision (see Decisions). The 92.8 MB figure from the original harness was a startup transient; about 30 MB of private memory is genuinely freed around +7-8 s. Steady-state working set is 57.5-59.6 MB for whim and 64 MB for a bare eframe window, so the floor is toolkit plus GPU driver. Details in "Idle RAM isolation" below. The harness now reports the settled minimum (+20..+30 s) as `idle_rss_mb` and the max including startup as `idle_rss_peak_mb`.

## glow vs wgpu

| | glow | wgpu | Delta |
|---|------|------|-------|
| Size | 5.01 MB | 9.24 MB | +4.23 MB |
| Startup median | 265.7 ms | 341.1 ms | +75 ms |
| Idle RSS (old harness metric) | 92.8 MB | 153.5 MB | +61 MB |
| Private bytes (steady) | ~58-60 MB | ~272 MB | +~213 MB |

glow is smaller, faster to start and much lighter in memory; wgpu fails startup and RSS and nearly exhausts the size budget. Recommendation: glow stays the default; wgpu remains an optional cargo feature only. (On the macOS runner wgpu is also over the RSS line, 90.5 MB.)

## IME supported scope

- Windows Japanese (Microsoft IME): PASS on all steps (user-tested). Preedit underline shown, candidate window directly under the caret, Space then Enter commits 日本語, Esc cancels the composition.
- macOS: unverified
- Linux X11/Wayland (ibus/fcitx): unverified
- Known limits: probe caret only; real text editing and caret handling are Phase 3.

## HiDPI / mixed DPI

| Check | Result |
|-------|--------|
| 100% | pass (crisp, even rows) |
| 125% | pass (crisp, even rows) |
| 150% | pass (crisp, even rows) |
| 200% | pass (crisp, even rows) |
| Drag between monitors | not tested (user reported the four scale checks only) |

Unit-test proxy: `view::snap` tests pass.

## CI

Repo: https://github.com/ajoakim/whim (private). Runs:

- 41268608993 (OLD idle metric, 60 MB budget): test jobs green on all 3 OSes; gate macOS glow/wgpu, Linux glow/wgpu and Windows wgpu green; gate Windows glow FAILED at "Size gate (glow, hard)". Root cause was a CI bug, not binary size: the step ran under PowerShell, where `$MAX_SIZE_MB` is not an environment reference, so `--max-mb` swallowed `--label` ("invalid float literal"). Local Windows glow is 5.01 MB. Fixed with `shell: bash` on the size steps (34d9163).
- 41269609008 (NEW settled metric): all 9 jobs green, including Windows glow (5.0 MB).

CI thresholds lowered from baselines (never raised; `MAX_SIZE_MB: "10"` unchanged): `CI_MAX_STARTUP_MS` 2000 -> 1300 (max observed CI median 631.4 ms x 2, rounded up to 100); `CI_MAX_RSS_MB` 200 -> 180 (max observed 118.0 MB x 1.5, rounded up to 10); `CI_MAX_IDLE_CPU` 2.0 unchanged. Linux and Windows CI runtime numbers come from software GL and are not NO-GO inputs (Windows runtime is also continue-on-error); macOS runtime is.

## Cold start

cold not measured, warm-only

Run 0 of the warm series (403.1 ms) is only a cold-ish indicator, not a reboot cold start.

## Decisions

- **Idle RAM metric and budget (USER DECISION, 2026-10-31, overrides the plan rule that thresholds may only be lowered):** the gate metric is the settled working set (minimum over +20..+30 s after launch) and the budget is 70 MB, raised from 60 MB. Rationale: the isolation diagnostic showed the original 92.8 MB was a startup transient freed at about +7-8 s; whim settles at 57.5-58.6 MB and bare eframe at 64 MB, so the floor is toolkit plus GPU driver and Slint would not obviously help. Old 60 MB / early-sample numbers stay in the tables, labelled OLD.
- **Cold start:** not measured (no reboot); recorded as warm-only.

## Rationale

Windows real hardware passes every threshold under the decision rule as amended: glow is 5.01 MB (limit 10), median startup 271.7 ms (limit 300; spread 257-403 ms), settled idle RSS 59.6 MB (limit 70; startup peak 90.7 MB), idle CPU 0.35 %. glow beats wgpu (9.24 MB, 341 ms, 153.5 MB on the old metric). Windows IME and HiDPI manual checks pass.

The decision rule makes a runtime-budget failure on the macOS CI runner a NO-GO input. On the settled metric the macOS glow idle RSS is 102.7 MB (wgpu 90.5 MB), above 70 MB, and the earlier run measured macOS glow startup median 501.6 ms (above 300 ms) although the later run gave 192.2 ms. The literal verdict is therefore NO-GO, with Slint 1.18.1 as the fallback toolkit to evaluate before Phase 2.

**Review note:** this NO-GO rests on macOS RSS (and one noisy startup run). Caveats the user may weigh: the macOS runner is a virtualized arm64 VM; resident size on macOS includes shared framework pages (AppKit/Metal) and is not directly comparable to the Windows working set; wgpu is also over 70 MB there, so the cost is not specific to glow; and the Windows isolation finding (floor is toolkit plus driver) suggests Slint may not do better. If the user judges the macOS figure a measurement artifact, the alternative reading is GO (provisional) with a real-hardware macOS measurement as a blocking open item (PERF-05: unverified (provisional verdict); Linux/macOS startup and idle RSS numbers). That is the user's call; this document does not soften the literal result.

Open items regardless of verdict:

- Real-hardware macOS and Linux startup and idle RSS numbers (CI only so far).
- Cold start never measured on Windows.
- Drag between monitors with different DPI untested.
- macOS and Linux IME unverified.

## Risks carried forward

- Mapped-file truncation by another process (Phase 2).
- macOS Apple Event open-file path (Phase 8).
- CI software GL is not representative of user hardware.
- Accessibility of the custom-painted view.
- accesskit size vs accessibility: glow + accesskit is 5.32 MB vs 5.01 MB default (+0.31 MB, `size-windows-local-glow-accesskit.json` minus `size-windows-local-glow.json`), so it still fits well under 10 MB. It is off by default to save size, so menus, tabs and the status bar get no screen-reader exposure until it is enabled. The cost is small enough that enabling it by default should be reconsidered.
- Idle RSS: the Windows metric question is resolved (Decisions); the macOS floor of about 100 MB is the open problem.
- macOS startup is noisy on the CI runner (501.6 ms vs 192.2 ms between runs).

## Idle RAM isolation (2026-10-31)

Method: PowerShell `Get-Process` polled every 1 s for 40 s per build (WorkingSet64 / PrivateMemorySize64; PagedMemorySize64 was identical to private in every sample). Machine as above. Bare app = `target/diag/bare-eframe/` (own workspace, eframe 0.37.0 glow + default_fonts, same release profile, explicit `Renderer::Glow`, one label, no custom painting). Mesa = copy of whim.exe in `target/diag/mesa/` with pal1000 mesa 26.2.5 `opengl32.dll` + `libgallium_wgl.dll` (archive SHA-256 verified).

| build | WS peak | WS settled | private settled | settle time |
|-------|---------|------------|-----------------|-------------|
| whim glow, no file | 89.0 MB | 57.5 MB | 59.0 MB (peak 90.9) | WS drops at +7 s, flat after |
| whim glow, 20 GiB sparse file | 91.2 MB | 58.6 MB | 99.4 MB (peak 133.0) | WS drops at +7 s to 60.0, ~58.5 by +16 s |
| bare eframe glow (label only) | 97.5 MB | 64.0 MB | 57.4 MB (peak 91.3) | drops at +8 s to 66.0, 64.0 by +23 s |
| whim glow + Mesa llvmpipe | 115.8 MB | 115.8 MB | 104.5 MB | no drop in 40 s |

Findings:

- The drop is a real release, not a working-set trim: private bytes fall in lockstep with the working set. About 30 MB of startup-transient allocations are freed about 7-8 s after launch.
- The 20 GiB file adds no working set; it adds about 40 MB of commit (file-backed section accounting), not resident RAM.
- Attribution: egui/eframe plus the GL driver is about 64 MB (bare eframe); whim's own additions are not measurable. Software GL (Mesa llvmpipe) is much larger (116 MB), so driver choice swings the number by tens of MB. Slint was not tested.
- Outcome: the harness now measures the settled minimum (+20..+30 s); local re-run gives 59.6 MB (peak 90.7 MB).

## Comparable footprint metric (re-measure)

The harness now gates `idle_footprint_mb`, an OS-specific counter that is comparable across OSes: `phys_footprint` on macOS (`proc_pid_rusage`, what Activity Monitor shows, excludes shared framework pages), PSS from `/proc/<pid>/smaps_rollup` on Linux, working set on Windows (private bytes also reported). Same window as before (min of 1 s samples from +20 s to +30 s). Raw `idle_rss_mb` is kept, report only. The bare column is `examples/bare.rs` (eframe 0.37 glow, one label, no file), measured the same way. CI run 41270819589; local Windows run is real hardware, 20 GiB sparse file for whim. Budget is 70 MB.

| OS/runner | backend | metric | whim settled | bare-eframe settled | whim minus bare | startup median |
|---|---|---|---|---|---|---|
| Windows 11 local (real GPU) | glow | working_set | 59.9 MB | 57.4 MB | +2.5 MB | 269.8 ms |
| windows-latest CI (VM, Mesa llvmpipe software GL) | glow | working_set | 82.9 MB | 80.0 MB | +2.8 MB | 265.3 ms |
| ubuntu-22.04 CI (VM, Xvfb + llvmpipe software GL) | glow | pss | 75.1 MB | 71.7 MB | +3.4 MB | 128.9 ms |
| macos-latest CI (arm64 VM) | glow | phys_footprint | 37.6 MB | 38.9 MB | -1.3 MB (noise) | 216.7 ms |

Supplementary: macOS raw RSS was 103.2 MB (whim) / 103.9 MB (bare), so the earlier 102.7 MB figure was dominated by shared framework pages that `phys_footprint` does not count. Linux private (Private_Clean+Private_Dirty) 54.1 MB (whim) / 50.4 MB (bare). Windows local private bytes 100.5 MB (whim, includes about 40 MB commit from the mapped 20 GiB file) / 59.2 MB (bare).

### Re-evaluation inputs

Under the comparable per-OS metric (CI run 41270819589, https://github.com/ajoakim/whim/actions/runs/41270819589), whim is within the 70 MB budget on Windows real hardware (59.9 MB) and on macOS (phys_footprint 37.6 MB). It is above 70 MB on the two software-GL CI VMs: windows-latest 82.9 MB (Mesa llvmpipe) and ubuntu-22.04 75.1 MB PSS (llvmpipe); these two figures are inflated by software GL and are VM numbers, as is the macOS one (arm64 VM, but it uses the real Apple GL stack). Whim exceeds the bare eframe floor by only 2.5 to 3.4 MB on Windows and Linux and by -1.3 MB (noise) on macOS, so essentially all idle memory is the toolkit plus GL driver, not whim code. The original macOS RSS NO-GO (102.7 MB) was an artefact of counting shared framework pages. Verdict above is unchanged; the orchestrator and user decide.
