---
phase: 01-toolkit-footprint-gate
verified: 2026-10-31T00:00:00Z
status: human_needed
score: 3/5 must-haves verified (2 partial, accepted-with-caveats pending user sign-off)
overrides_applied: 0
gaps: []
human_verification:
  - test: "Cold-start measurement on Windows (after reboot) and on real macOS/Linux hardware"
    expected: "Window with virtualized view in about 200 ms (gate 300 ms) from a true cold start"
    why_human: "Only warm runs were measured (about 270 ms median, spread 257-403 ms). Needs a reboot or real hardware."
  - test: "Drag the window between monitors with different DPI"
    expected: "Text stays crisp after the scale-factor change"
    why_human: "Untested. Single-scale 100/125/150/200% checks passed."
  - test: "IME on macOS and Linux (ibus/fcitx)"
    expected: "Preedit and candidate window behave as on Windows"
    why_human: "Unverified. Only Windows Japanese IME was tested."
  - test: "Accept or tighten CI runtime budgets"
    expected: "Decide whether CI thresholds of 1300 ms / 180 MB satisfy 'CI enforces budgets'"
    why_human: "Judgement call, see SC5."
---

# Phase 1: Toolkit & Footprint Gate Verification Report

**Phase Goal:** GUI toolkit and footprint targets proven on Windows, Linux and macOS with measured numbers.
**Status:** human_needed (no hard blocker; the user-accepted verdict is GO on egui/glow, but 3 success criteria are only partly met)
**Re-verification:** No

## Observable Truths (ROADMAP success criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Skeleton launches on 3 OSes with a virtualized monospace view in about 200 ms cold | PARTIAL (WARNING) | App exists (src/app.rs, src/view/) and launched on all 3 OSes in CI. Cold start was never measured. Warm Windows median is 271.7 ms (max 403.1), which is above the 200 ms target but within the 300 ms gate. macOS CI startup swung from 501.6 ms to 192.2 ms between runs. |
| 2 | 20 GB sparse file opens, first page shown, file not read into memory | VERIFIED | src/file.rs uses `memmap2::Mmap::map`, with no `read_to_end` or `fs::read` anywhere in src. Measured with a 20 GiB sparse file: working set unchanged (about 58.6 MB), about 40 MB commit only. CI measure uses the same file on all OSes. |
| 3 | Idle RAM in tens of MB, CPU near zero, numbers recorded for glow vs wgpu | VERIFIED (with caveat) | docs/measurements.md has glow and wgpu rows. Windows real GPU is 59.9 MB working set and 0.35 % CPU. macOS phys_footprint is 37.6 MB. Linux PSS is 75.1 MB and Windows CI is 82.9 MB, both on software GL; the bare eframe window is within 3.4 MB of them. The 70 MB budget was raised from 60 MB by user decision. The measured whim-minus-bare gap is only +2.5 to +3.4 MB, so the cost is the toolkit and driver, not whim code. |
| 4 | Crisp on HiDPI and mixed-DPI; IME checked and scope documented | PARTIAL (WARNING) | HiDPI 100/125/150/200% passed by user (manual). Mixed-DPI drag is untested. IME is documented in measurements and GATE: Windows PASS, macOS and Linux "unverified". The scope is documented as required, but it is Windows-only. |
| 5 | Written go/no-go with named fallback, backed by numbers; CI enforces startup, size and idle budgets | PARTIAL (WARNING) | 01-GATE.md: verdict GO, fallback Slint 1.18.1 named, numbers cited, NO-GO history kept. CI (.github/workflows/ci.yml) hard-gates size at 10 MB. Startup (1300 ms) and RSS (180 MB) thresholds are generous soft limits, and Windows measure and all wgpu jobs are `continue-on-error` or report-only. The 70 MB and 300 ms gate values are therefore not enforced in CI. Idle CPU limit is 2 %. |

**Score:** 2 verified, 3 partial.

## Requirements Coverage

| Requirement | Plans | Status | Evidence |
|-------------|-------|--------|----------|
| PERF-01 (window in about 200 ms cold) | 02, 03, 05 | PARTIAL | Warm about 270 ms, cold not measured, 200 ms target not met (gate 300 ms met). REQUIREMENTS.md still says Pending. |
| PERF-02 (idle RAM tens of MB, CPU near zero) | 01-05 | SATISFIED | See SC3. |
| PERF-05 (runs on Win/Linux/macOS) | 03, 04, 05 | SATISFIED for build and launch | CI builds, tests and measures on all three. Only CI (VM) evidence exists for Linux and macOS, no real hardware. |
| UI-04 (crisp on HiDPI and mixed-DPI) | 01, 02, 04, 05 | PARTIAL | Mixed-DPI untested. |

All four phase IDs appear in plan frontmatter and in REQUIREMENTS.md. No orphaned requirements. REQUIREMENTS.md traceability still marks all four "Pending" and should be updated.

## Local checks run

- `cargo test --workspace`: all pass (33 + 18 + others, 0 failed).
- `cargo clippy --workspace --all-targets -- -D warnings`: clean.
- I did not run `cargo build --release` or any measurement or GUI process.

## Anti-Patterns / Review

- 01-REVIEW.md: fixes landed. Deferred to Phase 2: SIGBUS or UB if another process truncates a mapped file (hard Phase 2 requirement), keyboard-scroll gating, and a FIFO or device blocking `File::open` on the main thread (src/file.rs:52). These are non-blocking for the gate but are real risks.
- Doc inconsistency: 01-GATE.md still says the repo is private in docs/measurements.md ("private") while the user says it is public. The "Measured numbers" table still shows macOS glow "NO" on the superseded raw-RSS metric, which can confuse readers; the Verdict header explains the supersession.
- The GATE decision rule text still reads "any runtime failure on macOS CI is NO-GO". The GO verdict depends on the user's mid-gate change of metric and budget (60 to 70 MB, working set to per-OS footprint). That is documented and user-authorised, but the thresholds were adjusted after the numbers were seen.

## Gaps Summary

No hard blockers: the goal "toolkit proven with measured numbers, go/no-go written" is met, and the GO decision is the user's. Remaining caveats: cold start unmeasured and the 200 ms target not met even warm, mixed-DPI untested, macOS and Linux IME unverified, no real-hardware Linux or macOS numbers, and CI does not enforce the actual 300 ms / 70 MB gate values. Recommended: carry these as tracked open items into Phase 2 (and add the real-hardware macOS and Linux checks), tighten CI thresholds if feasible, and update REQUIREMENTS.md status.

_Verifier: Claude (gsd-verifier)_
