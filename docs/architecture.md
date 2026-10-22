# Kitron architecture

This document describes how the Kitron runtime is put together: the processes,
the threads, the wire protocols, and the packaging pipeline.

> Spec references (`§…`) point to `instruction.md`.

---

## 1. Processes and threads

```text
┌─ app process ───────────────────────────────────────────────┐
│                                                              │
│  main thread (AppKit)          Rust core                    │
│  ├─ NSApplication loop        ├─ window registry             │
│  ├─ React Native (Fabric)     ├─ event bus                  │
│  ├─ AppKit window callbacks   ├─ node supervisor            │
│  └─ Kitron menu actions       └─ config + lifecycle          │
│                                                              │
│  JS thread (Hermes)            Node supervisor thread        │
│  ├─ app JS                    ├─ child process reaper        │
│  ├─ __kitron JSI host object   └─ stdout/stderr pump         │
│  └─ TurboModule calls                                        │
│                                                              │
│  work queue "dev.kitron.invoke"                              │
│  └─ blocking nodeInvoke (Rust → Node, waiting on a frame)    │
└──────────────────────────────────────────────────────────────┘
        │ stdio pipes (NDJSON frames)
        ▼
┌─ node child process ────────────────────────────────────────┐
│  host.mjs → loads the app backend (TypeScript, Node 24)      │
│  ├─ invoke handlers                                          │
│  ├─ console forwarding → {type:"log"} frames                 │
│  └─ events → {type:"event"} frames                           │
└──────────────────────────────────────────────────────────────┘
```

Rules enforced by the implementation:

* The **UI thread is never blocked** (§32). Anything that waits (e.g. a
  blocking `nodeInvoke`) runs on a dedicated serial work queue.
* Rust contains **no business logic** (§7/§45) — it only marshals to the OS.
* Node is used for everything it already does well (§6): filesystem, HTTP,
  child processes, streams, crypto.

---

## 2. React Native ⇄ Rust (JSI)

Two complementary surfaces:

### 2.1 `__kitron` — the synchronous JSI host object

Injected by `KitronRuntimeDelegate` through `RCTHost.runtimeDelegate` **before**
the Hermes engine starts, so it is available from the first line of app code.

```ts
__kitron.version        // "0.1.0"
__kitron.platform       // "macos"
__kitron.apiVersion     // 1
__kitron.app.bits()     // {pid,name,identifier,version,platform,dev,rootDir}
__kitron.windows.list() // [{id,title,visible,focused,…}]
__kitron.node.status()  // {running,pid,uptimeMs,handlers}
__kitron.log(level, message)
```

The getters read a **cached snapshot** owned by the Rust core, so they never
touch AppKit from the JS thread — no marshalling, no JSON string parsing
(§31: avoid unnecessary serialisation and copies).

### 2.2 `KitronRuntime` — the TurboModule

For calls that need a return value or run on the native queue. The pod declares
it as a classic `RCT_EXPORT_METHOD` module and conforms to `RCTTurboModule`
through a class extension in `KitronBridge.mm` (the public header stays free of
C++ headers so the app's Swift target can `import Kitron`).

```objc
RCT_EXPORT_METHOD(appQuit:(RCTPromiseResolveBlock)resolve rejecter:…)
RCT_EXPORT_METHOD(windowCreate:(NSString *)argsJson resolve:… rejecter:…)
RCT_EXPORT_METHOD(nodeInvoke:(NSString *)method payload:(NSString *)json …)
```

RN 0.83 is bridgeless: the TurboModule proxy is installed as
`global.nativeModuleProxy`, and classic modules only resolve when TurboModule
interop is enabled **before** the host starts. `KitronRuntimeHost.bootstrap`
therefore calls `RCTEnableTurboModuleInterop(YES)` and
`RCTEnableTurboModuleInteropBridgeProxy(YES)` at step 0, before
`[host start]`.

The TypeScript bridge (`@kitron/core`) resolves the module defensively:

```ts
nativeModuleProxy.KitronRuntime  →  __turboModuleProxy.get(…)  →  NativeModules
```

### 2.3 Events — push, not poll

Runtime events (window moved, menu clicked, app activated, backend pushed) are
delivered **immediately**:

```text
Rust core emits ──▶ the event sink (C callback) ──▶ KitronModule
                                                     └─ resolves the parked
                                                        `nextEvent()` promise
```

The UI parks one promise on `nextEvent()` and re-parks after each event; the
native side resolves it the moment something happens. Events emitted before
the UI subscribes are queued in the core's bus and flushed through the sink
when the module is created — nothing is lost.

(The legacy `eventsDrain()` is still exposed for compatibility, but the SDK
no longer polls it.)

---

## 3. React Native ⇄ backend (typed RPC)

The backend is a **separate process** that speaks the Kitron NDJSON protocol
(spec §28/§30) on its stdio. Two runtimes are supported and are
interchangeable — the UI cannot tell them apart:

| Backend                 | How it is spawned                                | SDK                      |
|-------------------------|--------------------------------------------------|--------------------------|
| TypeScript / JavaScript | `node <host.mjs> <backend/index.ts>`              | `kitron` (`@kitron/node`) |
| Rust                    | the compiled binary (`backend/target/release/…`)  | `kitron-backend` crate   |

```text
Rust → backend   {"type":"invoke","id":1,"method":"math.add","payload":{"a":19,"b":23}}
backend → Rust   {"type":"result","id":1,"result":{"sum":42}}
backend → Rust   {"type":"error","id":1,"code":"USER_NOT_FOUND","message":"…","platform":"macos"}
backend → Rust   {"type":"log","level":"info","message":"backend ready"}
backend → Rust   {"type":"event","name":"backend.event","payload":{…}}
Rust → backend   {"type":"shutdown","reason":"app-quit"}
```

Why not localhost HTTP (§30): no port to pick, no TLS/socket overhead, no
second serialization boundary, and the transport dies with the process.

The supervisor (`kitron-node-supervisor`) spawns *a command* — it knows
nothing about the backend's language, only the protocol. The CLI resolves
which one it is (`kitron.config.ts` → `backend.type`) and compiles a Rust
backend before launching.

Errors are always typed (§33):

```json
{ "code": "NODE_NO_HANDLER", "message": "no handler for does.notExist", "platform": "macos" }
```

---

## 4. Rust crates

| Crate                  | Responsibility                                                      |
|------------------------|---------------------------------------------------------------------|
| `kitron-protocol`      | Config schema, wire frames, typed error codes (no I/O).            |
| `kitron-core`          | Runtime state: window registry, event bus, config loader, logger.  |
| `kitron-node-supervisor` | Spawns/monitors the backend process, pumps frames, correlates replies. |
| `kitron-backend`       | The SDK an app's **Rust backend** is written against.               |
| `kitron-macos`         | AppKit/Foundation bindings: `NSWindow`, `NSMenu`, lifecycle, tray. |

`kitron-macos` compiles to a static library (`libkitron_macos.a`) that the
CocoaPods pod links into the app target. The C ABI is declared in
`runtime/macos/pod/Kitron/kitron.h`; every call returns a JSON **envelope**:

```json
{ "ok": true,  "result": … }
{ "ok": false, "error": { "code": "…", "message": "…", "platform": "macos" } }
```

---

## 5. The macOS pod

```text
runtime/macos/pod/
├── Kitron.podspec        # deps: React-Core, React-RCTAppDelegate, React-Fabric,
│                         #       React-RuntimeCore, React-RuntimeApple, React-jsi,
│                         #       React-NativeModulesApple (TurboModule interop)
├── LICENSE
└── Kitron/
    ├── kitron.h          # Rust C ABI
    ├── KitronBridge.h    # public, Swift-importable surface
    └── KitronBridge.mm   # JSI host object + TurboModule + bootstrap
```

`KitronRuntimeHost.bootstrap(factory:moduleName:)` is the single entry point
used by the app's `AppDelegate.swift`:

1. Enable TurboModule interop (before the host exists).
2. Bootstrap the Rust core (config from `KITRON_CONFIG_JSON` or the packaged
   `app-config.json`).
3. Create the `RCTHost`, install the runtime delegate, start it.
4. Create the main window **through Rust** (Rust owns the `NSWindow`).
5. `startReactNativeWithModuleName:inWindow:` — React Native renders into the
   Rust-owned window.

---

## 6. Lifecycle

```text
process start
   └─ Rust bootstrap (config, logger, node supervisor)
        └─ Node host spawn ──► ready frame
             └─ RN host start ──► Hermes runtime ──► __kitron injected
                  └─ main window created (Rust) ──► RN renders into it
                       └─ app.ready ──► app.on("ready")
                            └─ ⌘Q ─► applicationShouldTerminate ─► Rust veto check
                                 └─ app.beforeQuit ─► quit
```

`applicationShouldTerminate:` consults `KitronRuntimeHost.canTerminate()`
(the Rust veto flag) so `app.preventDefaultExit()` can cancel a pending quit
(§11).

---

## 7. The app bundle (`kitron create`)

`kitron create` does not just copy a template — it vendors the SDK **into the
app**, so the project is self-contained:

```text
my-app/node_modules/
├── @kitron/core|window|node|…   # the TypeScript SDK (compiled JS + sources)
├── kitron/                       # the umbrella package
└── @kitron/sdk/                  # the runtime the app builds against
    ├── cli/dist/                 # the CLI (compiled ESM)
    ├── runtime/rust/             # the Rust workspace (sources, Cargo.lock)
    ├── runtime/macos/pod/        # the Kitron pod sources
    ├── runtime/node/host.mjs     # the embedded Node host bootstrap
    └── templates/app/            # the app template
```

`node_modules/.bin/kitron` points at the vendored CLI, so `pnpm dev` and
`pnpm build` work without a global install (spec §36). The CLI resolves its
`SDK_ROOT` from its own location, so it always uses the vendored runtime.

Cargo artifacts are written to `<app>/.kitron/rust-target` (not
`node_modules`), so they survive a `pnpm install`.

---

## 8. Packaging (`kitron build`)
1. `react-native bundle --platform macos --dev false` → `main.jsbundle`.
2. `xcodebuild -configuration Release` → `MyApp.app`.
3. `embedRuntimeAssets` copies into `MyApp.app/Contents/Resources`:

```text
Resources/
├── main.jsbundle              # production JS
└── kitron/
    ├── app-config.json        # serialized kitron.config.ts
    ├── backend/               # the app's Node backend (TS)
    ├── node-host/             # host.mjs + supervisor runtime
    ├── node/bin/node          # the Node binary used as the packaged runtime
    └── node_modules/          # kitron + @kitron/* (for the backend imports)
```

At launch, `KitronSetUpBundledEnvironment()` points `KITRON_ROOT`,
`KITRON_RUNTIME_NODE_DIR`, `KITRON_NODE_PATH` and `KITRON_CONFIG_JSON` at the
packaged resources — so the shipped app needs **no environment variables** and
no Metro server.

---

## 9. Threading & performance rules (§31/§32)

* JSI getters read cached snapshots — no locks on the AppKit thread from JS.
* `nodeInvoke` (blocking) runs on a serial work queue, never on the UI thread.
* The event bus is drained on the main thread (it refreshes window snapshots
  that touch AppKit).
* Rust marshals to Objective-C directly; values cross the boundary as
  `NSString`/`NSNumber`/`NSArray`/`NSDictionary`, never as re-parsed JSON
  strings on the hot path.

---

## 10. Security model (§23)

* Permissions are declared in `kitron.config.ts` (`permissions: []` by default);
  sensitive APIs must be opted into explicitly.
* The gate lives in the **native** layer (`Core::require_permission`), so it
  cannot be bypassed from JavaScript — a call without the permission fails with
  `{ code: "PERMISSION_DENIED", … }`.
* The native surface is never exposed raw: the developer sees typed, scoped
  APIs (`window.create`, `dialog.openFile`, …), not `native.executeAnything`.
* The Node child process runs with the app's own privileges — it is the app's
  backend, not a sandbox.

---

## 11. Native API surface (phase 2)

The `KitronRuntime` TurboModule exposes the native APIs; every call returns the
same `{ ok, result | error }` envelope, and the TS layer rehydrates the error
into a `KitronError` (spec §33).

| JS API                  | C ABI                          | AppKit backing                    | Permission |
|-------------------------|--------------------------------|-----------------------------------|------------|
| `clipboard.read/write`  | `kitron_clipboard_read/write`  | `NSPasteboard`                    | `clipboard` |
| `dialog.message`        | `kitron_dialog_message`        | `NSAlert`                         | `dialog`   |
| `dialog.openFile`       | `kitron_dialog_open_file`      | `NSOpenPanel`                     | `dialog`   |
| `dialog.saveFile`       | `kitron_dialog_save_file`      | `NSSavePanel`                     | `dialog`   |
| `shell.openUrl`         | `kitron_shell_open_url`        | `NSWorkspace`                     | `shell`    |
| `shell.openPath`        | `kitron_shell_open_path`       | `NSWorkspace`                     | `shell`    |
| `shell.showItemInFolder`| `kitron_shell_show_item_in_folder` | `NSWorkspace`                 | `shell`    |
| `menu.setApplicationMenu` | `kitron_menu_set_application_menu` | `NSMenu`                | —          |

AppKit is `MainThreadOnly`, so these calls run on the module's main queue. The
panels are modal — the promise settles once the user dismisses them.

App lifecycle events (`app.activate`, `app.quit`) are pushed by the platform
shell: the Swift `AppDelegate` calls `KitronEmitAppEvent("activate")`, which
goes through `kitron_emit_app_event` into the core event bus and out to JS
through the regular event pump.

### Menus (§18)

The menu is declared in TypeScript and installed natively:

```ts
menu.setApplicationMenu([
  { label: "File", items: [{ label: "New", action: "new-file", accelerator: "CmdOrCtrl+N" }] },
]);
menu.onAction((action) => { /* "new-file" */ });
```

On the native side each item with an `action` gets

* the selector `kitronMenuItemClicked:`,
* a shared `MenuTarget` object as its target,
* a `tag` that indexes a registry of action ids.

When AppKit dispatches the click, the target reads the tag, looks the action id
up and pushes a `menu.action` event into the core event bus — the same bus the
UI reads through the regular event pump.
