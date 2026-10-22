#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Kitron Rust ABI for the Windows shell — the fixed mirror of
 * `runtime/macos/pod/Kitron/kitron.h` (spec §8) with HWND wording. The
 * implementation lives in `runtime/rust/crates/kitron-windows` (staticlib)
 * and conformance is enforced at link time by the host probe:
 * a missing or renamed export breaks `cargo test -p kitron-windows-host`.
 *
 * Every function returning `char*` returns a NUL-terminated JSON *envelope*:
 *   success: {"ok":true,"result":...}
 *   failure: {"ok":false,"error":{"code":"...","message":"...","platform":"windows"}}
 *
 * Returned strings are owned by the caller → release with `kitron_free_string`.
 * Window-surface functions must run on the thread that pumps messages (the
 * host's main thread); pure-Rust calls (node invoke/status) are free-threaded.
 */

/// Releases strings returned by the ABI.
void kitron_free_string(char *pointer);

/// Initializes the core (config from `KITRON_CONFIG_JSON`, logger, wiring).
char *kitron_bootstrap(void);

/// App info: {pid, name, identifier, version, platform, dev, rootDir}.
char *kitron_app_bits_json(void);

/// Creates a window. args: {"title","width","height","focus",...}
/// result: {"id":N,"windowPtr":P} — `windowPtr` is a borrowed HWND for the
/// RNW renderer to attach to (the host hands it to Microsoft.ReactNative on
/// the machine step).
char *kitron_window_create(const char *args_json);

/// Window op: close|minimize|maximize|restore|focus|setSize|setPosition|
/// show|hide|setTitle|setPreventClose.
char *kitron_window_op(uint64_t window_id, const char *op, const char *args_json);

/// Live snapshot for one window.
char *kitron_window_get(uint64_t window_id);

/// Array of live window snapshots.
char *kitron_window_list(void);

/// App visibility: hide|show.
char *kitron_app_visibility(const char *mode);

/// Asks the shell to quit (JS gets app.beforeQuit first; veto-able).
char *kitron_app_request_quit(void);

/// 1 when a pending termination may continue.
int32_t kitron_app_can_terminate(void);

/// JS veto of a pending termination.
char *kitron_app_cancel_termination(void);

/// Node status snapshot (pure Rust, free-threaded).
char *kitron_node_status_json(void);

/// Node lifecycle: "start" | "stop".
char *kitron_node(const char *mode);

/// Blocking backend invoke (pure Rust, free-threaded — worker threads).
char *kitron_node_invoke_wait(const char *method, const char *payload_json,
                              uint32_t timeout_ms);

/// Full snapshot {app, windows, node} for the JSI host object.
char *kitron_snapshot_json(void);

/// Drains the event bus: result = [{name, windowId, payload}, ...].
/// Message-pump thread only (refreshes cached window snapshots).
char *kitron_events_drain(void);

/// Push delivery: the core calls `sink` for every runtime event. `context`
/// is the opaque value handed back (the bridge passes its module instance).
typedef void (*KitronEventSink)(uintptr_t context,
                                const char *name,
                                const char *payload_json);

/// Installs (or clears, with `sink == NULL`) the event sink. Events queued
/// before installation flush through it immediately, RN schwarz.
void kitron_set_event_sink(uintptr_t context, KitronEventSink sink);

/// Emits an app lifecycle event ("activate" | "ready" | "beforeQuit" | "quit").
char *kitron_emit_app_event(const char *name);

/* ---------------------------------------------------------------------------
 * Phase-2 native APIs (spec §42). Gated by `permissions` in the config.
 * ------------------------------------------------------------------------- */

/// Clipboard (spec §13): result = {"text": "…"}.
char *kitron_clipboard_read(void);

/// Clipboard (spec §13).
char *kitron_clipboard_write(const char *text);

/// Shell (spec §15): opens a URL with the default handler.
char *kitron_shell_open_url(const char *url);

/// Shell (spec §15): opens a file or folder with the default handler.
char *kitron_shell_open_path(const char *path);

/// Shell (spec §15): reveals a file in Explorer (macOS: Finder).
char *kitron_shell_show_item_in_folder(const char *path);

/// Dialogs (spec §12): message box. Result: {button, index, confirmed}.
char *kitron_dialog_message(const char *args_json);

/// Dialogs (spec §12): file picker. Result: {canceled, paths}.
char *kitron_dialog_open_file(const char *args_json);

/// Dialogs (spec §12): save dialog. Result: {canceled, path}.
char *kitron_dialog_save_file(const char *args_json);

/// Menus (spec §18): installs the application menu; clicks answer
/// `menu.action` events. Args: [{"label":"File","items":[…]}].
char *kitron_menu_set_application_menu(const char *definition_json);

/* ---------------------------------------------------------------------------
 * Integration hooks — beyond the contract ABI, consumed by the host:
 *
 *   void kitron_windows_renderer_hwnd(void) — see below (returns usize).
 *
 * Host-side extras the shell crate exposes for the Win32 message loop and
 * the RNW renderer (not part of the shared shell contract):
 */

/// The accelerator table for the current menu — the host message pump is
/// required to call `TranslateAcceleratorW` with it. 0 when no menu uses
/// accelerators.
uintptr_t kitron_windows_accelerators(void);

/// The HWND of the RNW host window (window id 1), for the renderer attach.
/// 0 when no window exists yet.
uintptr_t kitron_windows_renderer_hwnd(void);

#ifdef __cplusplus
} // extern "C"
#endif
