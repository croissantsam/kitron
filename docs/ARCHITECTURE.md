# Architecture

```text
React Native (UI)
     │
JavaScript / TypeScript (logic)
     │
JSI / Native Module (prod) · IPC socket/stdio (MVP dev)
     │
Rust Core (windows, app, dialogs, clipboard, shell, node sidecar)
     │
macOS (AppKit, Foundation, UserNotifications, NSPasteboard, NSWindow, NSStatusItem)
Windows (Win32, WinRT, Shell, Notifications)
```

## Threads (§32)

```text
UI thread:        React Native + interaction (never blocked)
Rust background:  native IO, window ops, updater verify
Node background:  backend HTTP/WS/RPC/DB/workers
```

All bridge calls are async. High-frequency paths use typed JSI; the MVP socket uses the same `{id,domain,method,params}` envelope so transports are swappable.

## Packages → Rust mapping

| TS (`packages/`) | Rust (`kitron-runtime`) | OS |
|---|---|---|
| `@kitron/window` | `windows::WindowManager` (winit) | NSWindow / Win32 |
| `@kitron/dialog` | `dialogs` (rfd) | NSOpenPanel / GetOpenFileName |
| `@kitron/clipboard` | `clipboard` (arboard) | NSPasteboard / Win32 clipboard |
| `@kitron/notification` | `notification` | UserNotifications / WinRT |
| `@kitron/shell` | `shell` (open) | NSWorkspace / ShellExecute |
| `@kitron/process` | `process_mgr` | tokio::process |
| `@kitron/node` | `node_sidecar` | `node backend/index.js` child (piped stdio RPC) |
| `@kitron/updater` | `updater` (ed25519 verify) | — |
| `@kitron/tray` | `tray::TrayManager` (registry + events; OS icon Phase 4) | NSStatusItem / Win32 (Phase 4) |
| `@kitron/menu` | `menu::MenuManager` (registry + events; OS menu Phase 4) | NSMenu / HMENU (Phase 4) |
| `@kitron/shortcut` | `shortcuts::ShortcutManager` (registry + events; OS binding Phase 4) | RegisterHotKey / CGEventTap (Phase 4) |
| core `app` | `app::AppManager` | activate (macOS) vs quit-on-close (Win) |

## Backend RPC protocol (§30)

Rust spawns the backend with piped stdio and speaks NDJSON:

```text
Rust → Node: { "id": 1, "procedure": "users.get", "params": { "id": 7 } }
Node → Rust: { "id": 1, "ok": true, "result": { "id": 7 } }
Node → Rust: { "id": 1, "ok": false, "error": { "code": "USER_NOT_FOUND", "message": "..." } }
```

Backends implement it with `defineBackend` from `@kitron/node/backend`
(see `templates/basic/backend/index.ts`). Non-JSON stdout lines are ignored,
so `console.log` debugging is safe; `console.error` goes to the host stderr.
Backend `{code,message}` errors pass through to the UI caller unchanged.

## Security (§23)

Sensitive APIs (filesystem, shell, process, network, camera/mic, notifications) are explicit per-domain methods. There is no `native.executeAnything()`. Updates are ed25519-verified before install.
