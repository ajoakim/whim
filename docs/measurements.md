# Whim Footprint Measurements

Standing table of footprint measurements. Later phases append rows; do not rewrite history.
MB = MiB (1,048,576 bytes) everywhere in this project.

## Budgets

| Metric | Project target | Phase 1 gate | CI threshold |
|--------|----------------|--------------|--------------|
| Stripped size | < ~15 MB | <= 10 MB | 10 MB hard |
| Startup | ~200 ms cold | <= 300 ms | 2000 ms soft |
| Idle RSS | tens of MB | <= 60 MB | 200 MB soft |
| Idle CPU | ~0 | < 1 % | 2 % soft |

## Method

- Harness: `cargo xtask measure` (median of N runs, harness-side spawn-to-first-frame time; run 0 is reported separately as cold-ish; idle RSS and CPU are sampled for 10 s after the first frame plus a 2 s settle; RSS is the working set on Windows).
- Size: `cargo xtask check-size` on the stripped release binary.
- The 20 GiB test file is a sparse file created by `cargo xtask sparse`; on-disk size stays under 100 MB.
- CI is warm-only and uses software GL on Windows (Mesa llvmpipe) and Linux (Xvfb + llvmpipe), so CI runtime numbers are not representative of user hardware. Size is a hard CI limit; runtime thresholds in CI are generous.

## Results

| Date | Phase | OS | Machine / runner | Backend | Features | Stripped size (MB) | Startup median (ms) | Startup max (ms) | In-app startup (ms) | Cold/Warm | Idle RSS (MB) | Idle CPU (%) | File | Source | Notes |
|------|-------|----|------------------|---------|----------|--------------------|---------------------|------------------|---------------------|-----------|---------------|--------------|------|--------|-------|
| 2026-10-31 | 1 | Windows 11 | Ryzen 9 5950X / Intel Arc B580 | glow | default (bundled-mono) | 5.0 | 265.7 | 435.5 | 254.0 | Warm | 92.8 | 0.2 | 20 GiB sparse | windows-local-glow | FAILS 60 MB RSS gate by harness; see Idle RSS diagnostic. Run 0 (435.5 ms) is the cold-ish run |
| 2026-10-31 | 1 | Windows 11 | Ryzen 9 5950X / Intel Arc B580 | wgpu | wgpu,bundled-mono | 9.2 | 341.1 | 567.7 | 323.5 | Warm | 153.5 | 0.0 | 20 GiB sparse | windows-local-wgpu | Fails startup and RSS gates; no budgets passed to harness |

## Size deltas

| Variant | Size (MB) | Delta vs glow default |
|---------|-----------|-----------------------|
| glow default (bundled-mono) | 5.01 | 0 (baseline) |
| glow, no bundled font (`--no-default-features --features glow`) | 4.75 | -0.26 (bundled font costs 0.26 MB) |
| glow + accesskit | 5.32 | +0.31 (accessibility delta) |
| wgpu,bundled-mono | 9.24 | +4.23 |

Sources: `size-windows-local-glow`, `-glow-nofont`, `-glow-accesskit`, `-wgpu` in `target/measure/`.

## Idle RSS diagnostic (Windows, 2026-10-31, diagnostic only, no thresholds changed)

The harness reports idle RSS 92.8 MB (glow), above the 60 MB gate. Findings:

| Measurement | glow | wgpu |
|-------------|------|------|
| Harness idle RSS (sysinfo working set, max of 2 samples at +2 s and +12 s) | 92.8 MB | 153.5 MB |
| Get-Process at +8 s, no file arg: WorkingSet64 / PrivateMemorySize64 | 63.7 / 57.9 MB | 108.1 / 275.0 MB |
| Get-Process at +8 s, 20 GiB sparse file: WorkingSet64 / PrivateMemorySize64 | 58.5 / 60.2 MB | 89.2 / 271.8 MB |
| Peak working set during startup | 101-106 MB | 150-169 MB |
| glow with `--ppp 1.0`, 20 GiB file: WS / Private | 57.7 / 59.3 MB | n/a |

- The mmap does not contribute: with vs without the 20 GiB file the working set is the same within noise (63.7 vs 58.5 MB).
- Under the harness the process sat at WS 92 MB / commit 132 MB for the sampled window, then dropped to WS 61 MB / commit 101 MB (observed with a concurrent Get-Process poll, same pid). The extra ~31 MB is transient startup memory that Windows had not yet trimmed from the working set; the harness samples inside that window. Steady-state glow working set is about 58-64 MB.
- GPU: Intel Arc B580 Graphics, driver 32.0.101.6790 (CPU Ryzen 9 5950X). Intel's OpenGL ICD (loaded into the process) plausibly accounts for a large share of the ~60 MB, but this was not isolated; no minimal eframe window was built to separate toolkit from driver cost.
- wgpu is far heavier on every axis (private bytes 272 MB), which confirms glow as the default.
