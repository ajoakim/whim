---
phase: 1
reviewed: 2026-11-01
depth: standard
files_reviewed: 23
status: issues_found
findings:
  critical: 3
  warning: 8
  info: 3
  total: 14
---

# Phase 1 Code Review

Standard-depth static review (source read, not executed). Not opened: `src/theme.rs`, `examples/bare.rs`, `src/lib.rs`, `xtask/Cargo.toml`. No integer overflow / OOB found in row/offset math (`clamp_offset`, `row_end`, `prev_row_start` bound slices correctly; f64 thumb math fine at 20 GB).

## Critical

### BL-01: Gate passes with zeros if child dies during idle window
**File:** `xtask/src/measure.rs:424-446, 450-459`
When `snapshot()` returns `None` the loop warns and breaks; `window_min(&[])` returns 0.0 and `idle_cpu` stays 0.0, so `evaluate()` passes. A crash or early `--bench-exit-after` turns the footprint/CPU gate green. Same if `idle_secs` is 0 or the window never reached `SETTLE_START_SECS`.
**Fix:** exit code 2 when `win_fp` is empty or the child exited early; check `child.try_wait()` before reporting.

### BL-02: Child process leaked on bad `first_frame_unix_us` or panic after spawn
**File:** `xtask/src/measure.rs:188-190`
`.map_err(...)?` returns from `launch` with the child alive and never killed/waited; same for `.expect()` / sysinfo init panics. Only mitigation is the child's 75 s `--bench-exit-after`.
**Fix:** RAII guard around `Child` whose `Drop` calls `kill()` + `wait()`, used on all exit paths of `launch` and `run_inner`.

### BL-03: CI Windows measure step cannot fail the build
**File:** `.github/workflows/ci.yml:107`
`continue-on-error: ${{ runner.os == 'Windows' }}` makes the Windows runtime gate advisory. Windows CI uses software GL (llvmpipe) by design and is not a NO-GO input per 01-GATE.md, so this is intentional — but it must be stated explicitly (report-only) in the workflow and docs rather than look like an enforcing gate.
**Fix:** label the step report-only and document it; keep the size gate hard.

## Warnings

### WR-01: Unsound mmap if file truncated while mapped
**File:** `src/file.rs:12, 66-71`
`&[u8]` over a mapping another process can truncate → SIGBUS on Linux/macOS (immediate kill with `panic = "abort"`); external mutation is UB for `&[u8]`. Windows largely mitigated (truncating a mapped file fails). Deferred to Phase 2 in comments; must be tracked as a hard Phase 2 requirement.

### WR-02: End key does not reach EOF
**File:** `src/view/mod.rs:114-117`
`top = len - n*128` assumes 128-byte rows; rows break at `\n`, so End lands far from EOF / mid-line.
**Fix:** walk backwards with `rows::scroll_rows(data, len, -(n as i64 - 1))`.

### WR-03: Key/scroll input applied when view not focused
**File:** `src/view/mod.rs:75-79`
Arrow/Page keys typed in the About window or with a menu open scroll the file.
**Fix:** gate key handling on `response.has_focus()` and no other widget wanting keyboard input.

### WR-04: Scroll up and down are not inverse
**File:** `src/view/rows.rs:52-69`
`prev_row_start` vs 128-byte `row_end` splitting disagree on lines >128 bytes; rows jump or repeat. Add property test `prev(row_end(x)) <= x`.

### WR-05: Predictable shared temp path for 20 GB sparse file
**File:** `xtask/src/measure.rs:345`
`temp_dir()/whim-measure-20g.sparse.bin` with truncate+force: symlink attack on shared Linux `/tmp`, concurrent-run collisions, not removed on panic.
**Fix:** per-process name / `create_new`, cleanup guard.

### WR-06: Unpinned `cargo install cargo-bloat`
**File:** `.github/workflows/ci.yml:156`
Supply-chain gap in a public repo. **Fix:** `--version X.Y.Z --locked` or drop.

### WR-07: checkout persists credentials; push + pull_request double-runs
**File:** `.github/workflows/ci.yml:3-6, 33, 60`
**Fix:** `persist-credentials: false` on checkouts; restrict `push` to `branches: [main]`.

### WR-08: wgpu gate job enforces budgets though advertised report-only
**File:** `.github/workflows/ci.yml:96-120`
**Fix:** drop `--max-*` flags for `matrix.backend == 'wgpu'`.

## Info

### IN-01: `MAX_ROW_CHARS` effectively dead
**File:** `src/view/rows.rs:7, 109-119` — rows are ≤128 bytes so the 4096-char cap never triggers; live only after Phase 2's row model.

### IN-02: Per-frame clone of all input events
**File:** `src/view/mod.rs:62` — iterate inside the `ui.input` closure instead.

### IN-03: Minor xtask hygiene
- `label` used directly in filenames (`measure.rs:511`, `size.rs:78`) — `../x` escapes `target/measure`; sanitize.
- `sparse::parse_args` (`sparse.rs:145`) silently lets the last positional win.
- `write_at` (`sparse.rs:72-77`) silently skips markers that don't fit; END marker can overlap head for small `len`.
- `src/main.rs:1` release `windows_subsystem = "windows"` hides `--help`/`--version`/arg errors on Windows.
- `File::open` on a FIFO/device blocks the main thread before the window appears (`src/file.rs:52`).
