//! The ABI-level integration probe (Windows).
//!
//! Drives the whole shell surface through the exported C ABI — the same
//! surface the bridge exposes to TypeScript — on a real Windows session
//! (the CI runner has a desktop). Modal dialogs are never shown: the probe
//! stops at the typed-error paths. This is the CI twin of the macOS probe
//! (e2e), spec §41.
//!
//! Contract notes exercised here (docs/shell-contracts.md):
//! - the preventClose veto applies to the user-driven WM_CLOSE path; the
//!   shell-level `request quit` destroys windows directly (macOS parity),
//! - creating window 1 pushes `app.ready` and wires the RNW renderer hook.

#![cfg(windows)]

use std::ffi::{c_char, CStr, CString};
use std::sync::Mutex;

const CONFIG: &str = concat!(
    r#"{"app":{"name":"probe","identifier":"dev.kitron.probe","version":"0.1.0"},"#,
    r#""permissions":["window","clipboard","dialog","shell"]}"#
);

/// Every event the core pushes lands here, ordered.
fn captured() -> &'static Mutex<Vec<String>> {
    static CAPTURED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    &CAPTURED
}

extern "C" fn event_sink(_context: usize, name: *const c_char, payload: *const c_char) {
    let name = unsafe { CStr::from_ptr(name) }.to_string_lossy().into_owned();
    let payload = unsafe { CStr::from_ptr(payload) }.to_string_lossy().into_owned();
    captured().lock().unwrap().push(format!("{name} {payload}"));
}

fn c_string(json: &str) -> *mut c_char {
    CString::new(json).unwrap().into_raw()
}

/// Takes ownership, parses, releases. Panics loudly: the probe is a gate,
/// not a lenient client.
fn envelope(raw: *mut c_char, what: &str) -> serde_json::Value {
    assert!(!raw.is_null(), "{what}: null envelope");
    let json = unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned();
    kitron_windows::kitron_free_string(raw);
    assert!(json.starts_with("{\"ok\":true"), "{what} answered failure: {json}");
    serde_json::from_str(&json)
        .unwrap_or_else(|error| panic!("{what}: probe envelopes are JSON ({error})"))
}

fn ok_result(raw: *mut c_char, what: &str) -> serde_json::Value {
    envelope(raw, what)
        .get("result")
        .cloned()
        .unwrap_or(serde_json::json!({}))
}

/// Parses only to pull the error message out of a failure envelope.
fn envelope_typed(raw: *mut c_char, what: &str) -> String {
    assert!(!raw.is_null(), "{what}: null envelope");
    let json = unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned();
    kitron_windows::kitron_free_string(raw);
    assert!(json.starts_with("{\"ok\":false"), "{what}: expected failure: {json}");
    json
}

fn event(name: &str) -> bool {
    captured()
        .lock()
        .unwrap()
        .iter()
        .any(|entry| entry.starts_with(name))
}

#[test]
fn abi_probe() {
    // SAFETY: bootstrap is once-per-process; the harness runs this file as a
    // single sequential test. Env is read once, by the core, at bootstrap.
    unsafe {
        std::env::set_var("KITRON_CONFIG_JSON", CONFIG);

        // 1. Bootstrap + app bits.
        ok_result(kitron_windows::kitron_bootstrap(), "bootstrap");
        let bits = ok_result(kitron_windows::kitron_app_bits_json(), "appBits");
        assert_eq!(bits.get("name").and_then(|v| v.as_str()), Some("probe"));
        assert_eq!(bits.get("platform").and_then(|v| v.as_str()), Some("windows"));
        assert_eq!(
            bits.get("identifier").and_then(|v| v.as_str()),
            Some("dev.kitron.probe")
        );
        assert_eq!(bits.get("version").and_then(|v| v.as_str()), Some("0.1.0"));

        // 2. Push sink first — the install-time flush hands over boot events.
        kitron_windows::kitron_set_event_sink(0, Some(event_sink));

        // 3. Window 1 — the RNW host window.
        let create = ok_result(
            kitron_windows::kitron_window_create(c_string(
                r#"{"title":"Probe","width":800,"height":600,"focus":false}"#,
            )),
            "windowCreate",
        );
        assert_eq!(create.get("id").and_then(|v| v.as_u64()), Some(1));
        let window_ptr = create
            .get("windowPtr")
            .and_then(|v| v.as_u64())
            .unwrap_or_default();
        assert_ne!(window_ptr, 0, "create must hand the renderer its HWND");
        assert_eq!(
            kitron_windows::kitron_windows_renderer_hwnd() as u64,
            window_ptr,
            "the hook and the envelope agree on the HWND"
        );
        assert!(event("app.ready"), "window 1 creation pushes app.ready");
        assert!(event("window.created"), "…and window.created");

        // 4. Every surface op (close exercised last; the WindowOp ladder
        //    keeps wire names honest — no invented names).
        for (op, args) in [
            ("setSize", r#"{"width":640,"height":480}"#),
            ("setPosition", r#"{"x":40,"y":60}"#),
            ("setTitle", r#"{"title":"renamed"}"#),
            ("setPreventClose", r#"{"preventClose":true}"#),
            ("minimize", "{}"),
            ("restore", "{}"),
            ("maximize", "{}"),
            ("focus", "{}"),
            ("show", "{}"),
            ("hide", "{}"),
        ] {
            let raw =
                kitron_windows::kitron_window_op(1, c_string(op), c_string(args));
            ok_result(raw, &format!("op {op}"));
        }

        // 5. Snapshot truth: logical frame (DPI 96 runner → 1.0) + state.
        let snapshot = ok_result(kitron_windows::kitron_window_get(1), "windowGet");
        assert_eq!(snapshot.get("title").and_then(|v| v.as_str()), Some("renamed"));
        assert_eq!(snapshot.get("visible").and_then(|v| v.as_bool()), Some(false));
        let frame = snapshot.get("frame").cloned().unwrap_or_default();
        assert_eq!(frame.get("width").and_then(|v| v.as_f64()), Some(640.0));

        let list = ok_result(kitron_windows::kitron_window_list(), "windowList");
        assert_eq!(list.as_array().map(|v| v.len()), Some(1), "one live window");

        let hosted = ok_result(kitron_windows::kitron_snapshot_json(), "snapshot");
        assert_eq!(
            hosted.get("app").and_then(|v| v.get("name")).and_then(|v| v.as_str()),
            Some("probe"),
            "the snapshot carries app bits"
        );
        assert_eq!(
            hosted.get("windows").and_then(|v| v.as_array()).map(|v| v.len()),
            Some(1)
        );

        // 6. Menu + accelerator table + WM_COMMAND → menu.action.
        ok_result(
            kitron_windows::kitron_menu_set_application_menu(c_string(
                r#"[{"label":"File","items":[
                    {"label":"Ping","action":"host.ping","accelerator":"CmdOrCtrl+P"},
                    {"label":"Quit","action":"app.quit"}]}]"#,
            )),
            "menu",
        );
        assert_ne!(
            kitron_windows::kitron_windows_accelerators(),
            0,
            "the accelerator table exists after a menu with an accel"
        );
        // Real user path: the window's own WM_COMMAND dispatch (synchronous).
        use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_CLOSE, WM_COMMAND};
        let hwnd = HWND(window_ptr as *mut core::ffi::c_void);
        SendMessageW(hwnd, WM_COMMAND, Some(WPARAM(1)), Some(LPARAM(0)));
        assert!(event("menu.action"), "WM_COMMAND command id 1 → menu.action");
        assert!(
            captured()
                .lock()
                .unwrap()
                .iter()
                .any(|entry| entry.starts_with("menu.action ") && entry.contains("host.ping")),
            "the menu.action payload carries the action id"
        );

        // 7. Clipboard roundtrip (permission granted in the probe config).
        ok_result(
            kitron_windows::kitron_clipboard_write(c_string("kitron")),
            "clipboardWrite",
        );
        let text = ok_result(kitron_windows::kitron_clipboard_read(), "clipboardRead");
        assert_eq!(
            text.get("text").and_then(|v| v.as_str()),
            Some("kitron"),
            "clipboard roundtrip"
        );

        // 8. Typed-error paths (never modal): >3 buttons and an unknown id.
        assert!(
            envelope_typed(
                kitron_windows::kitron_dialog_message(c_string(
                    r#"{"message":"x","buttons":["a","b","c","d"]}"#
                )),
                "fourButtons"
            )
            .contains("DIALOG_BUTTONS_UNSUPPORTED"),
            "four buttons refuse with the typed code"
        );
        assert!(
            envelope_typed(kitron_windows::kitron_window_get(99), "unknownWindow")
                .contains("WINDOW_NOT_FOUND"),
            "an unknown window id answers WINDOW_NOT_FOUND"
        );

        // 9. The preventClose veto — user-path only (WM_CLOSE). The window
        //    stays alive: no window.closed, registry keeps it.
        assert!(
            !event("window.closed"),
            "nothing closed the window before the veto test"
        );
        SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
        let list = ok_result(kitron_windows::kitron_window_list(), "vetoList");
        assert_eq!(
            list.as_array().map(|v| v.len()),
            Some(1),
            "WM_CLOSE with preventClose=true must not close the window"
        );

        // 10. Un-veto: WM_CLOSE now closes for real → teardown events.
        ok_result(
            kitron_windows::kitron_window_op(
                1,
                c_string("setPreventClose"),
                c_string(r#"{"preventClose":false}"#),
            ),
            "preventClose off",
        );
        SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0)));
        assert!(event("window.closed"), "WM_CLOSE pushes window.closed");
        assert!(event("app.quit"), "the last window closing quits the app");
        let list = ok_result(kitron_windows::kitron_window_list(), "emptyList");
        assert_eq!(list.as_array().map(|v| v.len()), Some(0), "registry empty");
        assert_eq!(
            kitron_windows::kitron_windows_renderer_hwnd(),
            0,
            "the renderer hook clears with the window"
        );

        // 11. Request-quit lifecycle with no windows left (shell path).
        ok_result(kitron_windows::kitron_app_request_quit(), "requestQuit");
        assert!(event("app.beforeQuit"), "requestQuit pushes beforeQuit first");
        assert_eq!(kitron_windows::kitron_app_can_terminate(), 1, "quit allowed");
        ok_result(kitron_windows::kitron_app_cancel_termination(), "cancelTerm");

        // 12. The drain stays a valid envelope in pull mode too.
        ok_result(kitron_windows::kitron_events_drain(), "drain");
    }
}
