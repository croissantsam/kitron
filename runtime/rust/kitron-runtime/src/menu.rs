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
}

impl MenuManager {
    pub fn new() -> Self {
        Self { menus: Mutex::new(HashMap::new()), app_menu: Mutex::new(None) }
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
