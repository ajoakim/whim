# Whim Footprint Measurements

Standing table of footprint measurements. Later phases append rows; do not rewrite history.
MB = MiB (1,048,576 bytes) everywhere in this project.

## Budgets

| Metric | Project target | Phase 1 gate | CI threshold |
|--------|----------------|--------------|--------------|
| Stripped size | < ~15 MB | <= 10 MB | 10 MB hard |
| Startup | ~200 ms cold | <= 300 ms | 2000 ms soft |
| Idle RSS | tens of MB | <= 70 MB (settled working set) | 200 MB soft |
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

## Idle RAM isolation (2026-10-31)

Method: PowerShell `Get-Process` polled every 1 s for 40 s per build (WorkingSet64 / PrivateMemorySize64; PagedMemorySize64 was identical to private in every sample). Machine as above (Intel Arc B580, driver 32.0.101.6790). Bare app = `target/diag/bare-eframe/` (own workspace, eframe 0.37.0 glow + default_fonts, same release profile, explicit `Renderer::Glow`, one label, no custom painting). Mesa = copy of whim.exe in `target/diag/mesa/` with pal1000 mesa 26.2.5 `opengl32.dll` + `libgallium_wgl.dll` (archive SHA-256 verified).

| build | WS peak | WS settled | private settled | settle time |
|-------|---------|------------|-----------------|-------------|
| whim glow, no file | 89.0 MB | 57.5 MB | 59.0 MB (peak 90.9) | WS drops at +7 s, flat after |
| whim glow, 20 GiB sparse file | 91.2 MB | 58.6 MB | 99.4 MB (peak 133.0) | WS drops at +7 s to 60.0, ~58.5 by +16 s |
| bare eframe glow (label only) | 97.5 MB | 64.0 MB | 57.4 MB (peak 91.3) | drops at +8 s to 66.0, 64.0 by +23 s |
| whim glow + Mesa llvmpipe | 115.8 MB | 115.8 MB | 104.5 MB | no drop in 40 s |

Findings:
- The drop is a real release, not a working-set trim: private bytes fall in lockstep with the working set (whim no file 90.9 -> 59.0; bare 91.3 -> 59.4). A pure trim would leave private flat. The ~30 MB is startup-transient allocations freed ~7-8 s after launch (likely driver/GL init or font/atlas upload staging), then settled.
- Settle time is about +7-8 s from launch in all three real-GPU builds. The harness samples at +2 s and +12 s; +2 s is inside the transient and is what produces 92.8 MB. The +12 s sample is already settled.
- The 20 GiB file adds no working set. It does add about 40 MB of private/commit (99 vs 59 MB) while WS is unchanged; that is accounting for the mapping (file-backed section charged to commit), not resident RAM. Worth noting if commit were ever used as the metric.
- Not skipped: no non-eframe GL baseline (winit + glutin) was built; the bare-eframe result already separates toolkit from whim.

Attribution (settled WS):
- egui/eframe + GL driver base: about 64 MB (bare eframe), same transient peak (~97 MB) and same +8 s settle. This is essentially the whole cost.
- whim's own additions: about -6 MB vs the bare app (57.5 vs 64.0), i.e. nothing measurable; the difference is within run noise and the bundled mono font vs default layout. Whim adds no meaningful idle RAM.
- GL driver vs egui split: not cleanly separable. Software GL (Mesa llvmpipe) is much larger (116 MB, no release), so it cannot be used to subtract the Intel ICD; it shows only that the driver choice swings the number by tens of MB. The Intel ICD plus egui together account for the ~58-64 MB floor; the toolkit choice cannot reduce it much. Slint was not tested.

Recommendation:
- Gate metric: define idle RSS as the settled working set, i.e. the minimum of samples taken in the window after startup has settled (>= +15 s), not the +2 s sample or the max. Measured against that, whim is 57.5-58.6 MB (passes 60 MB with ~2 MB margin on this machine), and the bare toolkit itself is 64 MB, so the 60 MB line is at the edge of what egui/glow can achieve here; consider whether 60 MB should be relaxed to about 70 MB for headroom across drivers (user decision, thresholds untouched).
- Harness change (not made; `xtask/src/measure.rs` untouched): currently it sleeps 2 s, samples (a), sleeps `idle_secs` (default 10 s), samples (b), and reports `max(a, b)` as idle RSS at about +2 s and +12 s, which includes the transient. Change to: sleep ~20 s, then sample at +20 s and +30 s (or a few samples over 10 s) and report the min of that settled window. Keep the CPU delta over the same window.
- Verdict remains PENDING; the user decides.
