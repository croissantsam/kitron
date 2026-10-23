# Benchmark: Kitron vs Electron

Measured on this machine with `node bench/run.mjs --runs=5`
(see `results.json` for raw output). Re-run it yourself — same scripts,
same apps (an 800×600 window rendering 2000 rows).

## Results (median of 5, October 2026 run)

| Metric | Kitron | Electron 35 | Ratio |
|---|---|---|---|
| Shipped app bytes | **115 MB** (whole packaged `.app`) | **245 MB** (`Electron.app` framework every app re-bundles) | ~2.1× smaller |
| Cold start → interactive (2000 rows) | **896 ms** | **821 ms** | ~1.1× slower |
| Scroll 2000 rows to bottom | **31 ms** | **20 ms** | ~1.5× slower |
| Idle memory (whole tree) | **124.6 MiB** RSS | **424.5 MiB** RSS (main + renderer + GPU + utility) | ~3.4× leaner |

Machine: macOS 27 arm64, Apple M4, 16 GB RAM.

## What was compared

Two apps rendering the same workload: an 800×600 window with a scrollable
list of 2000 rows, scrolled to the bottom.
- Kitron: the real packaged Release `.app` (`kitron create` + `kitron build`),
  launched standalone with a clean environment. Embedded React Native (Fabric)
  `FlatList`, embedded Node backend, Rust native shell. Build strips debug
  symbols and prunes vendored SDK sources.
- Electron: `BrowserWindow` 800×600 with a 2000-row scrollable div, run via
  `electron .` with `--no-sandbox`.

## What this does NOT claim

- **Not feature parity.** Electron ships a full Chromium: run any web stack
  unchanged. Kitron renders via React Native's native renderer; porting cost is
  real.
- **Render perf is workload-dependent.** Chromium's compositor is highly
  optimised for DOM scrolling; React Native's `FlatList` wins on memory but
  trails on raw scroll throughput for huge lists. Real apps virtualise and the
  gap narrows.
- **Hello-world vs heavy.** The earlier hello-world run measured ~165 ms /
  ~111 MiB for Kitron; the 2000-row workload is heavier and more realistic.
- **macOS only (this run).** Windows numbers pending CI runners.
- Electron was run with `--no-sandbox`; sandboxing costs it a little memory.

## Reproduce

```bash
node bench/run.mjs --runs=5   # creates (if needed) + builds the .app, writes results.json
```
