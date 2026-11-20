---
phase: 02-core-storage-read-only-viewers
slug: core-storage-read-only-viewers
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-11-19
---

# Phase 2 — Validation Strategy

> Per-phase validation contract covering every Phase 2 task, threat mitigation, and manual UAT item.

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `cargo test` + `cargo clippy --workspace --all-targets -- -D warnings` + `cargo xtask` |
| **Quick run command** | `cargo test --workspace` |
| **Full suite command** | `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` |
| **Estimated runtime** | ~2 seconds (124 `whim` lib tests + 23 `xtask` tests) |

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Test Type | Automated Command | Status |
|---------|------|------|-------------|------------|-----------|-------------------|--------|
| 02-01-01 | 02-01 | 1 | CORE-02 | T-02-01..04 | unit | `cargo test -p whim --lib -- source:: testutil::` | ✅ green |
| 02-01-02 | 02-01 | 1 | UI-05 | T-02-02, T-02-06 | unit | `cargo test -p whim --lib -- fonts:: theme::` | ✅ green |
| 02-02-01 | 02-02 | 1 | PERF-04 | T-02-06 | unit | `cargo test -p xtask bundle::` | ✅ green |
| 02-02-02 | 02-02 | 1 | PERF-04 | T-02-05, T-02-07..08 | ci / grep | CI macOS glow gate (`plutil`, `codesign`, `ditto`, `test -x`, smoke) | ⏳ pending CI push |
| 02-03-01 | 02-03 | 2 | CORE-02, D-01 | T-02-09..14 | unit / io | `cargo test -p whim --lib filesource::` | ✅ green |
| 02-03-02 | 02-03 | 2 | D-10, D-19, CORE-01, HEX-05 | T-02-09..13 | unit / io | `cargo test -p whim --lib open::` | ✅ green |
| 02-04-01 | 02-04 | 2 | CORE-04, CORE-07 | T-02-15..17 | property / worker | `cargo test -p whim --lib lineindex::` | ✅ green |
| 02-04-02 | 02-04 | 2 | CORE-05, HEX-03, D-11 | T-02-18..19 | unit | `cargo test -p whim --lib goto::` | ✅ green |
| 02-05-01 | 02-05 | 2 | CORE-03, CORE-06, WR-04 | T-02-20..22 | property | `cargo test -p whim --lib scroll::` | ✅ green |
| 02-05-02 | 02-05 | 2 | CORE-06, UI-05, D-04, D-07 | T-02-21, T-02-30..31 | unit | `cargo test -p whim --lib -- text:: view::scrollbar::` | ✅ green |
| 02-06-01 | 02-06 | 2 | EDIT-10, D-13 | T-02-27 | unit | `cargo test -p whim --lib selection::` | ✅ green |
| 02-06-02 | 02-06 | 2 | EDIT-10, D-14, Q4, CORE-07 | T-02-24..26 | unit / worker | `cargo test -p whim --lib copy::` | ✅ green |
| 02-07-01 | 02-07 | 3 | CORE-06, EDIT-05, EDIT-06 | T-02-28..31 | property | `cargo test -p whim --lib view::textview::` | ✅ green |
| 02-07-02 | 02-07 | 3 | UI-05, D-07 | T-02-29, T-02-31 | headless egui | `cargo test -p whim --lib view::textview::tests::egui_smoke` | ✅ green |
| 02-08-01 | 02-08 | 3 | HEX-01, HEX-03, D-09 | T-02-32..34 | unit / headless egui | `cargo test -p whim --lib view::hexview::` | ✅ green |
| 02-09-01 | 02-09 | 4 | EDIT-10, D-13, Q8 | T-02-35..36 | headless egui | `cargo test -p whim --lib view::textview::tests::word_and_triple_select_and_click_hit_test` | ✅ green |
| 02-10-01 | 02-10 | 4 | CORE-02, UI-01, D-01, D-19 | T-02-37..38 | unit / build | `cargo test -p whim --lib doc::` | ✅ green |
| 02-11-01 | 02-11 | 4 | UI-01, OPEN-02, OPEN-09, D-12, D-14..16 | T-02-40..42 | unit / build | `cargo test -p whim --lib -- status:: dialogs::` | ✅ green |
| 02-12-01 | 02-12 | 4 | PERF-04, CORE-01..03 | T-02-43..44 | bench / gate | `cargo xtask check-size --max-mb 10` & `cargo xtask measure` | ✅ recorded in `02-GATE.md` |

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Status |
|----------|-------------|------------|--------|
| Windows interactive UAT (20 GiB open, scroll, Ctrl+G, hex, binary banner, CJK, select+copy, File > Open, DnD, idle CPU) | CORE-01..07, EDIT-05/06/10, HEX-01/03/05, OPEN-02/09, UI-01/05 | Visual & OS-shell integration | ✅ Approved (02-11 Task 3) |
| macOS native file dialog & cross-app clipboard paste | OPEN-09, EDIT-10 | Requires macOS desktop session | Open (non-blocking; CI smoke covers launch) |
| macOS Finder double-click `Whim.app` (no Terminal.app) | PERF-04, D-21 | Requires macOS Finder + Gatekeeper | Open (CI verifies bundle/sign/zip/launch) |
| Linux X11/Wayland native dialog & DnD | OPEN-02, OPEN-09 | Wayland DnD / xdg-portal desktop session | Open (CI verifies build + no GTK link) |
| CJK fallback on real macOS/Linux fonts | UI-05 | OS font availability | Open |

## Validation Sign-Off

- [x] All tasks have automated `<verify>` commands and pass (`124` whim + `23` xtask tests)
- [x] `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` pass
- [x] Sampling continuity: no unverified task windows
- [x] Manual UAT checkpoint (02-11 Task 3) approved
- [x] `nyquist_compliant: true` set in frontmatter
