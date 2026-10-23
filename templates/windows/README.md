# Windows app glue — machine-step build

This directory is vendored by `kitron create` into the app at
`windows/kitron/`. Nothing here compiles on macOS/CI: the Windows
application build happens on a real Windows machine (this is the
"machine step" of docs/shell-contracts.md §7). What CI verifies today:

- the `kitron-windows` shell crate + the full ABI probe
  (`cargo test -p kitron-windows-host` on windows-latest),
- the `kitron-windows-host` smoke boot (window 1, menu, WM_COMMAND →
  `menu.action`, `app.quit` → clean pump exit),
- `KitronAttachReact.h` ↔ `host_main.cpp` signature conformance.

## Architecture (one source of truth)

Kitron owns the host process — the same binary CI probes every push:

1. `kitron-windows-host` bootstraps the core, installs the push sink,
   creates window 1 and the app menu, and runs the Win32 pump with
   `TranslateAcceleratorW` (everything already heated on CI);
2. React Native for Windows renders INTO window 1: `attach_kitron_react`
   (this directory) initializes `Microsoft.ReactNative` against the HWND
   the ABI answers on `kitron_window_create`'s `windowPtr` /
   `kitron_windows_renderer_hwnd()`. The host compiles this file with
   `KITRON_ATTACH_REACT` defined; without the define (probe/smoke) the
   seam is skipped and the shell still boots.

Dev never touches this: the framework (Kitron) performs every step below,
the same way the macOS side owns CocoaPods.

## Machine-step checklist

Prerequisites: VS 2022 with "Desktop development with C++" (MSVC +
Windows SDK), Rust MSVC toolchain, Node. GN not involved.

1. Restore `Microsoft.ReactNative` (NuGet) — the version must match the
   `react-native-windows` npm package pinned in the app's
   `package.json`. `nuget restore` into `windows/packages/`.
2. Build the host in the app workspace from `.kitron/rust-target`…
   actually: the app's vendored rust target directory —
   `cargo build -p kitron-windows-host` with:
   - `KITRON_ATTACH_REACT` defined (env or build.rs flag),
   - the `Microsoft.ReactNative.Cxx` include dirs and `.lib` hand-off
     coming from the restored NuGet package (the glue's `#include`
     lines spell the expected layout).
3. Sync the vendored `windows/kitron/` glue (already done by
   `kitron create` / `kitron update` on any OS).
4. Run `kitron dev` on Windows — the CLI starts Metro, exports
   `KITRON_CONFIG_JSON`, launches the host exe; React renders inside
   window 1 (hot reload through Metro as usual).
5. Validate: attach panic-guard (the glue dies loudly if RNW fails to
   init — no silent degradation), the ABI probe assertions already run
   in CI, the payload-diff session with the macOS twin (window ops,
   dialogs, menus, clipboard, permissions denials, push events — an
   empty diff is the goal).
6. Later (packaging): WiX MSI installers bundle the exe, the
   `Microsoft.ReactNative` runtime DLLs, and the vendored node runtime.

## Files

| File | Role |
|------|------|
| `KitronHost.h` / `KitronHost.cpp` | The RNW attach glue (compiled only with `KITRON_ATTACH_REACT`) |
| this README | The machine-step `plan-of-record` — keep it in sync with reality |
