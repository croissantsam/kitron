//! Global shortcuts registry (§19).
//!
//! MVP: Kitron validates accelerators, owns registration state, and routes
//! `shortcut.<accelerator>` press events to JS. Binding the OS-level
//! hotkey (RegisterHotKey / CGEventTap) is Phase 4; the `simulatePress`
//! path (used by the CLI + tests) exercises the same routing.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum ShortcutError {
    #[error("shortcut registration failed: {0}")]
    RegisterFailed(String),
}

/// Normalize `CommandOrControl+Shift+K` → canonical key for dedupe + events.
/// Returns Err on empty / blank-only input.
pub fn normalize(accelerator: &str) -> Result<String, ShortcutError> {
    let parts: Vec<String> = accelerator
        .split('+')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return Err(ShortcutError::RegisterFailed("accelerator must not be empty".into()));
    }
    // Last part is the key: uppercase single chars for `k` == `K` dedupe.
    let mut parts = parts;
    if let Some(last) = parts.last_mut() {
        if last.len() == 1 {
            *last = last.to_uppercase();
        }
    }
    Ok(parts.join("+"))
}

pub struct ShortcutManager {
    shortcuts: Mutex<HashMap<String, ()>>,
}

impl ShortcutManager {
    pub fn new() -> Self {
        Self { shortcuts: Mutex::new(HashMap::new()) }
    }

    pub fn register(&self, accelerator: &str) -> Result<String, ShortcutError> {
        let key = normalize(accelerator)?;
        let mut g = self.shortcuts.lock().unwrap();
        if g.contains_key(&key) {
            return Err(ShortcutError::RegisterFailed(format!("already registered: {key}")));
        }
        g.insert(key.clone(), ());
        Ok(key)
    }

    pub fn unregister(&self, accelerator: &str) -> Result<(), ShortcutError> {
        let key = normalize(accelerator)?;
        self.shortcuts
            .lock()
            .unwrap()
            .remove(&key)
            .map(|_| ())
            .ok_or_else(|| ShortcutError::RegisterFailed(format!("not registered: {key}")))
    }

    pub fn is_registered(&self, accelerator: &str) -> bool {
        normalize(accelerator).map(|k| self.shortcuts.lock().unwrap().contains_key(&k)).unwrap_or(false)
    }

    pub fn list(&self) -> Vec<String> {
        self.shortcuts.lock().unwrap().keys().cloned().collect()
    }
}

impl Default for ShortcutManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_dedupe_case_insensitive_key() {
        let m = ShortcutManager::new();
        let a = m.register("CommandOrControl+Shift+k").unwrap();
        assert_eq!(a, "CommandOrControl+Shift+K");
        assert!(m.register("CommandOrControl+Shift+K").is_err());
        assert!(m.is_registered("CommandOrControl+Shift+K"));
        m.unregister("CommandOrControl+Shift+K").unwrap();
        assert!(!m.is_registered("CommandOrControl+Shift+K"));
    }

    #[test]
    fn rejects_blank() {
        let m = ShortcutManager::new();
        assert!(m.register("  ").is_err());
        assert!(m.unregister("never-registered").is_err());
    }
}
