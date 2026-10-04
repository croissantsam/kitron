# Kitron

> Build desktop applications with TypeScript, powered by Node.js and native Rust infrastructure — without Chromium, without React Native.

Mainline React renders through Kitron's own reconciler host, with a `react-native`-compatible API. TypeScript is the app language. Node.js provides filesystem/network/crypto. Rust owns windows, menus, tray, dialogs, shortcuts, updater, and OS integration. No Electron, no Chromium, no web renderer.

## Layout

```text
kitron/
├── packages/        TS APIs (@kitron/* + kitron facade)
│   ├── core/        errors, logger, bridge, app lifecycle
│   ├── window/      multi-window (§10)
│   ├── dialog/ clipboard/ notification/ shell/ process/
│   ├── tray/ menu/ shortcut/ updater/ node/
│   └── kitron/      public facade (§9)
├── runtime/rust/    native core (winit → AppKit / Win32)
│   ├── kitron-core/     wire types (mirrors packages/core)
│   ├── kitron-runtime/  windows, IPC dispatch, node sidecar
│   └── kitron/          `kitron-runtime` host binary
├── cli/             kitron create/dev/build/package/run/doctor (§35)
├── templates/basic/ `kitron create my-app` output (§36)
├── examples/hello-kitron/  MVP vertical slice (§41)
├── tests/           cross-package contract tests
└── docs/            architecture, API, MVP
```

## MVP vertical slice (§41)

```ts
import { window } from "kitron";
const mainWindow = await window.create({ width: 1200, height: 800 });
```

This opens a real OS window (`winit` → `NSWindow` on macOS, Win32 on Windows), with IPC between JS and Rust and an auto-started Node sidecar.

## Quickstart

```bash
npm install
npm run build --workspaces --if-present
cargo build --manifest-path runtime/rust/Cargo.toml
node cli/dist/cli.js doctor
node cli/dist/cli.js create my-app
```

## Rules (§45)

- Never: Chromium, Electron, Vite-in-runtime, TanStack-in-runtime, business logic in Rust, blocking the UI thread, raw OS APIs to app devs.
- Always: React + own renderer for UI, TypeScript-first API, reuse Node.js, Rust for native, typed async IPC, minimal runtime.
