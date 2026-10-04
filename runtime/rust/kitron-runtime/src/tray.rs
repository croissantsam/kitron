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
}

impl TrayManager {
    pub fn new() -> Self {
        Self { trays: Mutex::new(HashMap::new()) }
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
