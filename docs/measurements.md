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

## Size deltas

| Variant | Size (MB) | Delta vs glow default |
|---------|-----------|-----------------------|
