//! Native window registry (§16, §34).
//!
//! The host owns window state. Rendering itself is delegated to
//! `webview.rs` through the [`crate::webview::WebView`] abstraction so the
//! rest of the bridge never depends on WKWebView / WebView2 / WebKitGTK
//! directly (§8).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowOptions {
    #[serde(default = "d1200")]
    pub width: u32,
    #[serde(default = "d800")]
    pub height: u32,
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default = "t")]
    pub center: bool,
    #[serde(default = "t")]
    pub resizable: bool,
    #[serde(default)]
    pub always_on_top: bool,
    #[serde(default = "t")]
    pub show: bool,
    #[serde(default = "t")]
    pub frame: bool,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default)]
    pub devtools: bool,
}

fn d1200() -> u32 {
    1200
}
fn d800() -> u32 {
    800
}
fn t() -> bool {
    true
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            width: 1200,
            height: 800,
            x: None,
            y: None,
            title: None,
            url: None,
            file: None,
            center: true,
            resizable: true,
            always_on_top: false,
            show: true,
            frame: true,
            fullscreen: false,
            devtools: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct WindowState {
    pub id: String,
    pub options: WindowOptions,
    pub visible: bool,
    pub maximized: bool,
    pub minimized: bool,
    pub fullscreen: bool,
}

#[derive(Default)]
pub struct WindowManager {
    windows: HashMap<String, WindowState>,
}

impl WindowManager {
    pub fn create(&mut self, id: String, options: WindowOptions) -> &WindowState {
        let visible = options.show;
        self.windows.insert(
            id.clone(),
            WindowState {
                id,
                options,
                visible,
                maximized: false,
                minimized: false,
                fullscreen: false,
            },
        );
        self.windows.values().last().expect("just inserted")
    }

    pub fn get(&self, id: &str) -> Option<&WindowState> {
        self.windows.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut WindowState> {
        self.windows.get_mut(id)
    }

    pub fn remove(&mut self, id: &str) -> bool {
        self.windows.remove(id).is_some()
    }

    pub fn count(&self) -> usize {
        self.windows.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_update() {
        let mut m = WindowManager::default();
        m.create("w1".into(), WindowOptions {
            title: Some("Hi".into()),
            ..Default::default()
        });
        assert_eq!(m.count(), 1);
        let w = m.get_mut("w1").unwrap();
        w.options.title = Some("Yo".into());
        assert_eq!(m.get("w1").unwrap().options.title.as_deref(), Some("Yo"));
        assert!(m.remove("w1"));
        assert_eq!(m.count(), 0);
    }
}
