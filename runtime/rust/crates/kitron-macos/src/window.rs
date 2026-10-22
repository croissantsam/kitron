//! Native window management on AppKit (spec §10).
//!
//! Windows live on the main thread (AppKit requirement). The registry maps
//! Kitron window ids to `Retained<NSWindow>` entries plus their delegate
//! handles. Delegates forward AppKit events into the core event bus. Frames
//! exposed to JS are in top-left screen coordinates; AppKit's bottom-left
//! origin never leaks.


use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use kitron_core::EventName;
use kitron_protocol::KitronError;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{define_class, DefinedClass, MainThreadMarker, MainThreadOnly, msg_send};
use objc2_app_kit::{
    NSApplication, NSBackingStoreType, NSWindow, NSWindowDelegate, NSWindowStyleMask,
};
use objc2_foundation::{
    NSNotification, NSPoint, NSObject, NSObjectProtocol, NSRect, NSSize, NSString,
};
use serde::{Deserialize, Serialize};

use crate::{core, host};

// ---------------------------------------------------------------------------
// Shapes
// ---------------------------------------------------------------------------

/// Window state as exposed to JS (spec §10 `window.get`).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowSnapshot {
    pub id: u64,
    pub title: String,
    pub frame: WindowFrame,
    pub focused: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub visible: bool,
}

/// Frame in top-left screen coordinates.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowCreateArgs {
    title: Option<String>,
    width: Option<f64>,
    height: Option<f64>,
    focus: Option<bool>,
    resizable: Option<bool>,
    minimizable: Option<bool>,
    closable: Option<bool>,
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

pub struct WindowEntry {
    pub id: u64,
    pub window: Retained<NSWindow>,
    /// Keeps the delegate alive while registered.
    #[allow(dead_code)]
    delegate_alive: Retained<ProtocolObject<dyn NSWindowDelegate>>,
    pub closing: Arc<AtomicBool>,
    pub prevent_close: Arc<AtomicBool>,
}

// SAFETY: AppKit object handles are only ever dereferenced on the main thread
// (window ops, creation, and snapshot refreshes); worker threads treat the
// entry as opaque and only access `Arc<AtomicBool>` flags. Static access
// (`OnceLock<MacosHost>`) requires Send+Sync, so we assert them here with this
// invariant upheld everywhere in the crate.
unsafe impl Send for WindowEntry {}
unsafe impl Sync for WindowEntry {}

pub(crate) fn get_handle(id: u64) -> Option<(Retained<NSWindow>, Arc<AtomicBool>, Arc<AtomicBool>)> {
    let windows = host().windows.lock().ok()?;
    windows.get(&id).map(|entry| {
        (entry.window.clone(), entry.closing.clone(), entry.prevent_close.clone())
    })
}

// ---------------------------------------------------------------------------
// Delegate
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub(crate) struct WindowDelegateIvars {
    window_id: u64,
}

define_class!(
    /// NSWindowDelegate shim for Kitron windows: forwards AppKit events into
    /// the core event bus. Runs exclusively on the main thread.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = WindowDelegateIvars]
    pub(super) struct WindowDelegate;

    unsafe impl NSObjectProtocol for WindowDelegate {}

    unsafe impl NSWindowDelegate for WindowDelegate {
        #[unsafe(method(windowShouldClose:))]
        unsafe fn window_should_close(&self, _sender: &NSWindow) -> objc2::runtime::Bool {
            let Some(handle) = get_handle(self.ivars().window_id) else {
                return true.into();
            };
            (!handle.2.load(Ordering::SeqCst)).into()
        }

        #[unsafe(method(windowWillClose:))]
        unsafe fn window_will_close(&self, _notification: &NSNotification) {
            let id = self.ivars().window_id;
            if let Some(handle) = get_handle(id) {
                handle.1.store(true, Ordering::SeqCst);
            }
            emit_window_event(EventName::WindowClosed, id);
        }

        #[unsafe(method(windowDidBecomeKey:))]
        unsafe fn window_did_become_key(&self, _notification: &NSNotification) {
            emit_window_event(EventName::WindowFocus, self.ivars().window_id);
        }

        #[unsafe(method(windowDidResignKey:))]
        unsafe fn window_did_resign_key(&self, _notification: &NSNotification) {
            emit_window_event(EventName::WindowBlur, self.ivars().window_id);
        }

        #[unsafe(method(windowDidResize:))]
        unsafe fn window_did_resize(&self, _notification: &NSNotification) {
            emit_window_event(EventName::WindowResize, self.ivars().window_id);
        }

        #[unsafe(method(windowDidMiniaturize:))]
        unsafe fn window_did_miniaturize(&self, _notification: &NSNotification) {
            emit_window_event(EventName::WindowMinimize, self.ivars().window_id);
        }

        #[unsafe(method(windowDidDeminiaturize:))]
        unsafe fn window_did_deminiaturize(&self, _notification: &NSNotification) {
            emit_window_event(EventName::WindowRestore, self.ivars().window_id);
        }
    }
);

fn emit_window_event(name: EventName, id: u64) {
    core().emit_windowed(name, Some(id), serde_json::json!({ "id": id }));
}

fn new_delegate(mtm: MainThreadMarker, window_id: u64) -> Retained<WindowDelegate> {
    let allocated = WindowDelegate::alloc(mtm).set_ivars(WindowDelegateIvars { window_id });
    unsafe { msg_send![super(allocated), init] }
}

// ---------------------------------------------------------------------------
// Creation
// ---------------------------------------------------------------------------

/// Creates a native window and registers it. Returns `{id, windowPtr}` for
/// the bridge (`windowPtr` is a borrowed `NSWindow*` — Kitron owns it).
pub(crate) fn create_window(args: &serde_json::Value) -> Result<serde_json::Value, KitronError> {
    let parsed: WindowCreateArgs = serde_json::from_value(args.clone())
        .map_err(|e| KitronError::code("WINDOW_ARGS_INVALID", format!("bad window args: {e}")))?;

    let defaults = &core().config.window;
    let width = parsed.width.filter(|w| *w > 0.0).unwrap_or(defaults.width);
    let height = parsed.height.filter(|h| *h > 0.0).unwrap_or(defaults.height);
    let title = parsed
        .title
        .or_else(|| defaults.title.clone())
        .unwrap_or_else(|| core().config.app.name.clone());
    let focus = parsed.focus.unwrap_or(true);

    let mut style = NSWindowStyleMask::Titled;
    if parsed.resizable.unwrap_or(true) {
        style |= NSWindowStyleMask::Resizable;
    }
    if parsed.minimizable.unwrap_or(true) {
        style |= NSWindowStyleMask::Miniaturizable;
    }
    if parsed.closable.unwrap_or(true) {
        style |= NSWindowStyleMask::Closable;
    }

    let content_rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width, height));
    let Some(mtm) = MainThreadMarker::new() else {
        return Err(KitronError::code(
            "THREAD_MAIN_REQUIRED",
            "window creation must happen on the main thread",
        ));
    };
    let window: Retained<NSWindow> = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            content_rect,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };

    window.setTitle(&NSString::from_str(&title));
    unsafe {
        window.setReleasedWhenClosed(false);
    }

    let id = host().next_window_id.fetch_add(1, Ordering::SeqCst);
    let delegate = new_delegate(mtm, id);
    let proto: Retained<ProtocolObject<dyn NSWindowDelegate>> =
        ProtocolObject::from_retained(delegate);
    window.setDelegate(Some(proto.as_ref()));

    host().windows.lock().unwrap().insert(
        id,
        WindowEntry {
            id,
            window: window.clone(),
            delegate_alive: proto,
            closing: Arc::new(AtomicBool::new(false)),
            prevent_close: Arc::new(AtomicBool::new(false)),
        },
    );

    if focus {
        NSApplication::sharedApplication(mtm).activate();
        window.makeKeyAndOrderFront(None);
    } else {
        window.orderFront(None);
    }

    core().emit_windowed(
        EventName::WindowCreated,
        Some(id),
        serde_json::json!({ "id": id, "title": title, "width": width, "height": height }),
    );
    // The app signals ready when the main window exists (spec §11): the JS
    // `app.onReady()` promise and `app.ready` listeners resolve off the pump.
    if id == 1 {
        core().emit(EventName::AppReady, serde_json::json!({}));
    }
    refresh_snapshot_cache();

    Ok(serde_json::json!({
        "id": id,
        "windowPtr": Retained::as_ptr(&window) as usize,
    }))
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

pub(crate) fn apply_op(id: u64, op: &str, args: &serde_json::Value) -> Result<(), KitronError> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Err(KitronError::code(
            "THREAD_MAIN_REQUIRED",
            "window ops must run on the main thread",
        ));
    };
    let Some((window, prevent_close, _closing)) = get_handle(id) else {
        return Err(KitronError::code(
            "WINDOW_NOT_FOUND",
            format!("no window with id {id}"),
        ));
    };

    let app = NSApplication::sharedApplication(mtm);

    match op {
        "close" => window.performClose(None),
        "minimize" => window.miniaturize(None),
        "maximize" | "restore" => {
            if window.isMiniaturized() {
                window.deminiaturize(None);
            } else {
                window.zoom(None);
            }
        }
        "focus" => {
            app.activate();
            window.makeKeyAndOrderFront(None);
        }
        "setSize" => {
            let width = args.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let height = args.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if width <= 0.0 || height <= 0.0 {
                return Err(KitronError::code(
                    "WINDOW_ARGS_INVALID",
                    "setSize requires width and height",
                ));
            }
            window.setContentSize(NSSize::new(width, height));
        }
        "setPosition" => {
            let x = args.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let y = args.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
            window.setFrameTopLeftPoint(NSPoint::new(x, y));
        }
        "show" => window.orderFront(None),
        "hide" => window.orderOut(None),
        "setTitle" => {
            let Some(title) = args.get("title").and_then(|v| v.as_str()) else {
                return Err(KitronError::code(
                    "WINDOW_ARGS_INVALID",
                    "setTitle requires title",
                ));
            };
            window.setTitle(&NSString::from_str(title));
        }
        "setPreventClose" => {
            let flag = args
                .get("preventClose")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            prevent_close.store(flag, Ordering::SeqCst);
        }
        other => {
            return Err(KitronError::code(
                "WINDOW_OP_UNKNOWN",
                format!("unknown window op {other}"),
            ));
        }
    }
    refresh_snapshot_cache();
    Ok(())
}

// ---------------------------------------------------------------------------
// Snapshots
// ---------------------------------------------------------------------------

pub(crate) fn snapshots() -> Vec<WindowSnapshot> {
    let windows = host().windows.lock().unwrap();
    windows
        .values()
        .map(|entry| snapshot_for(&entry.window, entry.id))
        .collect()
}

pub(crate) fn window_snapshot(id: u64) -> Result<WindowSnapshot, KitronError> {
    let windows = host().windows.lock().unwrap();
    match windows.get(&id) {
        Some(entry) => Ok(snapshot_for(&entry.window, id)),
        None => Err(KitronError::code(
            "WINDOW_NOT_FOUND",
            format!("no window with id {id}"),
        )),
    }
}

fn snapshot_for(window: &NSWindow, id: u64) -> WindowSnapshot {
    let rect = window.frame();
    WindowSnapshot {
        id,
        title: window.title().to_string(),
        frame: WindowFrame {
            x: rect.origin.x,
            y: rect.origin.y,
            width: rect.size.width,
            height: rect.size.height,
        },
        focused: window.isKeyWindow(),
        minimized: window.isMiniaturized(),
        maximized: window.isZoomed(),
        visible: window.isVisible(),
    }
}

/// Removes windows whose `windowWillClose` completed (deferred removal keeps
/// the delegate safe while AppKit is inside its callbacks).
pub(crate) fn purge_closing_windows() {
    let mut windows = host().windows.lock().unwrap();
    let closed_ids: Vec<u64> = windows
        .iter()
        .filter(|(_, entry)| entry.closing.load(Ordering::SeqCst))
        .map(|(id, _)| *id)
        .collect();
    for id in closed_ids {
        windows.remove(&id);
    }
    drop(windows);
    refresh_snapshot_cache();
}

// ---------------------------------------------------------------------------
// Snapshot cache (NSWindow is MainThreadOnly; JS-thread sync reads use this)
// ---------------------------------------------------------------------------

static SNAPSHOT_CACHE: std::sync::Mutex<Option<serde_json::Value>> = std::sync::Mutex::new(None);

/// Rebuilds the cached window list. MUST run on the main thread (touches
/// NSWindow snapshot getters). Called after every window op and event drain.
pub(crate) fn refresh_snapshot_cache() {
    if MainThreadMarker::new().is_none() {
        return;
    }
    let value = serde_json::to_value(snapshots()).unwrap_or_default();
    *SNAPSHOT_CACHE.lock().unwrap() = Some(value);
}

/// Thread-safe read used by the JSI host object (works from any thread).
pub(crate) fn cached_snapshots() -> serde_json::Value {
    SNAPSHOT_CACHE
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_else(|| serde_json::json!([]))
}
