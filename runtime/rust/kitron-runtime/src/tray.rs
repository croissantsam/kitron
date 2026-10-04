//! Tray registry (§17).
//!
//! MVP: Kitron owns tray identity (id/icon/tooltip) and click routing.
//! Rendering the `NSStatusItem` / Win32 notify-icon is Phase 4 OS work;
//! the registry + `tray.clicked { id }` events already let apps and tests
//! drive the full flow through the typed TS facade.

use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct TrayEntry {
    pub id: String,
    pub icon: String,
    pub tooltip: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum TrayError {
    #[error("tray creation failed: {0}")]
    CreationFailed(String),
    #[error("tray not found: {0}")]
    NotFound(String),
}

pub struct TrayManager {
    trays: Mutex<HashMap<String, TrayEntry>>,
    os_bound: std::sync::atomic::AtomicBool,
}

impl TrayManager {
    pub fn new() -> Self {
        Self { trays: Mutex::new(HashMap::new()), os_bound: std::sync::atomic::AtomicBool::new(false) }
    }

    pub fn create(&self, icon: String, tooltip: Option<String>) -> Result<TrayEntry, TrayError> {
        if icon.trim().is_empty() {
            return Err(TrayError::CreationFailed("tray.create: icon path is required".into()));
        }
        let id = Uuid::new_v4().to_string();
        let entry = TrayEntry { id: id.clone(), icon, tooltip };
        self.trays.lock().unwrap().insert(id, entry.clone());
        Ok(entry)
    }

    pub fn destroy(&self, id: &str) -> Result<(), TrayError> {
        self.trays
            .lock()
            .unwrap()
            .remove(id)
            .map(|_| ())
            .ok_or_else(|| TrayError::NotFound(id.into()))
    }

    pub fn get(&self, id: &str) -> Option<TrayEntry> {
        self.trays.lock().unwrap().get(id).cloned()
    }

    pub fn list(&self) -> Vec<TrayEntry> {
        self.trays.lock().unwrap().values().cloned().collect()
    }
}

impl Default for TrayManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Build real OS tray icons (`NSStatusItem` on macOS, notify icon on
/// Windows) for every registered tray. Must run on the main thread.
/// Clicks are routed to `tray.clicked { id }` — the same events the
/// `tray.click` dispatch hook emits, so headless and windowed behave alike.
///
/// Icon handles are intentionally leaked: a tray icon lives for the whole
/// process lifetime, and leaking avoids `Send` bounds on the registry.
pub fn bind_os(
    mgr: &std::sync::Arc<TrayManager>,
    app: &std::sync::Arc<crate::app::AppManager>,
) {
    use tray_icon::{TrayIconBuilder, TrayIconId};

    if mgr.os_bound.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let entries = mgr.list();
    if entries.is_empty() {
        return;
    }
    for e in entries {
        let icon = load_icon(&e.icon);
        let builder = TrayIconBuilder::new()
            .with_id(TrayIconId(e.id.clone()))
            .with_icon(icon);
        let builder = match &e.tooltip {
            Some(t) => builder.with_tooltip(t),
            None => builder,
        };
        match builder.build() {
            Ok(handle) => std::mem::forget(handle),
            Err(err) => tracing::warn!(tray_id = %e.id, error = %err, "OS tray icon failed"),
        }
    }
    let app = app.clone();
    std::thread::spawn(move || {
        use tray_icon::{MouseButton, TrayIconEvent};
        let rx = TrayIconEvent::receiver();
        while let Ok(ev) = rx.recv() {
            let id = match &ev {
                TrayIconEvent::Click { id, button: MouseButton::Left | MouseButton::Right, .. } => Some(id.0.clone()),
                TrayIconEvent::DoubleClick { id, .. } => Some(id.0.clone()),
                _ => None,
            };
            if let Some(id) = id {
                app.emit("tray.clicked", serde_json::json!({ "id": id }));
            }
        }
    });
}

/// Jennings-style fallback so a missing icon file never breaks the tray.
fn load_icon(path: &str) -> tray_icon::Icon {
    if let Ok(img) = image::open(path) {
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width(), rgba.height());
        if let Ok(icon) = tray_icon::Icon::from_rgba(rgba.into_raw(), w, h) {
            return icon;
        }
    }
    // 16x16 solid placeholder.
    let px = vec![255u8; 16 * 16 * 4];
    tray_icon::Icon::from_rgba(px, 16, 16).expect("placeholder icon")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_destroy_roundtrip() {
        let m = TrayManager::new();
        let e = m.create("./icon.png".into(), Some("My App".into())).unwrap();
        assert!(m.get(&e.id).is_some());
        assert_eq!(m.list().len(), 1);
        m.destroy(&e.id).unwrap();
        assert!(m.get(&e.id).is_none());
    }

    #[test]
    fn empty_icon_rejected() {
        let m = TrayManager::new();
        assert!(matches!(m.create("  ".into(), None), Err(TrayError::CreationFailed(_))));
    }

    #[test]
    fn destroy_unknown() {
        let m = TrayManager::new();
        assert!(matches!(m.destroy("nope"), Err(TrayError::NotFound(_))));
    }
}
