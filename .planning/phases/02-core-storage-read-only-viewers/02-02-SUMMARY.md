---
phase: 02-core-storage-read-only-viewers
plan: 02
subsystem: ci-packaging
tags: [macos, xtask, ci, footprint-gate]
requires: []
provides: [xtask/src/bundle.rs, macos-app-bundle, macos-ci-hard-gate]
key-files.created:
  - xtask/src/bundle.rs
key-files.modified:
  - xtask/src/lib.rs
  - xtask/src/main.rs
  - .github/workflows/ci.yml
  - README.md
key-decisions:
  - `cargo xtask bundle-macos` uses only `std::fs` and never invokes `codesign`, so it runs and is unit-tested on Windows, Linux, and macOS
  - macOS CI hard-gates real-GL idle footprint at 70 MB and startup median at 600 ms (D-18) while software-GL runners stay report-only
  - CI uploads `Whim-macos-arm64.zip` created via `ditto --sequesterRsrc --keepParent` to preserve the executable bit
requirements-completed:
  - PERF-04
---

# Phase 02 Plan 02: macOS Whim.app Bundle & CI Hard Gate Summary

Delivered `cargo xtask bundle-macos` for assembling unsigned `Whim.app` bundles, hard-gated macOS real-GL runtime budgets in CI, and documented macOS first-launch routes in README.

### What was built

* `xtask/src/bundle.rs`: std-only bundle builder generating `Whim.app/Contents/{Info.plist, PkgInfo, MacOS/whim, Resources/}` with `io.github.ajoakim.whim`, `NSHighResolutionCapable`, `LSMinimumSystemVersion = 11.0`, XML escaping, and `[package].version` extraction. 5 unit tests pass.
* `.github/workflows/ci.yml`: added `MACOS_MAX_STARTUP_MS: "600"` and `MACOS_MAX_FOOTPRINT_MB: "70"` for `Measure (macOS)`, plus `Bundle Whim.app (macOS, glow)` (lint, ad-hoc sign, verify, `ditto` zip, extract, exec-bit check, smoke launch) and artifact upload.
* `README.md`: added `## macOS` section covering macOS 15+ Open Anyway, macOS 14 and earlier right-click Open, and `xattr -dr com.apple.quarantine`.

### Deviations from Plan

None - plan executed exactly as written.

## Self-Check: PASSED

Wave 1 complete. Ready for Wave 2.
