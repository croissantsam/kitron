//! Native macOS shell for the Kitron runtime (spec §25).
//!
//! Responsibilities:
//! - core host wiring used by the Swift app bootstrap
//! - NSWindow management on the main thread (spec §10)
//! - a basic native application menu (spec §18; full Menu API is phase 2)
//! - window delegate events → core event bus (spec §32: never block threads)
//!
//! The ObjC++ bridge in `runtime/macos/pod` consumes the `extern "C"` surface
//! at the bottom of this file. Every mutating call returns a JSON envelope:
//! `{"ok": true, "result": ...}` / `{"ok": false, "error": {...}}` (spec §33).

pub mod menu;
pub mod window;

use std::collections::BTreeMap;
use std::ffi::{c_char, CString, CStr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use kitron_core::{Core, EventName};
use kitron_protocol::KitronError;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

use crate::window::{apply_op, cached_snapshots, create_window, purge_closing_windows, refresh_snapshot_cache, snapshots, window_snapshot};
// ---------------------------------------------------------------------------
// Runtime singletons
// ---------------------------------------------------------------------------

static CORE: OnceLock<Core> = OnceLock::new();
static HOST: OnceLock<MacosHost> = OnceLock::new();
static TERMINATION_CANCELLED: AtomicBool = AtomicBool::new(false);

/// Registry of live native windows owned by Kitron (spec §10: multi-window).
pub struct MacosHost {
    pub windows: Mutex<BTreeMap<u64, window::WindowEntry>>,
    pub next_window_id: AtomicU64,
}

impl MacosHost {
    fn new() -> Self {
        Self {
            windows: Mutex::new(BTreeMap::new()),
            next_window_id: AtomicU64::new(1),
        }
    }
}

pub(crate) fn core() -> &'static Core {
    CORE.get_or_init(Core::from_env)
}

pub(crate) fn host() -> &'static MacosHost {
    HOST.get_or_init(MacosHost::new)
}

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

/// Runtime bootstrap. MUST run on the main thread (the Swift bootstrap calls
/// it from `applicationDidFinishLaunching`).
pub(crate) fn bootstrap() -> Result<(), KitronError> {
    let core = core();
    let _host = host();

    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return Err(KitronError::code(
            "THREAD_MAIN_REQUIRED",
            "kitron bootstrap must run on the main thread",
        ));
    };
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    // Standard application menu (spec §18 MVP base).
    menu::install_application_menu(&core.config.app.name);

    // Start the embedded Node backend when `node.autoStart` is set (§28). The
    // supervisor spawns + returns synchronously; the NDJSON handshake happens
    // on its reader thread. Failure is not fatal to the UI launch.
    if core.config.node.auto_start {
        match core.start_node_from_config() {
            Ok(supervisor) => {
                let pid = supervisor.status().pid.map(|p| p.to_string()).unwrap_or_default();
                core.logger.info("node", &format!("backend starting (pid {pid})"));
            }
            Err(e) => {
                core.logger
                    .error("node", &format!("backend start failed: {} ({})", e.code, e.message))
            }
        }
    }

    core.logger.info("macos", "runtime bootstrapped");
    schedule_smoke_quit_if_configured(core);
    Ok(())
}

/// Smoke-test helper: `KITRON_SMOKE_QUIT_MS` env forces a process exit after
/// the timeout so headless integration tests can assert a passing window boot.
fn schedule_smoke_quit_if_configured(core: &Core) {
    if let Some(ms) = std::env::var("KITRON_SMOKE_QUIT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
    {
        let timeout = Duration::from_millis(ms);
        let logger = core.logger.clone();
        std::thread::Builder::new()
            .name("kitron-smoke-quit".to_string())
            .spawn(move || {
                std::thread::sleep(timeout);
                logger.info("macos", "smoke test quit");
                std::process::exit(0);
            })
            .ok();
    }
}

// ---------------------------------------------------------------------------
// App-level operations (spec §11)
// ---------------------------------------------------------------------------

fn app_hide() -> Result<(), KitronError> {
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return Err(KitronError::code(
            "THREAD_MAIN_REQUIRED",
            "app visibility must run on the main thread",
        ));
    };
    NSApplication::sharedApplication(mtm).hide(None);
    Ok(())
}

fn app_show() -> Result<(), KitronError> {
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return Err(KitronError::code(
            "THREAD_MAIN_REQUIRED",
            "app visibility must run on the main thread",
        ));
    };
    let app = NSApplication::sharedApplication(mtm);
    app.unhide(None);
    app.activate();
    Ok(())
}

pub(crate) fn app_can_terminate() -> bool {
    !TERMINATION_CANCELLED.load(Ordering::SeqCst)
}

// ---------------------------------------------------------------------------
// JSON helpers for the C bridge
// ---------------------------------------------------------------------------

pub(crate) fn json_to_c_string(value: serde_json::Value) -> *mut c_char {
    let raw = serde_json::to_string(&value).unwrap_or_else(|_| r#"{"ok":false}"#.to_string());
    CString::new(raw)
        .unwrap_or_else(|_| CString::new(r#"{"ok":false}"#).unwrap())
        .into_raw()
}

pub(crate) fn status_ok(result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "ok": true, "result": result })
}

pub(crate) fn status_err(error: &KitronError) -> serde_json::Value {
    serde_json::json!({ "ok": false, "error": error.to_json() })
}

fn parse_args(pointer: *const c_char) -> serde_json::Value {
    unsafe { c_string_to_json(pointer) }.unwrap_or(serde_json::json!({}))
}

// ---------------------------------------------------------------------------
// extern "C" ABI — consumed by runtime/macos/pod/KitronBridge.mm
// ---------------------------------------------------------------------------

pub(crate) unsafe fn c_string_to_json(pointer: *const c_char) -> Option<serde_json::Value> {
    if pointer.is_null() {
        return None;
    }
    let raw = unsafe { CStr::from_ptr(pointer) }.to_string_lossy();
    serde_json::from_str(&raw).ok()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_free_string(pointer: *mut c_char) {
    if pointer.is_null() {
        return;
    }
    drop(unsafe { CString::from_raw(pointer) });
}

/// Bootstrap the runtime. Main thread only.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_bootstrap() -> *mut c_char {
    match bootstrap() {
        Ok(()) => json_to_c_string(status_ok(serde_json::json!({}))),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// App-information snapshot for the JSI host object (envelope: `{ok, result}`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_app_bits_json() -> *mut c_char {
    let core = core();
    json_to_c_string(status_ok(serde_json::json!({
        "pid": std::process::id(),
        "name": core.config.app.name,
        "identifier": core.config.app.identifier,
        "version": core.config.app.version,
        "platform": kitron_protocol::platform_name(),
        "dev": core.dev_mode,
        "rootDir": core.root_dir,
    })))
}

/// Creates a window. Args: `{"title":..., "width":..., "height":..., "focus":...}`.
/// Result: `{"id": N, "windowPtr": P}` where `windowPtr` is a borrowed
/// `NSWindow*` handed to the RN factory (Kitron owns the window).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_window_create(args: *const c_char) -> *mut c_char {
    let parsed = parse_args(args);
    match create_window(&parsed) {
        Ok(created) => json_to_c_string(status_ok(created)),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Window op. `op` ∈ close|minimize|maximize|restore|focus|setSize|
/// setPosition|show|hide|setTitle|setPreventClose.
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
    match apply_op(id, &op_name, &parsed) {
        Ok(()) => json_to_c_string(status_ok(serde_json::json!({ "id": id }))),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Snapshot of a single window (or error envelope).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_window_get(id: u64) -> *mut c_char {
    match window_snapshot(id) {
        Ok(snap) => json_to_c_string(status_ok(serde_json::to_value(snap).unwrap_or_default())),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// List of live windows.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_window_list() -> *mut c_char {
    json_to_c_string(status_ok(
        serde_json::to_value(snapshots()).unwrap_or_default(),
    ))
}

/// App visibility. `mode` ∈ hide|show.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_app_visibility(mode: *const c_char) -> *mut c_char {
    let mode = unsafe { CStr::from_ptr(mode) }.to_string_lossy().into_owned();
    let result = match mode.as_str() {
        "hide" => app_hide(),
        "show" => app_show(),
        other => {
            let e = KitronError::code("APP_VISIBILITY_UNKNOWN", format!("unknown mode {other}"));
            return json_to_c_string(status_err(&e));
        }
    };
    match result {
        Ok(()) => json_to_c_string(status_ok(serde_json::json!({ "mode": mode }))),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Requests app termination (`NSApplication.terminate`); JS receives
/// `app.beforeQuit` via the event pump (spec §11).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_app_request_quit() -> *mut c_char {
    core().emit(EventName::AppBeforeQuit, serde_json::json!({}));
    if let Some(mtm) = objc2::MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).terminate(None);
    } else {
        core().logger.error("macos", "quit requested off the main thread; ignoring");
    }
    json_to_c_string(status_ok(serde_json::json!({})))
}

/// Whether pending termination may continue (JS veto support grows in phase 3).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_app_can_terminate() -> i32 {
    i32::from(app_can_terminate())
}

/// JS veto of a pending termination.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_app_cancel_termination() -> *mut c_char {
    TERMINATION_CANCELLED.store(true, Ordering::SeqCst);
    json_to_c_string(status_ok(serde_json::json!({})))
}

/// Node status snapshot for the JSI host object (sync).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_node_status_json() -> *mut c_char {
    let status = kitron_core::node::node_status(core());
    json_to_c_string(status_ok(
        status
            .map(|s| serde_json::to_value(&s).unwrap_or_default())
            .unwrap_or(serde_json::Value::Null),
    ))
}

/// Node lifecycle. `mode` ∈ start|stop.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_node(mode: *const c_char) -> *mut c_char {
    let mode = unsafe { CStr::from_ptr(mode) }.to_string_lossy().into_owned();
    let core = core();
    match mode.as_str() {
        "start" => {
            match core.start_node_from_config() {
                Ok(supervisor) => {
                    core.logger.info("node", "backend starting");
                    json_to_c_string(status_ok(serde_json::json!({
                        "pid": supervisor.status().pid,
                    })))
                }
                Err(e) => json_to_c_string(status_err(&e)),
            }
        }
        "stop" => {
            core.stop_node("requested");
            json_to_c_string(status_ok(serde_json::json!({})))
        }
        other => {
            let e = KitronError::code("NODE_MODE_UNKNOWN", format!("unknown node mode {other}"));
            json_to_c_string(status_err(&e))
        }
    }
}

/// Blocking invoke into the Node backend. Called from bridge worker threads
/// only (never the JS thread).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_node_invoke_wait(
    method: *const c_char,
    payload: *const c_char,
    timeout_ms: u32,
) -> *mut c_char {
    let method_name = if method.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(method) }.to_string_lossy().into_owned()
    };
    let payload = parse_args(payload);
    let payload = if payload.is_null() { None } else { Some(payload) };

    match kitron_core::node::invoke_supervisor(
        core(),
        &method_name,
        payload,
        Duration::from_millis(timeout_ms as u64),
    ) {
        Ok(result) => json_to_c_string(status_ok(result)),
        Err(e) => json_to_c_string(status_err(&e)),
    }
}

/// Full snapshot used by the JSI host object sync getters.
///
/// Uses the cached window list — this can be invoked from the JS thread, and
/// NSWindow is MainThreadOnly. The cache refreshes on the main thread after
/// every window op and event drain.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_snapshot_json() -> *mut c_char {
    let bits_pointer = unsafe { kitron_app_bits_json() };
    let app = unsafe { c_string_to_json(bits_pointer) }
        .and_then(|envelope| envelope.get("result").cloned());
    unsafe { kitron_free_string(bits_pointer) };
    let windows = window::cached_snapshots();
    let node = kitron_core::node::node_status(core());
    json_to_c_string(status_ok(serde_json::json!({
        "app": app,
        "windows": windows,
        "node": node.map(|n| serde_json::to_value(n).unwrap_or_default()),
    })))
}

/// Drains the event bus for the bridge pump. Result: `{"ok":true,"result":[...events]}`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_events_drain() -> *mut c_char {
    // Main thread only (the pump runs on the main queue): safe to refresh.
    purge_closing_windows();
    refresh_snapshot_cache();
    let events: Vec<serde_json::Value> = core()
        .events()
        .drain()
        .into_iter()
        .map(|event| {
            serde_json::json!({
                "name": event.name,
                "windowId": event.window_id,
                "payload": event.payload,
            })
        })
        .collect();
    json_to_c_string(status_ok(serde_json::json!(events)))
}
