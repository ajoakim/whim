---
phase: 01-toolkit-footprint-gate
plan: 03
subsystem: measurement
tags: [xtask, sparse, ci, github-actions, footprint]
requires: [01-01]
provides:
  - "cargo xtask sparse | check-size | measure"
  - ".github/workflows/ci.yml (test + 3-OS x 2-backend gate)"
  - "docs/measurements.md standing table"
affects: [01-04, 01-05]
tech-stack:
  added: []
  patterns: [exit 2 = launch failure, exit 1 = budget failure, MB = MiB]
key-files:
  created: [xtask/src/sparse.rs, xtask/src/size.rs, xtask/src/measure.rs, .github/workflows/ci.yml, docs/measurements.md]
  modified: [xtask/src/main.rs, xtask/src/lib.rs]
key-decisions:
  - "Windows sparse via windows-sys FSCTL_SET_SPARSE (no fsutil fallback needed); on-disk size via GetCompressedFileSizeW"
  - "Mesa llvmpipe pinned to pal1000/mesa-dist-win 26.2.5 release-msvc, SHA-256 997123ef...53c2 (verified by local download)"
requirements-completed: [PERF-01, PERF-02, PERF-05]
completed: 2026-10-31
---

# Phase 1 Plan 03: Measurement Harness and CI Gate Summary

Rust xtask with a real sparse 20 GiB file helper, a hard stripped-size gate and a startup/idle measure harness (JSON, distinct exit codes), plus a SHA-pinned GitHub Actions 3-OS x 2-backend gate workflow and the standing measurements table.

## Tasks
1. sparse + check-size + dispatch - 8ffdc87
2. measure subcommand - 39163b3
3. CI workflow and docs/measurements.md - see git log (`ci(01-03)` commit)

## Verification
- `cargo test -p xtask`: sparse 4 passed, size 3 passed, measure 4 passed. Clippy `-D warnings` clean.
- `cargo xtask sparse --size 20GiB`: apparent_bytes=21474836480, on_disk_bytes=0 on local NTFS; second run without `--force` exits 1; test file deleted.
- `check-size --bin Cargo.toml --max-mb 0.0001` exits 1 (FAIL); `cargo xtask bogus` exits 2.
- `measure --help` exits 0; `measure --bin Cargo.toml` exits 2 (launch failure path).
- ci.yml parses with js-yaml (npx available); all 7 `uses:` are 40-char SHA pinned.

## Deviations from Plan
- sysinfo 0.40 `System::new()` returns `Result`; handled (exit 2 on failure). Plan flagged this signature as adjustable (A7).
- Windows sparse path compiled as written; fsutil fallback not used (A1 held).
- Actions pinned: checkout v7.0.1, rust-cache v2.9.3, upload-artifact v7.0.2, rust-toolchain master 683b588.

## Not verified here
- The measure idle-sampling path (RSS/CPU) was not exercised against a real process: whim `--bench` comes from plan 01-02. End-to-end run is plan 01-04.
- The Mesa archive layout (`x64/opengl32.dll`, `x64/libgallium_wgl.dll`) could not be checked locally (no 7z); the hash was verified. The CI step follows the plan's stated layout and will fail loudly if it differs. The Windows measure step is continue-on-error.
- CI workflow itself has not run.

## Known Stubs
None.

## Self-Check: PASSED
Files exist; commits 8ffdc87 and 39163b3 present; CI commit present on branch.
