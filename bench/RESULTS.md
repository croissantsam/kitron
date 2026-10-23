# Benchmark: Kitron vs Electron

Measured on this machine with `node bench/run.mjs --runs=5`
(see `results.json` for raw output). Re-run it yourself — same scripts,
same hello-world apps.

## Results (median of 5, October 2026 run)

| Metric | Kitron | Electron 35 | Ratio |
|---|---|---|---|
| Shipped app bytes | **115 MB** (whole packaged `.app`) | **245 MB** (`Electron.app` framework every app re-bundles) | ~2.1× smaller |
| Cold start → interactive hello | **165 ms** (spawn → `app.ready received`) | **260 ms** (spawn → `did-finish-load`) | ~1.6× faster |
| Idle memory (whole tree) | **111.1 MiB** RSS | **400.0 MiB** RSS (main + renderer + GPU + utility) | ~3.6× leaner |

Machine: macOS 27 arm64, Apple M4, 16 GB RAM. Kitron first run 246 ms cold
(page cache), then 158–165 ms warm; Electron 260–273 ms.
Idle sampled 3× over ~3.5 s after a 1.5 s settle, whole process tree.

## What was compared

Two equivalent hello-world apps: an 800×600 window with one text line.
- Kitron: the real packaged Release `.app` (`kitron create` + `kitron build`),
  launched standalone with a clean environment (no Metro, no `KITRON_*` vars).
  Embedded React Native (Fabric) UI, embedded Node backend, Rust native shell.
  Build strips debug symbols and prunes vendored SDK sources from the bundle.
- Electron: `BrowserWindow` 800×600 + local `index.html` (`<h1>Hello Electron</h1>`),
  run via `electron .` with `--no-sandbox`.

## What this does NOT claim

- **Not feature parity.** Electron ships a full Chromium: run any web stack
  unchanged. Kitron renders via React Native's native renderer; porting cost is
  real.
- **Not render performance.** No FPS/scroll benchmarks here — that needs
  display automation and a shared workload; contributions welcome.
- **Hello-world only.** Real apps grow both numbers. The gap direction is
  structural, the exact ratios will vary.
- **macOS only (this run).** Windows numbers pending CI runners.
- Electron was run with `--no-sandbox` (sandbox breaks in this shell
  session); sandboxing costs it a little memory in production, not less.

## Reproduce

```bash
node bench/run.mjs --runs=5   # creates (if needed) + builds the .app, writes results.json
```
