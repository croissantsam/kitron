# Shell contracts — what a platform shell must implement

The Kitron runtime is a portable Rust core (`runtime/rust/crates/kitron-core`)
plus one shell per platform (`kitron-macos` today, `kitron-windows` next). The
core owns the state, the event bus, the protocol and the wire models; a shell
owns the platform surface (windows, menus, dialogs, clipboard, app lifecycle).

This file is the checklist a new shell has to satisfy. Everything referenced
here lives in two places only:

- **wire models** — `kitron_core::shell` (`runtime/rust/crates/kitron-core/src/shell.rs`)
- **typed errors + config** — `kitron-protocol`

If a JSON field or an op name is not in `kitron_core::shell`, a shell must not
invent one: add it to the core first, otherwise the TypeScript SDK and the two
shells drift apart silently.

## 1. The C ABI the bridge calls

The pod's bridge (`KitronBridge.mm` on macOS) binds this exact list; a second
shell exports the same symbols from its lib, and app packaging links *one* of
the shells in (`Kitron.podspec` → `vendored_libraries`; Windows: a link arg in
the VS project). Signatures mirror `runtime/macos/pod/Kitron/kitron.h`.

Every function follows the same envelope (spec §33):

```
char *fn(...)           → "{ \"ok\": true, \"result\": … }"
                          | "{ \"ok\": false, \"error\": { code, message, platform } }"
void kitron_free_string(char *)   → the caller frees what the callee returned
```

| Category | Symbols | Notes |
|---|---|---|
| bootstrap | `kitron_bootstrap` | reads `KITRON_*` env, builds `Core`, initializes host services; called once before the JS starts |
| app | `kitron_app_bits_json`, `kitron_app_visibility`, `kitron_app_request_quit`, `kitron_app_can_terminate`, `kitron_app_cancel_termination` | `app.bits` is read synchronously over JSI too |
| windows | `kitron_window_create`, `kitron_window_get`, `kitron_window_list`, `kitron_window_op` | answers use `WindowSnapshot`; ops are the `WindowOp` wire names |
| snapshot | `kitron_snapshot_json` | full app state for the JSI host object (`windows.list`, `node.status`) |
| events | `kitron_events_drain`, `kitron_set_event_sink` | §3 below |
| backend | `kitron_node`, `kitron_node_status_json`, `kitron_node_invoke_wait` | supervisor protocol — portable, part of `kitron-node-supervisor`, do not reimplement |
| permissions | — | each sensitive call gates itself (`Core::require_permission`) before touching the OS |
| clipboard | `kitron_clipboard_read`, `kitron_clipboard_write` | permission `clipboard` |
| shell | `kitron_shell_open_url`, `kitron_shell_open_path`, `kitron_shell_show_item_in_folder` | permission `shell` |
| dialogs | `kitron_dialog_message`, `kitron_dialog_open_file`, `kitron_dialog_save_file` | permission `dialog`; `windowId` → sheet/child-modal presentation |
| menus | `kitron_menu_set_application_menu` | declarative JSON (`MenuDefinition`); clicks emit `menu.action` |
| lifecycle | `kitron_emit_app_event` | the platform AppDelegate calls in ("activate", …) |

A Rust shell function needs `#[unsafe(no_mangle)]` — a missing one links under
a Rust-mangled symbol and the app fails to link. `tests/bridge-conformance.mjs`
enforces the full triangle (native module ↔ TypeScript API ↔ C ABI) on every
run; keep it green.

## 2. Wire models (the JSON every platform answers identically)

From `kitron_core::shell`, serialized camelCase:

- **WindowSnapshot / WindowFrame** — `window.list()` and the JSI surface read
  this directly: `id, title, frame{x,y,width,height}, focused, minimized,
  maximized, visible`. Frame origin is **top-left** in screen coordinates on
  every platform (AppKit is bottom-left — flip it inside the shell).
- **WindowCreateArgs** — `title,width,height,focus,resizable,minimizable,
  closable`. A shell answers `{ id, width, height, windowPtr? }` (pointer
  fields stay shell-private).
- **WindowOp** — exactly `close, minimize, maximize, restore, focus, show,
  hide, setSize, setPosition, setTitle, setPreventClose`; anything else is
  `WINDOW_OP_UNKNOWN`. The enum lives in the core so a second shell cannot
  invent names.
- **MenuDefinition / MenuItemDefinition** — `{ label, items[] }` where items
  are `{ type?, label?, action?, accelerator?, items?, enabled?, checked? }`.
  The shell owns no menu state model: it parses the same struct and dispatches
  clicks as `menu.action` events carrying the `action` id.
- **Dialog args / results** — `message{title,message,detail,buttons,style,
  windowId}`, `openFile{title,multiple,directory,extensions,windowId}`,
  `saveFile{title,defaultName,directory,windowId}`; results are built by
  `shell::dialog_results` (`message → {button,index,confirmed}`,
  `openFile → {canceled,paths[]}`, `saveFile → {canceled,path|null}`).

`runtime/rust/crates/kitron-core/src/shell.rs` ships round-trip tests for all
of these — they are the normative examples.

## 3. Events — push, with a queue fallback

`Core::emit(name, payload)` hands the event to the installed sink when one is
set, otherwise it queues into the bus (`Core::events()`). A shell installs the
*push* path at startup so events reach the UI in the same tick:

1. `kitron_set_event_sink(context, fn)` — the shell registers a C callback
   (called on the emitting thread; the context is an opaque `usize`).
2. Deliver into the JS runtime — on macOS the bridge parks a promise
   (`nextEvent`) and the sink resolves it; on Windows, use the RNW event
   emitter (`ReactContext` emits `coreEvents` over the dispatcher) — the SDK
   only assumes *a* promise or an event named `nextEvent` in the native module.
3. Flush first: events emitted before the sink existed are still in the bus;
   drain and push them through the sink at install time (kitron-macos does
   this inside `kitron_set_event_sink`).

Event names are `kitron_core::EventName` — `app.ready, app.activate,
app.beforeQuit, app.quit, window.created, window.closed, window.focus,
window.blur, window.resize, window.move, window.minimize, window.restore,
node.started, node.ready, node.log, node.stopping, node.exited, backend.event,
menu.action`.

## 4. Lifecycle & threading rules

- Sensitive ops run on the **main thread**; off-thread callers get
  `THREAD_MAIN_REQUIRED` (macOS does this with `MainThreadMarker` checks).
- Sheets/constituent modals pump the platform's modal run loop
  (`NSModalPanelRunLoopMode` / a modal message loop) so a synchronous call can
  wait for user input without freezing event dispatch.
- Permissions gate **inside the shell**, before touching the OS:
  `clipboard/dialog/shell/menu`… — denied calls answer `PERMISSION_DENIED`
  from `Core::require_permission` (spec §23).
- The app delegate/AppWindow forwards platform lifecycle hooks (« active »,
  quit requests) via `kitron_emit_app_event`; `app.beforeQuit` +
  `kitron_app_can_terminate` implement the cancellable quit (spec §15).

## 5. What Windows must do differently (watch-outs)

- Window ids and window *pointers*: `WindowSnapshot` carries no pointer — the
  shell maps its own `HWND`/`NSWindow` registry by id.
- Menus: Win32 `HMENU` is implicit per-accelerator handling; accelerators
  translate through `TranslateAccelerator` — the `accelerator` strings parse
  through `MenuDefinition` before building the `ACCEL` table.
- Sheets: `windowId` on dialogs maps to `_WIN32`-style parented dialogs; keep
  the response mapping (first button = `confirmed`) identical.
- DPI: `frame.{x,y,width,height}` are logical points on macOS; on Windows set
  the process DPI-aware mode and answer logical points too (scale inside the
  shell), so apps do not see platform-specific units.

## 6. Verifying a shell

1. `cargo test` — the model round-trips in `kitron-core/src/shell.rs` and the
   supervisor protocol tests must pass unchanged for the new shell target.
2. `tests/bridge-conformance.mjs` — extend `tests/` with the same triangle for
   the Windows bridge (the × 3 surfaces) when the Windows bridge exists.
3. `KITRON_E2E_MACOS=1` runs the macOS end-to-end; write the Windows analog
   (`tests/e2e-windows.ps1`) around `cargo build` + `msbuild RNW app` +
   launch + protocol smoke before declaring §s done.
