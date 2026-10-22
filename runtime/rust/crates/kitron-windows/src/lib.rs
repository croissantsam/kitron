//! Kitron shell for Windows (spec §40 — `kitron-windows`).
//!
//! Twin of `kitron-macos`: identical C ABI (see
//! `runtime/macos/pod/Kitron/kitron.h` and `docs/shell-contracts.md`), same
//! envelopes, same wire models — implemented over Win32 with the `windows`
//! crate. Exactly one shell is linked into a Windows app.
//!
//! Win32 surfaces used here are main-thread only (`THREAD_MAIN_REQUIRED`
//! otherwise), and every sensitive call passes the permission gate before it
//! touches the OS.
//!
//! The whole crate is `cfg(windows)`: on other platforms the library compiles
//! empty, so the portable workspace keeps working for macOS development while
//! Windows CI (`windows-latest`) compiles this for real.

#![cfg(windows)]

pub mod dialog;
pub mod menu;
pub mod services;
pub mod window;

use std::ffi::{c_char, CStr, CString};
use std::sync::{Arc, Mutex, OnceLock};

use kitron_core::EventName;
use kitron_protocol::KitronError;

/// The runtime core instance shared between the shell, the JSI bridge and the
/// Node supervisor.
pub fn core() -> &'static kitron_core::Core {
    static CORE: OnceLock<kitron_core::Core> = OnceLock::new();
    CORE.get_or_init(kitron_core::Core::from_env)
}

/// The event sink the bridge (or the RNW template) calls — same contract as
/// the macOS twin: the callback runs on whatever thread emitted the event.
struct RawEventSink {
    context: usize,
    sink: unsafe extern "C" fn(usize, *const c_char, *const c_char),
}

// SAFETY: the context is only ever handed back to the side that installed it.
unsafe impl Send for RawEventSink {}
unsafe impl Sync for RawEventSink {}

fn raw_event_sink() -> &'static Mutex<Option<RawEventSink>> {
    static SINK: OnceLock<Mutex<Option<RawEventSink>>> = OnceLock::new();
    SINK.get_or_init(|| Mutex::new(None))
}

type SinkFn = unsafe extern "C" fn(usize, *const c_char, *const c_char);

/// Installs the push event sink and flushes the events queued before the UI
/// subscribed (docs/shell-contracts.md §3).
///
/// # Safety
///
/// The callback must be a valid function pointer; the context is an opaque
/// value it understands.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_set_event_sink(context: usize, sink: Option<SinkFn>) {
    *raw_event_sink().lock().unwrap() = sink.map(|callback| RawEventSink { context, sink: callback });
    let installed: Option<kitron_core::EventSink> = sink.map(|_callback| {
        Arc::new(move |name: EventName, payload: serde_json::Value| {
            let guard = raw_event_sink().lock().unwrap();
            if let Some(raw) = guard.as_ref() {
                let name = CString::new(name.as_str()).unwrap_or_default();
                let payload = CString::new(payload.to_string()).unwrap_or_default();
                unsafe { (raw.sink)(raw.context, name.as_ptr(), payload.as_ptr()) };
            }
        }) as kitron_core::EventSink
    });
    core().set_event_sink(installed);

    // Events that happened before the UI subscribed are still in the bus.
    for event in core().events().drain() {
        let Some(name) = EventName::parse(event.name) else {
            continue;
        };
        core().emit(name, event.payload);
    }
}

// ---------------------------------------------------------------------------
// Envelope helpers — the same JSON contract every shell answers with.
// ---------------------------------------------------------------------------

/// Maps a Win32 call failure onto the typed error contract (platform-scoped
/// code the shell reserves for raw system-call failures).
pub(crate) fn werr(error: windows::core::Error) -> KitronError {
    KitronError::code("SHELL_CALL_FAILED", error.to_string())
}

pub(crate) fn status_ok(result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "ok": true, "result": result })
}

pub(crate) fn status_err(error: &KitronError) -> serde_json::Value {
    let platform = std::env::consts::OS; // "windows"
    serde_json::json!({
        "ok": false,
        "error": { "code": error.code.clone(), "message": error.message.clone(), "platform": platform }
    })
}

pub(crate) fn parse_args(args: *const c_char) -> serde_json::Value {
    if args.is_null() {
        return serde_json::json!({});
    }
    let raw = unsafe { CStr::from_ptr(args) }.to_string_lossy();
    serde_json::from_str(raw.as_ref()).unwrap_or_else(|_| serde_json::json!({}))
}

pub(crate) fn json_to_c_string(value: serde_json::Value) -> *mut c_char {
    let c_string = CString::new(value.to_string()).unwrap_or_else(|_| CString::new("{}").unwrap());
    c_string.into_raw()
}

/// Frees a string one of the shell functions returned.
///
/// # Safety
///
/// The pointer must come from this library (or be null).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_free_string(pointer: *mut c_char) {
    if !pointer.is_null() {
        unsafe { drop(CString::from_raw(pointer)) };
    }
}

/// Config-free bootstrap: reads `KITRON_*` env and builds the core for the
/// one-shot processes Node hosts; the shell app calls it before JS starts.
///
/// # Safety
///
/// Safe to call from any thread, once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_bootstrap() -> *mut c_char {
    let core = core();
    core.logger.info("system", "kitron-windows shell bootstrapped");
    json_to_c_string(status_ok(serde_json::json!({})))
}

/// App information for the JSI host object (`app.bits`).
///
/// # Safety
///
/// Safe to call from any thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_app_bits_json() -> *mut c_char {
    let bits = crate::services::app_bits();
    json_to_c_string(status_ok(bits))
}

/// App visibility (`hide` | `show`).
///
/// # Safety
///
/// Must be called on the main thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_app_visibility(mode: *const c_char) -> *mut c_char {
    let mode = unsafe { CStr::from_ptr(mode) }.to_string_lossy().into_owned();
    match mode.as_str() {
        "hide" => crate::window::app_shown(false),
        "show" => crate::window::app_shown(true),
        other => {
            let e = KitronError::code("APP_VISIBILITY_UNKNOWN", format!("unknown mode {other}"));
            return json_to_c_string(status_err(&e));
        }
    }
    json_to_c_string(status_ok(serde_json::json!({ "mode": mode })))
}

/// Requests app termination: JS receives `app.beforeQuit` through the event
/// pump and may veto (spec §15).
///
/// # Safety
///
/// Must be called on the main thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_app_request_quit() -> *mut c_char {
    core().emit(EventName::AppBeforeQuit, serde_json::json!({}));
    crate::window::request_quit();
    json_to_c_string(status_ok(serde_json::json!({})))
}

/// Whether a pending termination may continue.
///
/// # Safety
///
/// Safe to call from any thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_app_can_terminate() -> i32 {
    i32::from(crate::window::can_terminate())
}

/// JS veto of a pending termination.
///
/// # Safety
///
/// Safe to call from any thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_app_cancel_termination() -> *mut c_char {
    crate::window::cancel_termination();
    json_to_c_string(status_ok(serde_json::json!({})))
}

/// Platform lifecycle events pushed by the app window procedure ("activate").
///
/// # Safety
///
/// String arguments must be null or valid NUL-terminated C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_emit_app_event(name: *const c_char) -> *mut c_char {
    let raw = if name.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned()
    };
    let event = match raw.as_str() {
        "activate" => EventName::AppActivate,
        "quit" => EventName::AppQuit,
        other => {
            let e = KitronError::code("APP_EVENT_UNKNOWN", format!("unknown app event {other}"));
            return json_to_c_string(status_err(&e));
        }
    };
    core().emit(event, serde_json::json!({}));
    json_to_c_string(status_ok(serde_json::json!({})))
}

// ---------------------------------------------------------------------------
// Node/backend — the supervisor is portable, the shell just binds it.
// ---------------------------------------------------------------------------

/// Backend status snapshot for the JSI host object.
///
/// # Safety
///
/// Safe to call from any thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_node_status_json() -> *mut c_char {
    json_to_c_string(status_ok(serde_json::json!(kitron_core::node::node_status(core()))))
}

/// Backend supervisor control: `start` | `stop`.
///
/// # Safety
///
/// String arguments must be null or valid NUL-terminated C strings. Must not
/// be called on the JS thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_node(mode: *const c_char) -> *mut c_char {
    let mode = unsafe { CStr::from_ptr(mode) }.to_string_lossy().into_owned();
    match crate::services::node_control(&mode) {
        Ok(result) => json_to_c_string(status_ok(result)),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Blocking invoke into the backend. Called from bridge worker threads only.
///
/// # Safety
///
/// String arguments must be null or valid NUL-terminated C strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_node_invoke_wait(
    method: *const c_char,
    payload: *const c_char,
    timeout_ms: u32,
) -> *mut c_char {
    match crate::services::node_invoke(method, payload, timeout_ms) {
        Ok(result) => json_to_c_string(status_ok(result)),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

/// Creates a window. Args: `{"title":…, "width":…, "height":…, "focus":…}`.
/// Result: `{"id": N, "windowPtr": P}` — a borrowed HWND handed to the RNW
/// renderer (Kitron owns the window).
///
/// # Safety
///
/// String arguments must be null or valid NUL-terminated C strings. Must be
/// called on the main thread (Win32 rules).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_window_create(args: *const c_char) -> *mut c_char {
    let parsed = parse_args(args);
    match window::create_window(&parsed) {
        Ok(result) => json_to_c_string(status_ok(result)),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Snapshot of a single window (or error envelope).
///
/// # Safety
///
/// Must be called on the main thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_window_get(id: u64) -> *mut c_char {
    match window::window_snapshot(id) {
        Ok(snapshot) => json_to_c_string(status_ok(serde_json::to_value(&snapshot).unwrap_or_default())),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Every live window (the async bridge surface; the JSI getter uses the
/// snapshot cache below).
///
/// # Safety
///
/// Must be called on the main thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_window_list() -> *mut c_char {
    let snapshots = window::snapshots();
    json_to_c_string(status_ok(serde_json::json!(snapshots)))
}

/// Window op. `op` ∈ close|minimize|maximize|restore|focus|show|hide|setSize|
/// setPosition|setTitle|setPreventClose — the `WindowOp` wire contract.
///
/// # Safety
///
/// String arguments must be null or valid NUL-terminated C strings. Must be
/// called on the main thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_window_op(
    id: u64,
    op: *const c_char,
    args: *const c_char,
) -> *mut c_char {
    let op_name = if op.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(op) }.to_string_lossy().into_owned()
    };
    let parsed = parse_args(args);
    match window::apply_op(id, &op_name, &parsed) {
        Ok(()) => json_to_c_string(status_ok(serde_json::json!({ "id": id }))),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Full snapshot for the JSI host object sync getters (thread-safe: reads the
/// window cache, never an HWND — macOS twin uses the identical pattern).
///
/// # Safety
///
/// Safe to call from any thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_snapshot_json() -> *mut c_char {
    json_to_c_string(status_ok(serde_json::json!({
        "app": crate::services::app_bits(),
        "windows": window::cached_snapshots(),
        "node": kitron_core::node::node_status(core())
            .map(|status| serde_json::to_value(&status).unwrap_or_default()),
    })))
}

/// Drains the event bus for the bridge pump.
///
/// # Safety
///
/// Must be called on the main thread (the pump runs there, like the macOS
/// twin, so the window cache refreshes just before the drain).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_events_drain() -> *mut c_char {
    window::refresh_snapshot_cache();
    json_to_c_string(status_ok(serde_json::json!(crate::services::drain_events())))
}

// ---------------------------------------------------------------------------
// Phase-2 native APIs (spec §42): clipboard, shell, dialogs
// ---------------------------------------------------------------------------

/// Reads the clipboard as text (spec §13). Requires the `clipboard` permission.
///
/// # Safety
///
/// Safe to call from any thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_clipboard_read() -> *mut c_char {
    match crate::services::clipboard_read() {
        Ok(text) => json_to_c_string(status_ok(serde_json::json!({ "text": text }))),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Writes text to the clipboard (spec §13). Requires the `clipboard` permission.
///
/// # Safety
///
/// The pointer must be null or a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_clipboard_write(text: *const c_char) -> *mut c_char {
    let value = if text.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned()
    };
    match crate::services::clipboard_write(&value) {
        Ok(()) => json_to_c_string(status_ok(serde_json::json!({}))),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Opens a URL with the default handler (spec §15). Requires `shell`.
///
/// # Safety
///
/// The pointer must be null or a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_shell_open_url(url: *const c_char) -> *mut c_char {
    let value = read_c_string(url);
    match crate::services::shell_open(&value) {
        Ok(()) => json_to_c_string(status_ok(serde_json::json!({}))),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Opens a path with its default application (spec §15). Requires `shell`.
///
/// # Safety
///
/// The pointer must be null or a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_shell_open_path(path: *const c_char) -> *mut c_char {
    let value = read_c_string(path);
    match crate::services::shell_open(&value) {
        Ok(()) => json_to_c_string(status_ok(serde_json::json!({}))),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Reveals a file in Explorer (spec §15). Requires `shell`.
///
/// # Safety
///
/// The pointer must be null or a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_shell_show_item_in_folder(path: *const c_char) -> *mut c_char {
    let value = read_c_string(path);
    match crate::services::shell_show_item_in_folder(&value) {
        Ok(()) => json_to_c_string(status_ok(serde_json::json!({}))),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Shows a native message box (spec §12). Requires `dialog`.
///
/// # Safety
///
/// The pointer must be null or a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_dialog_message(args: *const c_char) -> *mut c_char {
    let raw = read_c_string(args);
    match dialog::message(&raw) {
        Ok(result) => json_to_c_string(status_ok(result)),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Opens the native file picker (spec §12). Requires `dialog`.
///
/// # Safety
///
/// The pointer must be null or a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_dialog_open_file(args: *const c_char) -> *mut c_char {
    let raw = read_c_string(args);
    match dialog::open_file(&raw) {
        Ok(result) => json_to_c_string(status_ok(result)),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Opens the native save dialog (spec §12). Requires `dialog`.
///
/// # Safety
///
/// The pointer must be null or a valid NUL-terminated C string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_dialog_save_file(args: *const c_char) -> *mut c_char {
    let raw = read_c_string(args);
    match dialog::save_file(&raw) {
        Ok(result) => json_to_c_string(status_ok(result)),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Installs the application menu (spec §18): `MenuDefinition[]`, same wire as
/// macOS; clicks answer `menu.action`.
///
/// # Safety
///
/// The pointer must be null or a valid NUL-terminated C string. Must be
/// called on the main thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_menu_set_application_menu(definition: *const c_char) -> *mut c_char {
    let raw = read_c_string(definition);
    match menu::set_application_menu(&raw) {
        Ok(()) => json_to_c_string(status_ok(serde_json::json!({}))),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

fn read_c_string(pointer: *const c_char) -> String {
    if pointer.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(pointer) }.to_string_lossy().into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The event-sink flush path never double-delivers: queue, install, strip.
    #[test]
    fn sink_setup_checks() {
        // No event here — Core::from_env needs KITRON_* env, which CI supplies
        // for the separator integration tests; unit tests stay for the pure
        // helpers only.
        let value = status_ok(serde_json::json!({ "a": 1 }));
        assert_eq!(value["ok"], true);
        let error = KitronError::code("X", "bad");
        assert_eq!(status_err(&error)["error"]["platform"], "windows");
    }
}
