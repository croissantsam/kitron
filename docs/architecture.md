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

---

## 3. React Native ⇄ Node (typed RPC)

The embedded Node runtime is a child process started by the Rust supervisor
(`kitron-node-supervisor`). The bootstrap lives in `runtime/node/host.mjs` and
speaks **NDJSON frames** over stdio pipes — one JSON object per line.

```text
Rust → Node   {"type":"invoke","id":1,"method":"math.add","payload":{"a":19,"b":23}}
Node → Rust   {"type":"result","id":1,"result":{"sum":42}}
Node → Rust   {"type":"error","id":1,"code":"USER_NOT_FOUND","message":"…","platform":"macos"}
Node → Rust   {"type":"log","level":"info","message":"backend ready"}
Node → Rust   {"type":"event","name":"backend.event","payload":{…}}
Rust → Node   {"type":"shutdown","reason":"app-quit"}
```

Why not localhost HTTP (§30): no port to pick, no TLS/socket overhead, no
second serialization boundary, and the transport dies with the process.

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
| `kitron-node-supervisor` | Spawns/monitors the Node host, pumps frames, correlates replies. |
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

## 7. Packaging (`kitron build`)

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

## 8. Threading & performance rules (§31/§32)

* JSI getters read cached snapshots — no locks on the AppKit thread from JS.
* `nodeInvoke` (blocking) runs on a serial work queue, never on the UI thread.
* The event bus is drained on the main thread (it refreshes window snapshots
  that touch AppKit).
* Rust marshals to Objective-C directly; values cross the boundary as
  `NSString`/`NSNumber`/`NSArray`/`NSDictionary`, never as re-parsed JSON
  strings on the hot path.

---

## 9. Security model (§23)

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

## 10. Native API surface (phase 2)

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

AppKit is `MainThreadOnly`, so these calls run on the module's main queue. The
panels are modal — the promise settles once the user dismisses them.

App lifecycle events (`app.activate`, `app.quit`) are pushed by the platform
shell: the Swift `AppDelegate` calls `KitronEmitAppEvent("activate")`, which
goes through `kitron_emit_app_event` into the core event bus and out to JS
through the regular event pump.
