---
phase: 01-toolkit-footprint-gate
plan: 05
subsystem: infra
tags: [egui, eframe, glow, gate, measurement, ci]

requires:
  - phase: 01-toolkit-footprint-gate
    provides: walking skeleton, xtask measure/check-size harness, CI workflow
provides:
  - Settled-idle-RSS measurement harness (70 MB budget)
  - docs/measurements.md with local Windows and CI rows
  - 01-GATE.md with a final verdict (NO-GO on macOS CI RSS, pending user review)
  - CI size-gate fix for Windows and thresholds lowered to baselines
affects: [phase-2-planning, toolkit-decision]

tech-stack:
  added: []
  patterns: ["idle RSS = min working set over +20..+30 s window; peak reported separately"]

key-files:
  created: [.planning/phases/01-toolkit-footprint-gate/01-GATE.md, docs/measurements.md]
  modified: [xtask/src/measure.rs, .github/workflows/ci.yml, .planning/PROJECT.md]

key-decisions:
  - "Idle RAM gate metric is the settled working set, budget 70 MB (user decision, overrides lower-only rule)"
  - "Cold start recorded as 'cold not measured, warm-only'"
  - "Literal verdict NO-GO because macOS CI settled idle RSS is 102.7 MB (> 70 MB); flagged for user review"

patterns-established:
  - "Measurement rows labelled OLD vs NEW metric; history is never rewritten"

requirements-completed: []

duration: ~1h (continuation)
completed: 2026-11-01
---

# Phase 1 Plan 05: Toolkit Gate Summary

**Windows real hardware passes the glow gate (5.01 MB, 271.7 ms, 59.6 MB settled RSS), but the macOS CI runner settles at 102.7 MB idle RSS, so the literal decision rule yields NO-GO with Slint 1.18.1 as the fallback; the verdict is flagged for user review.**

## Accomplishments
- Harness now measures steady state: samples once a second over +20..+30 s, reports the minimum as `idle_rss_mb`, the startup-inclusive max as `idle_rss_peak_mb`, idle CPU over the same window; budget 70 MB; unit test added.
- Local Windows re-measurement (10 runs, 20 GiB sparse file, deleted afterwards): startup median 271.7 ms (max 403.1), settled RSS 59.6 MB (peak 90.7), CPU 0.35 %.
- CI numbers from both runs recorded (old metric run 41268608993, new metric run 41269609008, all 9 jobs green).
- 01-GATE.md finalized: IME (Windows Japanese all pass), HiDPI (100/125/150/200 % pass), cold start (warm-only), Decisions, Rationale, Review note.
- CI thresholds lowered: startup 2000 -> 1300 ms, RSS 200 -> 180 MB; `MAX_SIZE_MB` stays 10.
- PROJECT.md records the idle RAM definition and gate outcome.

## Task Commits
1. Task 1: 80cdd5b, d466a1b (diagnostic), 4c41db2 (rustfmt/.gitattributes)
2. Harness + budget: acb20d8 `feat(01-05): measure settled idle RSS; RSS budget 70 MB (user decision)`
3. CI fix: 34d9163 `fix(01-05): run size gate steps under bash ...`
4. Task 3: e83026b `docs(01-05): finalize phase 1 gate verdict ...`

## Deviations from Plan

**1. [Rule 1 - Bug] Windows glow size gate failed in CI**
- **Found during:** CI run 41268608993
- **Issue:** the size-gate steps ran under PowerShell, where `$MAX_SIZE_MB` is empty, so `--max-mb` consumed `--label` (invalid float literal). Not a real size problem (5.0 MB).
- **Fix:** `shell: bash` on both check-size steps; confirmed green in run 41269609008.
- **Commit:** 34d9163

**2. [User decision] RSS budget 60 -> 70 MB and settled metric.** Overrides the plan rule that thresholds may only be lowered. Recorded in GATE Decisions and PROJECT.md.

**3. Leftover whim.exe from the manual checkpoint** locked the release binary; killed it before rebuilding and measuring.

## Issues Encountered
- macOS CI startup was 501.6 ms in run 1 and 192.2 ms in run 2 (noisy runner).
- GitHub API polling kept to few calls; no rate-limit trouble.

## Open Issues / Needs User Review
- Verdict NO-GO is driven solely by macOS CI idle RSS 102.7 MB (wgpu 90.5 MB). The macOS runner is a VM and macOS resident size includes shared framework pages, so the user may reasonably read it as GO (provisional) with PERF-05 unverified. Requirements PERF-01/02/05/UI-04 were NOT marked complete because the gate is unresolved.
- Cold start not measured; monitor-drag DPI untested; macOS/Linux IME unverified.

## Known Stubs
None.

## Self-Check: PASSED
- Commits acb20d8, 34d9163, e83026b exist; 01-GATE.md, docs/measurements.md present; acceptance greps pass.
