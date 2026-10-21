# Kitron

**Desktop runtime for React Native + TypeScript + Node.js apps — powered by native Rust, without Chromium.**

Kitron is an Electron alternative that never embeds a browser engine. The UI is
rendered by React Native's native renderer (Fabric) inside a real native window,
the backend runs on an embedded Node.js runtime, and the native layer (windows,
menus, lifecycle, IPC supervision) is written in Rust.

```text
┌──────────────────────────────────────────────┐
│                  KITRON APP                  │
│      React Native  ·  TypeScript  ·  Node    │
└──────────────────────┬───────────────────────┘
                       │  JSI / typed RPC
                       ▼
                ┌──────────────┐
                │   Rust Core  │  ← native layer only
                └──────┬───────┘
                       ▼
                   macOS / Windows
```

---

## Status — MVP vertical slice (spec §41)

| # | Requirement                                          | Status |
|---|------------------------------------------------------|--------|
| 1 | React Native works                                   | ✅ RN 0.83 (macOS), Fabric renderer, bridgeless |
| 2 | A native desktop window works                        | ✅ `NSWindow` owned by the Rust core |
| 3 | TypeScript works                                     | ✅ TypeScript-first SDK, Node 24 type stripping |
| 4 | JSI works                                            | ✅ `__kitron` host object injected before engine start |
| 5 | Rust works                                           | ✅ `kitron-core`, `kitron-macos`, `kitron-node-supervisor` |
| 6 | Node.js can be started                               | ✅ embedded Node host, supervised by Rust |
| 7 | React Native ⇄ Rust                                  | ✅ JSI snapshots + TurboModule `KitronRuntime` |
| 8 | React Native ⇄ Node                                  | ✅ typed RPC (`backend.invoke`), no localhost HTTP |
| 9 | macOS works                                          | ✅ AppKit / Foundation |
|10 | An app can be packaged                               | ✅ `kitron build` → `.app` with embedded JS bundle + backend + Node |

### Phase 2 (§42) — first batch landed

| API                | Native backing                                | Status |
|--------------------|-----------------------------------------------|--------|
| `clipboard`        | `NSPasteboard`                                | ✅ |
| `dialog`           | `NSAlert` / `NSOpenPanel` / `NSSavePanel`     | ✅ |
| `shell`            | `NSWorkspace`                                 | ✅ |
| `app.activate` / `app.quit` | `NSApplication` lifecycle               | ✅ |
| `menu` / `tray` / `notification` / `process` / `shortcut` / `updater` | — | 🚧 typed `KITRON_PHASE2` stubs |

### Permissions (§23)

Native APIs are never implicitly granted. An app declares what it needs in
`kitron.config.ts` and the **native layer** enforces it:

```ts
export default {
  app: { name: "My App", identifier: "com.example.myapp", version: "1.0.0" },
  window: { width: 1200, height: 800 },
  node: { entry: "./backend/index.ts" },
  permissions: ["clipboard", "dialog", "shell"],
};
```

A call without the matching permission fails with a typed error
(`{ code: "PERMISSION_DENIED", … }`) — it cannot be bypassed from JavaScript.

---

## Quick start

```bash
# 1. scaffold an app
node cli/src/kitron.ts create my-app
cd my-app

# 2. run it (Metro + Debug build + launch)
node <kitron>/cli/src/kitron.ts dev

# 3. build a distributable .app
node <kitron>/cli/src/kitron.ts build
```

`kitron create` generates a working app: React Native UI, a TypeScript backend,
the Xcode project, the Rust runtime and the Kitron pod.

### The generated app

```tsx
import { app, backend, window } from "kitron";

export default function App() {
  useEffect(() => {
    backend.invoke<{ sum: number }>("math.add", { a: 19, b: 23 }).then((r) => {
      console.log(r.sum); // 42
    });
  }, []);
  return <Text>Hello Kitron</Text>;
}
```

```ts
// backend/index.ts — runs on the embedded Node runtime
export const handlers = {
  "math.add": ({ a, b }: { a: number; b: number }) => ({ sum: a + b }),
};
```

---

## CLI (spec §35)

| Command            | What it does                                                                 |
|--------------------|------------------------------------------------------------------------------|
| `kitron create`    | Scaffolds an app (RN UI + Node backend + Rust runtime + Xcode project).       |
| `kitron dev`       | Builds the Rust core, starts Metro, builds the Debug app and launches it.    |
| `kitron build`     | Production JS bundle + Release `.app` with the backend and Node embedded.    |
| `kitron package`   | Wraps the Release `.app` into a macOS `.dmg`.                                 |
| `kitron run`       | Launches the most recent Release build.                                       |
| `kitron doctor`    | Checks the toolchain (Node, Rust, Xcode, CocoaPods, Metro).                   |

---

## Architecture

```text
kitron/
├── packages/            # @kitron/* TypeScript SDK (spec §24)
│   ├── core/            # app lifecycle, events, typed errors, native bridge
│   ├── window/          # window API
│   ├── node/            # backend RPC client (RN ⇄ Node)
│   ├── dialog/ clipboard/ menu/ tray/ notification/
│   ├── process/ shell/ shortcut/ updater/   # phase-2 namespaces (typed stubs)
│   └── kitron/          # umbrella package — `import { app, window, backend } from "kitron"`
├── runtime/
│   ├── rust/            # Rust core (native layer only — no business logic)
│   │   ├── crates/kitron-protocol/      # config + wire protocol + typed errors
│   │   ├── crates/kitron-core/          # runtime state, event bus, node supervisor
│   │   ├── crates/kitron-node-supervisor/# spawns/monitors the Node host
│   │   └── crates/kitron-macos/         # AppKit bindings (windows, menu, lifecycle)
│   ├── macos/pod/       # CocoaPods pod: JSI host object + RCT TurboModule
│   └── node/            # embedded Node host bootstrap (NDJSON frames)
├── cli/                 # @kitron/cli
├── templates/app/       # `kitron create` template
├── examples/            # example apps
├── tests/               # test suite
└── docs/                # architecture notes
```

### The three runtimes

| Runtime    | Runs                                    | Talks to                       |
|------------|-----------------------------------------|--------------------------------|
| React Native | Hermes in the app process            | Rust via JSI (`__kitron`)       |
| Node.js    | A supervised child process             | Rust via NDJSON frames         |
| Rust       | The app process (core) + supervisors   | macOS via AppKit/Foundation    |

### Communication paths

```text
RN (TS)  ──JSI __kitron snapshots──▶  Rust core  ──AppKit──▶  NSWindow
RN (TS)  ──TurboModule KitronRuntime──▶  Rust core  ──NDJSON──▶  Node backend
Node     ──events───────────────▶  Rust core  ──event bus──▶  RN (TS)
```

* **RN ⇄ Rust** uses JSI: synchronous, allocation-light snapshot getters
  (`__kitron.app.bits()`, `__kitron.windows.list()`, `__kitron.node.status()`),
  plus the `KitronRuntime` TurboModule for calls that need a return value.
* **RN ⇄ Node** uses a typed RPC framed over NDJSON on a pipe — no localhost
  HTTP server, no JSON-over-HTTP round trips.
* **Errors** are always typed: `{ code, message, platform }` (§33).

---

## Repository layout

See [`docs/architecture.md`](docs/architecture.md) for the deep dive (thread
model, wire protocol, lifecycle, packaging).

---

## Development

```bash
pnpm install          # workspace deps
pnpm typecheck        # TypeScript (SDK + CLI)
pnpm test             # typecheck + cargo test + node-host smoke + CLI e2e
pnpm test:rust        # cargo test only
pnpm build:rust       # cargo build --release -p kitron-macos
```

The macOS end-to-end gate (pod install + xcodebuild + launch) is opt-in:

```bash
KITRON_E2E_MACOS=1 bash tests/run.sh
```

---

## License

MIT — see [`LICENSE`](LICENSE).
