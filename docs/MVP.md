# MVP checklist (§41)

- [x] Own renderer (`@kitron/ui` → taffy layout → tiny-skia paint; no RN fork, no HTML)
- [x] Mainline React (`@kitron/react` reconciler host: JSX, hooks, state)
- [x] RN lib compat (`@kitron/react-native`: FlatList/Pressable/ScrollView/Image/Alert/Animated; `react-native` npm alias; guided error for native-code libs)
- [x] Native window works (`winit` → AppKit/Win32; `WindowManager` + `kitron-runtime run`)
- [x] TypeScript works (strict `tsconfig.base`, typed facades per package)
- [x] Bridge works (`packages/core` bridge + `runtime` dispatch, same envelope; JSI-ready)
- [x] Rust works (`cargo build` / `cargo test` in `runtime/rust`)
- [x] Node.js can be launched (`node_sidecar` + `@kitron/node` start/stop/status)
- [x] RN → Rust (`window.create` → dispatch → `WindowManager`)
- [x] RN → Node (`backend.invoke` → sidecar stdio NDJSON → `defineBackend`; typed error passthrough)

## Phase 2 (§42) — native APIs

- [x] Dialog (`dialog.openFile/saveFile/selectDirectory/message` → `rfd`; cancel → `DIALOG_CANCELLED` → `null`)
- [x] Clipboard (`arboard`; text now, html/image/files later)
- [x] Shell (`openExternal/openPath/showItemInFolder`; scheme allowlist)
- [x] Process (`spawn`/`kill` via `tokio::process`; prefer `node:child_process` when possible)
- [x] Notification (native via `notify-rust`)
- [x] Menu (registry + validation + `menu.action` events; `NSMenu`/HMENU rendering is Phase 4)
- [x] Tray (registry + `tray.clicked` events; `NSStatusItem`/notify-icon rendering is Phase 4)

## Phase 3 (§43) — backend + OS integration

- [x] Node backend (`start/stop/restart/status` + `backend.invoke` RPC, no forced HTTP localhost)
- [x] IPC events over stdio (`tray.clicked`, `menu.action`, `shortcut.*`, `app.deepLink`, `app.fileOpen`)
- [x] Deep links / file associations (OS entry points + `app.deepLink`/`app.fileOpen` events)
- [x] Global shortcuts (registry + validation + press routing; OS hotkey binding is Phase 4)
- [x] Auto updater (`check` over file/https feeds + ed25519 `verify`; download/install is Phase 4)
- [ ] Workers (Node `worker_threads` from backend; no runtime work needed — document)
- [ ] Windows (compiles cross-platform via `winit`; Phase 4: CI, signing, installers)
- [x] macOS works (AppKit path via winit; `NSOpenPanel`/notifications/clipboard wired)
- [x] Packaging (`kitron build` + `kitron package` → `.app` skeleton; `.dmg`/`.msi` in Phase 4)

## Verify

```bash
npm install
npm run build --workspaces --if-present
cargo test --manifest-path runtime/rust/Cargo.toml
node --test tests/
node cli/dist/cli.js doctor
```
