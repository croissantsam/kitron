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
    os: Mutex<Option<OsState>>,
    os_bound: std::sync::atomic::AtomicBool,
}

struct OsState {
    manager: global_hotkey::GlobalHotKeyManager,
    /// global-hotkey id → canonical accelerator for event routing.
    ids: HashMap<u32, String>,
    /// canonical accelerator → registered hotkey for unregister.
    by_key: HashMap<String, global_hotkey::hotkey::HotKey>,
}

impl ShortcutManager {
    pub fn new() -> Self {
        Self {
            shortcuts: Mutex::new(HashMap::new()),
            os: Mutex::new(None),
            os_bound: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn register(&self, accelerator: &str) -> Result<String, ShortcutError> {
        let key = normalize(accelerator)?;
        {
            let mut g = self.shortcuts.lock().unwrap();
            if g.contains_key(&key) {
                return Err(ShortcutError::RegisterFailed(format!("already registered: {key}")));
            }
            g.insert(key.clone(), ());
        }
        // Bind the OS hotkey when the OS layer is up; roll back on failure
        // so registry and OS state never diverge.
        {
            let mut guard = self.os.lock().unwrap();
            if let Some(os) = guard.as_mut() {
                match parse_hotkey(&key).and_then(|hk| {
                    let id = hk.id();
                    os.manager
                        .register(hk)
                        .map(|_| (id, hk))
                        .map_err(|e| ShortcutError::RegisterFailed(e.to_string()))
                }) {
                    Ok((id, hk)) => {
                        os.by_key.insert(key.clone(), hk);
                        os.ids.insert(id, key.clone());
                    }
                    Err(e) => {
                        self.shortcuts.lock().unwrap().remove(&key);
                        return Err(e);
                    }
                }
            }
        }
        Ok(key)
    }

    pub fn unregister(&self, accelerator: &str) -> Result<(), ShortcutError> {
        let key = normalize(accelerator)?;
        self.shortcuts
            .lock()
            .unwrap()
            .remove(&key)
            .ok_or_else(|| ShortcutError::RegisterFailed(format!("not registered: {key}")))?;
        let mut guard = self.os.lock().unwrap();
        if let Some(os) = guard.as_mut() {
            if let Some(hk) = os.by_key.remove(&key) {
                os.ids.remove(&hk.id());
                let _ = os.manager.unregister(hk);
            }
        }
        Ok(())
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

/// Bring up OS-level hotkeys (RegisterHotKey / CGEventTap via
/// `global-hotkey`) for every registered accelerator and route presses to
/// `shortcut.<canonical>` — the same events `shortcut.press` emits.
pub fn bind_os(
    mgr: &std::sync::Arc<ShortcutManager>,
    app: &std::sync::Arc<crate::app::AppManager>,
) {
    use global_hotkey::GlobalHotKeyManager;

    if mgr.os_bound.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let keys: Vec<String> = mgr.shortcuts.lock().unwrap().keys().cloned().collect();
    let manager = match GlobalHotKeyManager::new() {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(error = %e, "OS hotkey manager unavailable");
            return;
        }
    };
    let mut os = OsState { manager, ids: HashMap::new(), by_key: HashMap::new() };
    for key in keys {
        match parse_hotkey(&key).and_then(|hk| {
            let id = hk.id();
            os.manager
                .register(hk)
                .map(|_| (id, hk))
                .map_err(|e| ShortcutError::RegisterFailed(e.to_string()))
        }) {
            Ok((id, hk)) => {
                os.by_key.insert(key.clone(), hk);
                os.ids.insert(id, key);
            }
            Err(e) => tracing::warn!(accelerator = %key, error = %e, "OS hotkey bind failed"),
        }
    }
    *mgr.os.lock().unwrap() = Some(os);

    let mgr = mgr.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        let rx = global_hotkey::GlobalHotKeyEvent::receiver();
        while let Ok(ev) = rx.recv() {
            if let Some(key) = mgr.os.lock().unwrap().as_ref().and_then(|os| os.ids.get(&ev.id).cloned()) {
                app.emit(&format!("shortcut.{key}"), serde_json::Value::Null);
            }
        }
    });
}

fn parse_hotkey(canonical: &str) -> Result<global_hotkey::hotkey::HotKey, ShortcutError> {
    use std::str::FromStr;
    // global-hotkey parses case-insensitively and maps COMMANDORCONTROL to
    // Command (macOS) / Control (elsewhere) — matching our canonical form.
    global_hotkey::hotkey::HotKey::from_str(canonical)
        .map_err(|e| ShortcutError::RegisterFailed(format!("{canonical}: {e}")))
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
