---
phase: 01-toolkit-footprint-gate
plan: 04
subsystem: ui
tags: [egui, ime, wgpu, glow, measurement]
requires: [01-02, 01-03]
provides:
  - ImeProbe composition state machine
  - IME wiring in view (focus, Event::Ime, IMEOutput, preedit/caret paint)
  - Verified glow and wgpu builds, first end-to-end xtask measure run
affects: [01-05]
key-files:
  created: [src/ime.rs]
  modified: [src/lib.rs, src/view/mod.rs]
requirements: [UI-04, PERF-05]
completed: 2026-10-31
---

# Phase 1 Plan 04: IME probe and backend smoke Summary

IME preedit/commit/candidate-window positioning is wired into the custom view as an in-memory probe, and both renderer builds plus the xtask harness were proven against the real binary.

## Commits
- 519acb7: ImeProbe state machine (8 unit tests pass)
- 1ca1c95: view IME wiring

## Smoke results (not final; plan 05 does real runs)
- wgpu release (`--no-default-features --features wgpu,bundled-mono`): `backend=wgpu`, startup_ms=653, exe 9.69 MB
- debug build with both features: `backend=glow` (explicit renderer wins)
- glow release: exe 5.26 MB. `cargo xtask measure --runs 3 --idle-secs 5 --label smoke-glow` on 20 GiB sparse file: first_run 393.6 ms, startup_median 273.4 ms, in-app median 260 ms, idle_rss 90.6 MB, idle_virtual 133.1 MB, idle_cpu 0.00%. JSON at target/measure/measure-smoke-glow.json (contains all required keys, backend glow).

## Deviations from Plan
- [Rule 3] The plan's interface listed `egui::IMEOutput`; in egui 0.37 it lives at `egui::output::IMEOutput` (IMEPurpose is at the root). Used the correct path. `ImeEvent::Preedit` has `active_range_chars: Option<Range<usize>>`.
- No bugs found in `xtask/src/measure.rs`; idle RSS/CPU sampling worked first time and no GUI processes were left running.

## Notes for plan 05
- Idle RSS of ~90 MB on Windows/glow is well above the "tens of MB" aspiration in PROJECT constraints. Plan 05 should record it against the budget and investigate (GL driver contribution likely).
- Windows Japanese IME manual test is still to be done by a human; the probe is ready (Esc outside composition clears the probe line).

## Known Stubs
None. The IME probe is intentionally in-memory only (file stays read-only).

## Self-Check: PASSED
