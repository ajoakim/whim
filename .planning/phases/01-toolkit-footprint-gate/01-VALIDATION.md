---
phase: 1
slug: toolkit-footprint-gate
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-10-31
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | built-in `cargo test` (Rust 1.99.0 via rust-toolchain.toml) + `cargo xtask` budget checks |
| **Config file** | none — Wave 0 creates workspace, `xtask/`, `.cargo/config.toml` alias |
| **Quick run command** | `cargo test --workspace --lib` |
| **Full suite command** | `cargo test --workspace && cargo build --release && cargo xtask check-size && cargo xtask measure --runs 5` |
| **Estimated runtime** | ~120 seconds (release build dominates) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace --lib` and `cargo clippy --workspace -- -D warnings`
- **After every plan wave:** Run full suite (`cargo test --workspace` + release build + `cargo xtask check-size`)
- **Before `/gsd-verify-work`:** Full suite green locally; CI matrix green (size hard, runtime soft); manual IME/HiDPI checklists complete
- **Max feedback latency:** 120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD by planner | — | — | PERF-01 | — | N/A | integration | `cargo xtask measure --runs 10 --max-startup-ms 300` | ❌ W0 | ⬜ pending |
| TBD by planner | — | — | PERF-02 | — | N/A | integration | `cargo xtask measure --max-rss-mb 60 --max-idle-cpu 1.0` | ❌ W0 | ⬜ pending |
| TBD by planner | — | — | PERF-02 | — | mmap never reads whole file | integration | `cargo xtask sparse --size 20GiB out.bin && cargo xtask measure --file out.bin` | ❌ W0 | ⬜ pending |
| TBD by planner | — | — | PERF-05 | — | N/A | CI matrix | GitHub Actions job `gate` | ❌ W0 | ⬜ pending |
| TBD by planner | — | — | (gate) | — | N/A | size | `cargo xtask check-size --max-mb 10` | ❌ W0 | ⬜ pending |
| TBD by planner | — | — | PERF-02 | — | bounds-checked row slicing / EOF clamp | unit | `cargo test -p whim view::` | ❌ W0 | ⬜ pending |
| TBD by planner | — | — | UI-04 | — | N/A | unit | `cargo test -p whim view::snap` | ❌ W0 | ⬜ pending |
| TBD by planner | — | — | (sparse) | — | N/A | integration | `cargo test -p xtask sparse` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `rust-toolchain.toml`, workspace `Cargo.toml`, `.cargo/config.toml` (xtask alias, crt-static)
- [ ] `xtask/` crate with `sparse`, `measure`, `check-size` subcommands
- [ ] Unit test modules in `src/view.rs` (row slicing, snapping)
- [ ] `.github/workflows/ci.yml`
- [ ] `docs/measurements.md` template, `01-GATE.md` template

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Crisp text on HiDPI / mixed DPI | UI-04 | Visual judgement | Run at 100/125/150/200% scaling and drag across monitors; record screenshots in 01-GATE.md |
| Windows Japanese IME compose/convert/commit/cancel | (IME scope) | Needs a real IME | Enable MS-IME, type in the view, check preedit underline, candidate window position, commit and Esc-cancel |
| Cold-start timing on local Windows | PERF-01 | CI cannot do a true cold cache | Reboot or flush the standby list, then run `cargo xtask measure --runs 10`; record in docs/measurements.md |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 120s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
