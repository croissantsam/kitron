//! Native menu registry (§18).
//!
//! MVP: Kitron validates + stores menu trees and routes `menu.action`
//! events (`{ menuId, action }`) to JS. Building the `NSMenu` / Win32
//! HMENU from the tree is Phase 4 OS work.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuItem {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub separator: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<MenuItem>>,
}

impl MenuItem {
    fn validate(&self) -> Result<(), String> {
        if self.separator == Some(true) {
            return Ok(());
        }
        if let Some(items) = &self.items {
            if self.label.as_ref().map(|l| !l.trim().is_empty()).unwrap_or(false) {
                for i in items {
                    i.validate()?;
                }
                return Ok(());
            }
            return Err("menu item with children needs a label".into());
        }
        if self.label.as_ref().map(|l| !l.trim().is_empty()).unwrap_or(false) {
            return Ok(());
        }
        Err("menu item needs a label or separator:true".into())
    }

    /// Collect all action ids in the subtree.
    fn actions(&self, out: &mut Vec<String>) {
        if let Some(a) = &self.action {
            out.push(a.clone());
        }
        if let Some(items) = &self.items {
            for i in items {
                i.actions(out);
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MenuError {
    #[error("menu creation failed: {0}")]
    CreationFailed(String),
    #[error("menu not found: {0}")]
    NotFound(String),
}

pub struct MenuManager {
    menus: Mutex<HashMap<String, Vec<MenuItem>>>,
    app_menu: Mutex<Option<String>>,
    /// muda `MenuId.0` → (menu_id, action) for click routing.
    os_actions: Mutex<HashMap<String, (String, String)>>,
    os_bound: std::sync::atomic::AtomicBool,
}

impl MenuManager {
    pub fn new() -> Self {
        Self {
            menus: Mutex::new(HashMap::new()),
            app_menu: Mutex::new(None),
            os_actions: Mutex::new(HashMap::new()),
            os_bound: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn create(&self, items: Vec<MenuItem>) -> Result<String, MenuError> {
        if items.is_empty() {
            return Err(MenuError::CreationFailed("menu.create: items must not be empty".into()));
        }
        for i in &items {
            i.validate().map_err(MenuError::CreationFailed)?;
        }
        let id = Uuid::new_v4().to_string();
        self.menus.lock().unwrap().insert(id.clone(), items);
        Ok(id)
    }

    pub fn set_app_menu(&self, id: &str) -> Result<(), MenuError> {
        if !self.menus.lock().unwrap().contains_key(id) {
            return Err(MenuError::NotFound(id.into()));
        }
        *self.app_menu.lock().unwrap() = Some(id.into());
        Ok(())
    }

    pub fn app_menu(&self) -> Option<String> {
        self.app_menu.lock().unwrap().clone()
    }

    /// Returns true when `action` exists in menu `id` (click routing guard).
    pub fn has_action(&self, id: &str, action: &str) -> bool {
        let g = self.menus.lock().unwrap();
        match g.get(id) {
            Some(items) => {
                let mut all = Vec::new();
                for i in items {
                    i.actions(&mut all);
                }
                all.iter().any(|a| a == action)
            }
            None => false,
        }
    }
}

impl Default for MenuManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Separator between menu id and action inside a muda `MenuId`.
const ID_SEP: char = '\u{1f}';

/// Build the real OS app menu (`NSMenu` on macOS, `HMENU` on Windows)
/// from the registered `app_menu` tree. Must run on the main thread.
/// The muda menu is intentionally leaked (process lifetime). Activations
/// are routed to `menu.action { menuId, action }` — the same events the
/// `menu.click` dispatch hook emits.
///
/// `hwnd` is the Win32 window handle for `init_for_hwnd`; `None` elsewhere.
pub fn bind_os(
    mgr: &std::sync::Arc<MenuManager>,
    app: &std::sync::Arc<crate::app::AppManager>,
    hwnd: Option<isize>,
) {
    use muda::{IsMenuItem, Menu, MenuId, MenuItem as OsItem, PredefinedMenuItem, Submenu};

    let app_menu_id = mgr.app_menu();
    let menu_id = match app_menu_id {
        Some(id) => id,
        None => return,
    };
    #[cfg(target_os = "windows")]
    if hwnd.is_none() {
        // The Win32 menu needs the window handle, which only exists after
        // the first window is created. run_windowed re-calls bind_os then.
        return;
    }
    #[cfg(not(target_os = "windows"))]
    let _ = hwnd;
    if mgr.os_bound.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let items = match mgr.menus.lock().unwrap().get(&menu_id).cloned() {
        Some(items) => items,
        None => return,
    };

    fn append_all(
        menu_id: &str,
        items: &[MenuItem],
        actions: &mut HashMap<String, (String, String)>,
        append: &dyn Fn(&dyn IsMenuItem),
    ) {
        for it in items {
            if it.separator == Some(true) {
                let sep = PredefinedMenuItem::separator();
                append(&sep);
                continue;
            }
            let label = it.label.clone().unwrap_or_default();
            if let Some(children) = &it.items {
                let sub = Submenu::new(label, it.enabled.unwrap_or(true));
                append_all(menu_id, children, actions, &|kind| {
                    let _ = sub.append(kind);
                });
                append(&sub);
            } else {
                let key = format!("{menu_id}{ID_SEP}{}", it.action.clone().unwrap_or_default());
                actions
                    .insert(key.clone(), (menu_id.to_string(), it.action.clone().unwrap_or_default()));
                let item = OsItem::with_id(MenuId(key), label, it.enabled.unwrap_or(true), None);
                append(&item);
            }
        }
    }

    let mut actions = HashMap::new();
    let menu = Menu::new();
    append_all(&menu_id, &items, &mut actions, &|kind| {
        let _ = menu.append(kind);
    });
    *mgr.os_actions.lock().unwrap() = actions;
    #[cfg(target_os = "macos")]
    menu.init_for_nsapp();
    #[cfg(target_os = "windows")]
    if let Some(hwnd) = hwnd {
        menu.init_for_hwnd(hwnd);
    }
    std::mem::forget(menu);

    let mgr = mgr.clone();
    let app = app.clone();
    std::thread::spawn(move || {
        let rx = muda::MenuEvent::receiver();
        while let Ok(ev) = rx.recv() {
            let key = ev.id.0;
            if let Some((mid, action)) = mgr.os_actions.lock().unwrap().get(&key).cloned() {
                if !action.is_empty() {
                    app.emit("menu.action", serde_json::json!({ "menuId": mid, "action": action }));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(label: &str, action: &str) -> MenuItem {
        MenuItem {
            label: Some(label.into()),
            action: Some(action.into()),
            checked: None,
            enabled: None,
            separator: None,
            items: None,
        }
    }

    #[test]
    fn create_and_route() {
        let m = MenuManager::new();
        let id = m
            .create(vec![MenuItem {
                label: Some("File".into()),
                action: None,
                checked: None,
                enabled: None,
                separator: None,
                items: Some(vec![item("New", "new-file")]),
            }])
            .unwrap();
        assert!(m.has_action(&id, "new-file"));
        assert!(!m.has_action(&id, "nope"));
        m.set_app_menu(&id).unwrap();
        assert_eq!(m.app_menu().unwrap(), id);
    }

    #[test]
    fn rejects_empty_and_labelless() {
        let m = MenuManager::new();
        assert!(m.create(vec![]).is_err());
        assert!(
            m.create(vec![MenuItem {
                label: None,
                action: Some("x".into()),
                checked: None,
                enabled: None,
                separator: None,
                items: None,
            }])
            .is_err()
        );
    }
}
