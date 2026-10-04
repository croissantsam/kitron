# Benchmark: Kitron vs Electron

Measured on this machine with `node bench/run.mjs --runs=5`
(see `results.json` for raw output). Re-run it yourself — same scripts,
same hello-world apps.

## Results (median of 5)

| Metric | Kitron | Electron 35 | Ratio |
|---|---|---|---|
| Shipped runtime bytes | **13.2 MB** (`kitron-runtime` release binary) | **246 MB** (`Electron.app` framework every app re-bundles) | ~19× smaller |
| Cold start → interactive hello | **47 ms** (spawn → `window.create` + render + layout) | **678 ms** (spawn → `did-finish-load`) | ~14× faster |
| Idle memory (whole tree) | **13.3 MiB** RSS | **346 MiB** RSS (main + renderer + GPU + utility) | ~26× leaner |

Machine: macOS 27 arm64, Apple M4, 16 GB RAM. Kitron first run was
920 ms cold (page cache), then 43–50 ms warm; Electron 650–800 ms.
Idle sampled 3× over ~3.5 s after a 1.5 s settle, whole process tree.

## What was compared

Two equivalent hello-world apps: an 800×600 window with one text line.
- Kitron: headless `kitron-runtime` + `window.create` + `ui` tree + `layout`.
- Electron: `BrowserWindow` 800×600 + local `index.html` (`<h1>Hello</h1>`).

## What this does NOT claim

- **Not feature parity.** Electron ships a full Chromium: run any web stack
  unchanged. Kitron renders its own 4-component UI kit; porting cost is real
  (see `docs/RN-COMPAT.md`).
- **Not render performance.** No FPS/scroll benchmarks here — that needs
  display automation and a shared workload; contributions welcome.
- **Hello-world only.** Real apps grow both numbers. The gap direction is
  structural (no bundled browser engine), the exact ratios will vary.
- **macOS only (this run).** Windows numbers pending CI runners.
- Electron was run with `--no-sandbox` (sandbox breaks in this shell
  session); sandboxing costs it a little memory in production, not less.

## Reproduce

```bash
cargo build --release --bin kitron-runtime   # runtime/rust
node bench/run.mjs --runs=5                  # writes results.json
```
