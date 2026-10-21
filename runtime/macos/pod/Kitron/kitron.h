#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Kitron Rust ABI (mirror of runtime/rust/crates/kitron-macos, spec §8).
 *
 * Every function returning `char*` returns a NUL-terminated JSON *envelope*:
 *   success: {"ok":true,"result":...}
 *   failure: {"ok":false,"error":{"code":"...","message":"...","platform":"macos"}}
 *
 * Returned strings are owned by the caller → must be released with
 * `kitron_free_string`. Functions touching AppKit must run on the main
 * thread; pure-Rust calls (node invoke/status) are free-threaded.
 */

/// Releases strings returned by the ABI.
void kitron_free_string(char *pointer);

/// Initializes the core (config from `KITRON_CONFIG_JSON`, logger, node wiring).
char *kitron_bootstrap(void);

/// App info: {pid, name, identifier, version, platform, dev, rootDir}.
char *kitron_app_bits_json(void);

/// Creates a window. args: {"title","width","height","focus","resizable",...}
/// result: {"id":N,"windowPtr":P} — `windowPtr` is a borrowed NSWindow*.
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

/// Requests app termination (NSApp.terminate; JS gets app.beforeQuit first).
char *kitron_app_request_quit(void);

/// 1 when a pending termination may continue.
int32_t kitron_app_can_terminate(void);

/// JS veto of a pending termination.
char *kitron_app_cancel_termination(void);

/// Node status snapshot (pure Rust, free-threaded).
char *kitron_node_status_json(void);

/// Node lifecycle: "start" | "stop".
char *kitron_node(const char *mode);

/// Blocking backend invoke (pure Rust, free-threaded — run on a worker queue).
char *kitron_node_invoke_wait(const char *method, const char *payload_json,
                              uint32_t timeout_ms);

/// Full snapshot {app, windows, node} for the JSI host object (cached reads,
/// safe from the JS thread).
char *kitron_snapshot_json(void);

/// Drains the event bus: result = [{name, windowId, payload}, ...].
/// Main thread only (refreshes window snapshots touching AppKit).
char *kitron_events_drain(void);

#ifdef __cplusplus
} // extern "C"
#endif
