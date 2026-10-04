//! Main-thread OS binding requests.
//!
//! Tray icons (`NSStatusItem`), app menus (`NSMenu`) and global hotkeys
//! must be created on the main thread. Dispatch runs on tokio workers, so
//! registrations request a bind through this channel instead: the winit
//! event loop (main thread) owns the only consumer and calls
//! [`Runtime::bind_os`](crate::ipc_server::Runtime::bind_os).
//!
//! Headless mode and tests never set the proxy — requests are no-ops there
//! and the registry + `*.click`/`press` hooks keep working.

use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KitronUserEvent {
    BindOs,
    /// Reconcile OS windows with the registry (open pending, drop closed).
    SyncWindows,
}

type Proxy = winit::event_loop::EventLoopProxy<KitronUserEvent>;

static PROXY: OnceLock<Proxy> = OnceLock::new();

/// Called once by the host binary when the winit event loop exists.
pub fn set_proxy(proxy: Proxy) {
    let _ = PROXY.set(proxy);
}

/// Ask the main thread to (re)bind OS tray/menu/hotkeys. Cheap and
/// idempotent — managers ignore repeat binds via once-guards.
pub fn request_os_bind() {
    send(KitronUserEvent::BindOs);
}

/// Ask the main thread to open OS windows for pending headless records and
/// drop locals whose records were closed.
pub fn request_sync_windows() {
    send(KitronUserEvent::SyncWindows);
}

fn send(ev: KitronUserEvent) {
    if let Some(p) = PROXY.get() {
        let _ = p.send_event(ev);
    }
}
