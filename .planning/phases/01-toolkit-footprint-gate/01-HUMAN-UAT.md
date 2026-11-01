---
status: partial
phase: 01-toolkit-footprint-gate
source: [01-VERIFICATION.md]
started: 2026-11-01T00:00:00Z
updated: 2026-11-01T00:00:00Z
---

## Current Test

[awaiting human testing]

## Tests

### 1. Cold-start measurement on Windows (after reboot) and on real macOS/Linux hardware
expected: Window with virtualized view in about 200 ms (gate 300 ms) from a true cold start
result: [pending]

### 2. Drag the window between monitors with different DPI
expected: Text stays crisp after the scale-factor change
result: [pending]

### 3. IME on macOS and Linux (ibus/fcitx)
expected: Preedit and candidate window behave as on Windows
result: [pending]

### 4. Accept or tighten CI runtime budgets
expected: Decide whether CI thresholds (1300 ms / 180 MB soft; size 10 MB hard) satisfy "CI enforces budgets"
result: pass — user decision 2026-11-01: hard-gate macOS CI (real GL) at footprint <= 70 MB and startup <= 600 ms; software-GL runners stay report-only; size hard everywhere. Implement in Phase 2 setup.

## Summary

total: 4
passed: 1
issues: 0
pending: 3
skipped: 0
blocked: 0

## Gaps
