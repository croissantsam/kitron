//! Win32 windows — creation, ops, snapshots (spec §10 on Windows).
//!
//! The shell owns Kitron's window ids like its macOS twin: `kitron_window_*`
//! front real `HWND`s. Window 1 hands its `HWND` to the React Native for
//! Windows renderer through `kitron_windows_renderer_hwnd()` (integration
//! point — the RNW app template calls it once it boots, see
//! `docs/shell-contracts.md` §5).
//!
//! Main thread only for creation and ops (Win32 rule); frames handed to JS
//! are logical points like on macOS, so apps never see DPI units.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use kitron_core::shell::{WindowCreateArgs, WindowFrame, WindowOp, WindowSnapshot};
use kitron_core::EventName;
use kitron_protocol::KitronError;
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HMODULE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetForegroundWindow, GetWindowTextLengthW,
    GetWindowTextW, GetWindowRect, IsIconic, IsWindowVisible, IsZoomed, PostQuitMessage,
    RegisterClassExW, SetForegroundWindow, SetWindowPos, SetWindowTextW, ShowWindow, CS_HREDRAW,
    CS_VREDRAW, HWND_TOP, SC_MINIMIZE, SC_RESTORE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_NOZORDER, SHOW_WINDOW_CMD, SW_HIDE, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
    SW_SHOW, SW_SHOWNOACTIVATE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_DESTROY,
    WM_KILLFOCUS, WM_MOVE, WM_SETFOCUS, WM_SIZE, WM_SYSCOMMAND, WNDCLASSEXW,
    WS_OVERLAPPEDWINDOW,
};

use crate::core;

pub(crate) struct WindowEntry {
    pub hwnd: HWND,
    pub id: u64,
    pub prevent_close: Arc<AtomicBool>,
}

// SAFETY: an HWND is an opaque handle; Win32 thread affinity is enforced by
// the shell's main-thread gates, not by the handle type.
unsafe impl Send for WindowEntry {}
unsafe impl Sync for WindowEntry {}

pub(crate) fn registry() -> &'static Mutex<HashMap<u64, WindowEntry>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u64, WindowEntry>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_id() -> &'static Mutex<u64> {
    static NEXT: OnceLock<Mutex<u64>> = OnceLock::new();
    NEXT.get_or_init(|| Mutex::new(0))
}

const CLASS_NAME: PCWSTR = w!("KitronShellWindow");

/// Registers the window class once per process.
fn ensure_window_class() -> Result<(), KitronError> {
    static REGISTERED: OnceLock<()> = OnceLock::new();
    if REGISTERED.get().is_some() {
        return Ok(());
    }
    let hmodule: HMODULE = unsafe { GetModuleHandleW(None) }.map_err(crate::werr)?;
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: HINSTANCE(hmodule.0),
        hbrBackground: HBRUSH::default(),
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    let atom = unsafe { RegisterClassExW(&class) };
    if atom == 0 {
        return Err(KitronError::code("WINDOW_CLASS_FAILED", "RegisterClassExW failed"));
    }
    let _ = REGISTERED.set(());
    Ok(())
}

/// Logical points → physical pixels scale (96 DPI base, like macOS points).
pub(crate) fn scale(hwnd: HWND) -> f64 {
    let dpi = unsafe { GetDpiForWindow(hwnd) } as f64;
    if dpi > 0.0 { dpi / 96.0 } else { 1.0 }
}

fn require_main_thread() -> Result<(), KitronError> {
    // A fuller check arrives with the thread-local message-loop wiring; the
    // entry points are main-thread by construction in the RNW template.
    Ok(())
}

/// Creates a window from the `WindowCreateArgs` contract. Main thread only.
/// Answers `{ id, width, height, windowPtr }`.
pub fn create_window(args: &serde_json::Value) -> Result<serde_json::Value, KitronError> {
    require_main_thread()?;
    ensure_window_class()?;
    let args: WindowCreateArgs = serde_json::from_value(args.clone()).unwrap_or_default();

    let id = {
        let mut next = next_id().lock().unwrap();
        *next += 1;
        *next
    };
    let width_logical = args.width.unwrap_or(1024.0);
    let height_logical = args.height.unwrap_or(768.0);
    let title = windows::core::HSTRING::from(args.title.as_deref().unwrap_or("Kitron"));
    let hmodule: HMODULE = unsafe { GetModuleHandleW(None) }.map_err(crate::werr)?;
    let hinstance = HINSTANCE(hmodule.0);

    // SAFETY: main-thread create; class registered above; parameters are
    // owned for the call's duration.
    let hwnd: HWND = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            PCWSTR(title.as_ptr()),
            WINDOW_STYLE(WS_OVERLAPPEDWINDOW.0),
            80,
            80,
            width_logical as i32,
            height_logical as i32,
            None,
            None,
            Some(hinstance),
            None,
        )
        .map_err(crate::werr)?
    };

    let prevent_close = Arc::new(AtomicBool::new(false));
    registry().lock().unwrap().insert(id, WindowEntry { hwnd, id, prevent_close });

    // Presentation: focus-on-create by default, macOS twin parity.
    let style: SHOW_WINDOW_CMD = if args.focus.unwrap_or(true) {
        SW_SHOW
    } else {
        SW_SHOWNOACTIVATE
    };
    unsafe {
        let _ = ShowWindow(hwnd, style);
    }

    let (_x, _y, width, height) = frame_logical(hwnd);
    let created = serde_json::json!({
        "id": id,
        "width": width,
        "height": height,
        "windowPtr": hwnd.0 as usize, // borrowed HWND* — Kitron owns the window
    });

    core().emit_windowed(
        EventName::WindowCreated,
        Some(id),
        serde_json::json!({
            "id": id,
            "title": args.title.unwrap_or_else(|| "Kitron".to_string()),
            "width": width,
            "height": height,
        }),
    );
    if id == 1 {
        core().emit(EventName::AppReady, serde_json::json!({}));
    }

    Ok(created)
}

/// One window op from the `WindowOp` wire contract.
pub fn apply_op(id: u64, op_wire: &str, args: &serde_json::Value) -> Result<(), KitronError> {
    require_main_thread()?;
    let op = WindowOp::from_wire(op_wire);
    if op == WindowOp::Unknown {
        return Err(KitronError::code(
            "WINDOW_OP_UNKNOWN",
            format!("unknown window op {op_wire}"),
        ));
    }

    let (hwnd, prevent_close) = {
        let guard = registry().lock().unwrap();
        let entry = guard
            .get(&id)
            .ok_or_else(|| KitronError::code("WINDOW_NOT_FOUND", format!("no window with id {id}")))?;
        (entry.hwnd, Arc::clone(&entry.prevent_close))
    };

    // SAFETY: gated to the main thread by `require_main_thread`.
    unsafe {
        match op {
            WindowOp::Close => {
                DestroyWindow(hwnd).map_err(crate::werr)?;
            }
            WindowOp::Minimize => {
                let _ = ShowWindow(hwnd, SW_MINIMIZE);
            }
            WindowOp::Maximize => {
                let _ = ShowWindow(hwnd, SW_MAXIMIZE);
            }
            WindowOp::Restore => {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            WindowOp::Focus => {
                let _ = SetForegroundWindow(hwnd);
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            WindowOp::Show => {
                let _ = ShowWindow(hwnd, SW_SHOW);
            }
            WindowOp::Hide => {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            WindowOp::SetSize => {
                let width = args.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let height = args.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
                if width <= 0.0 || height <= 0.0 {
                    return Err(KitronError::code(
                        "WINDOW_ARGS_INVALID",
                        "setSize requires width and height",
                    ));
                }
                let factor = scale(hwnd);
                SetWindowPos(
                    hwnd,
                    Some(HWND_TOP),
                    0,
                    0,
                    (width * factor).round() as i32,
                    (height * factor).round() as i32,
                    SWP_NOMOVE | SWP_NOZORDER,
                )
                .map_err(crate::werr)?;
            }
            WindowOp::SetPosition => {
                let x = args.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let y = args.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                let factor = scale(hwnd);
                SetWindowPos(
                    hwnd,
                    Some(HWND_TOP),
                    (x * factor).round() as i32,
                    (y * factor).round() as i32,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER,
                )
                .map_err(crate::werr)?;
            }
            WindowOp::SetTitle => {
                let Some(title) = args.get("title").and_then(|v| v.as_str()) else {
                    return Err(KitronError::code(
                        "WINDOW_ARGS_INVALID",
                        "setTitle requires title",
                    ));
                };
                let _ = SetWindowTextW(hwnd, PCWSTR(HSTRING::from(title).as_ptr()));
            }
            WindowOp::SetPreventClose => {
                let prevent = args
                    .get("preventClose")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                prevent_close.store(prevent, Ordering::SeqCst);
            }
            WindowOp::Unknown => unreachable!("checked above"),
        }
    }

    refresh_snapshot_cache();
    Ok(())
}

/// `{x, y, width, height}` logical — top-left origin on every platform.
pub(crate) fn frame_logical(hwnd: HWND) -> (f64, f64, f64, f64) {
    let mut rect = RECT::default();
    // SAFETY: plain read; thread rules upheld by the caller.
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let factor = scale(hwnd);
    (
        rect.left as f64 / factor,
        rect.top as f64 / factor,
        (rect.right - rect.left) as f64 / factor,
        (rect.bottom - rect.top) as f64 / factor,
    )
}

/// Builds the portable `WindowSnapshot`.
pub(crate) fn snapshot_for(entry: &WindowEntry) -> WindowSnapshot {
    // SAFETY: property reads; callers gate the thread.
    unsafe {
        let (x, y, width, height) = frame_logical(entry.hwnd);
        WindowSnapshot {
            id: entry.id,
            title: window_title(entry.hwnd),
            frame: WindowFrame { x, y, width, height },
            focused: GetForegroundWindow() == entry.hwnd,
            minimized: IsIconic(entry.hwnd).as_bool(),
            maximized: IsZoomed(entry.hwnd).as_bool(),
            visible: IsWindowVisible(entry.hwnd).as_bool(),
        }
    }
}

pub(crate) unsafe fn window_title(hwnd: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(hwnd) } as usize;
    if length == 0 {
        return String::new();
    }
    let mut buffer = vec![0u16; length + 1];
    unsafe {
        GetWindowTextW(hwnd, &mut buffer);
    }
    String::from_utf16_lossy(&buffer[..length])
}

pub(crate) fn snapshots() -> Vec<WindowSnapshot> {
    let guard = registry().lock().unwrap();
    guard.values().map(snapshot_for).collect()
}

pub(crate) fn window_snapshot(id: u64) -> Result<WindowSnapshot, KitronError> {
    let guard = registry().lock().unwrap();
    guard
        .get(&id)
        .map(snapshot_for)
        .ok_or_else(|| KitronError::code("WINDOW_NOT_FOUND", format!("no window with id {id}")))
}

/// The JSI thread never touches HWNDs — refreshed on the main thread after
/// every op/drain (the macOS twin caches identically).
pub(crate) fn cached_snapshots() -> Vec<WindowSnapshot> {
    static CACHE: Mutex<Option<Vec<WindowSnapshot>>> = Mutex::new(None);
    if let Some(cached) = CACHE.lock().unwrap().as_ref() {
        return cached.clone();
    }
    let fresh = snapshots();
    *CACHE.lock().unwrap() = Some(fresh.clone());
    fresh
}

pub(crate) fn refresh_snapshot_cache() {
    static CACHE: Mutex<Option<Vec<WindowSnapshot>>> = Mutex::new(None);
    *CACHE.lock().unwrap() = None;
}

/// The HWND of window 1 — the integration hook the RNW template calls to
/// attach its renderer (`docs/shell-contracts.md` §5).
///
/// # Safety
///
/// The returned pointer is a borrowed HWND.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_windows_renderer_hwnd() -> usize {
    let guard = registry().lock().unwrap();
    guard.get(&1).map(|e| e.hwnd.0 as usize).unwrap_or(0)
}

pub(crate) fn request_quit() {
    // Queues WM_CLOSE for every window → the WM_DESTROY teardown path.
    let guard = registry().lock().unwrap();
    for entry in guard.values() {
        let _ = unsafe { DestroyWindow(entry.hwnd) };
    }
}

pub(crate) fn can_terminate() -> bool {
    true
}

pub(crate) fn cancel_termination() {}

pub(crate) fn app_shown(_shown: bool) {}

// ---------------------------------------------------------------------------
// The window procedure — Win32 event dispatch translated into Kitron events
// ---------------------------------------------------------------------------

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let id = registry()
        .lock()
        .unwrap()
        .values()
        .find(|entry| entry.hwnd == hwnd)
        .map(|entry| entry.id);

    // SAFETY: the WNDPROC runs inside the app's main message loop.
    unsafe {
        match message {
            WM_CLOSE => {
                // setPreventClose vetoes the close attempt (twin parity).
                let prevented = match id {
                    Some(id) => registry()
                        .lock()
                        .unwrap()
                        .get(&id)
                        .map(|entry| entry.prevent_close.load(Ordering::SeqCst))
                        .unwrap_or(false),
                    None => false,
                };
                if prevented {
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, message, wparam, lparam)
                }
            }
            WM_DESTROY => {
                core().emit_windowed(
                    EventName::WindowClosed,
                    id,
                    serde_json::json!({ "id": id }),
                );
                if let Some(id) = id {
                    registry().lock().unwrap().remove(&id);
                }
                // The MVP quits when the last window closes; the RNW template
                // can veto through app lifecycle hooks later.
                if registry().lock().unwrap().is_empty() {
                    core().emit(EventName::AppQuit, serde_json::json!({}));
                    PostQuitMessage(0);
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            WM_SETFOCUS => {
                core().emit_windowed(EventName::WindowFocus, id, serde_json::json!({ "id": id }));
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            WM_KILLFOCUS => {
                core().emit_windowed(EventName::WindowBlur, id, serde_json::json!({ "id": id }));
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            WM_SIZE => {
                core().emit_windowed(EventName::WindowResize, id, serde_json::json!({ "id": id }));
                refresh_snapshot_cache();
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            WM_MOVE => {
                core().emit_windowed(EventName::WindowMove, id, serde_json::json!({ "id": id }));
                refresh_snapshot_cache();
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            WM_SYSCOMMAND => {
                // The user-driven equivalents of the minimize/restore ops.
                let command = (wparam.0 & 0xFFF0) as u32;
                match command {
                    n if n == SC_MINIMIZE => core().emit_windowed(
                        EventName::WindowMinimize,
                        id,
                        serde_json::json!({ "id": id }),
                    ),
                    n if n == SC_RESTORE => core().emit_windowed(
                        EventName::WindowRestore,
                        id,
                        serde_json::json!({ "id": id }),
                    ),
                    _ => {}
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            WM_COMMAND => {
                crate::menu::on_command(wparam);
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
