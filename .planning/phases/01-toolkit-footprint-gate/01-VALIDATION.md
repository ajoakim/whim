---
phase: 1
slug: toolkit-footprint-gate
status: planned
nyquist_compliant: true
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
| 01-01-T1 | 01-01 | 1 | (scaffold) | — | N/A | build | `cargo build --workspace --locked && cargo xtask; test $? -eq 2` | ❌ W0 | ⬜ pending |
| 01-01-T2 | 01-01 | 1 | PERF-02 | — | bounds-checked row slicing / EOF clamp | unit | `cargo test -p whim view::` | ❌ W0 | ⬜ pending |
| 01-01-T2 | 01-01 | 1 | UI-04 | — | N/A | unit | `cargo test -p whim view::snap` | ❌ W0 | ⬜ pending |
| 01-02-T1 | 01-02 | 2 | PERF-01, PERF-02 | — | read-only mmap, no whole-file read | unit | `cargo test -p whim --lib args:: bench:: file:: && cargo build -p whim` | ❌ W0 | ⬜ pending |
| 01-02-T2 | 01-02 | 2 | PERF-01, UI-04 | — | N/A | smoke | `cargo test -p whim --lib && cargo build --release -p whim && whim --bench --bench-exit-after 3` (greps `startup_ms=`, `backend=glow`) | ❌ W0 | ⬜ pending |
| 01-03-T1 | 01-03 | 2 | (sparse), (gate) | — | N/A | unit + integration | `cargo test -p xtask sparse && cargo test -p xtask size && cargo xtask sparse --size 20GiB target/t.sparse.bin` | ❌ W0 | ⬜ pending |
| 01-03-T2 | 01-03 | 2 | PERF-01, PERF-02 | — | N/A | unit | `cargo test -p xtask measure && cargo xtask measure --help` | ❌ W0 | ⬜ pending |
| 01-03-T3 | 01-03 | 2 | PERF-05 | T-01-12 | hash-pinned Mesa download | CI config | `npx --yes js-yaml .github/workflows/ci.yml` + grep `cargo xtask check-size` | ❌ W0 | ⬜ pending |
| 01-04-T1 | 01-04 | 3 | (IME scope) | — | N/A | unit | `cargo test -p whim --lib ime::` | ❌ W0 | ⬜ pending |
| 01-04-T2 | 01-04 | 3 | UI-04, PERF-05 | — | N/A | smoke | `cargo test --workspace --lib` + wgpu bench smoke + `cargo xtask measure --runs 3 --label smoke-glow` | ❌ W0 | ⬜ pending |
| 01-05-T1 | 01-05 | 4 | PERF-01, PERF-02 | — | mmap never reads whole file (20 GiB sparse) | integration | `cargo xtask check-size --max-mb 10` + `cargo xtask measure --runs 10 --max-startup-ms 300 --max-rss-mb 60 --max-idle-cpu 1.0` (glow, nofont, accesskit, wgpu JSON exist) | ❌ W0 | ⬜ pending |
| 01-05-T2 | 01-05 | 4 | UI-04, PERF-01, PERF-05 | — | N/A | manual (checkpoint) | see Manual-Only Verifications | n/a | ⬜ pending |
| 01-05-T3 | 01-05 | 4 | PERF-05, PERF-01 | T-01-19 | thresholds never raised; MAX_SIZE_MB stays 10 | doc + CI config | verdict grep, cold/warm-only grep, provisional open-items grep, `npx --yes js-yaml .github/workflows/ci.yml` | ❌ W0 | ⬜ pending |

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
| Cold-start timing on local Windows | PERF-01 | CI cannot do a true cold cache | Reboot or flush the standby list, then run `cargo xtask measure --runs 10 --label windows-local-glow-cold`; record in docs/measurements.md and 01-GATE.md, or record exactly "cold not measured, warm-only" in 01-GATE.md |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (01-05-T2 is a human-verify checkpoint)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [ ] Feedback latency < 120s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
