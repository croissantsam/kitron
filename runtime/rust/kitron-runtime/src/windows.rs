//! Window manager — real native windows via winit.
//! winit uses AppKit/NSWindow on macOS and Win32 on Windows,
//! so this satisfies "native window" without hand-rolled objc.

use kitron_core::{WindowCreateOptions, WindowInfo};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use winit::window::Window as WinitWindow;

#[derive(Debug, thiserror::Error)]
pub enum WindowError {
    #[error("window creation failed: {0}")]
    CreationFailed(String),
    #[error("window not found: {0}")]
    NotFound(String),
    #[error("window operation failed: {0}")]
    OpFailed(String),
}

pub struct WindowRecord {
    pub info: WindowInfo,
    pub opts: WindowCreateOptions,
    // The live winit window. Kept so the OS window stays alive.
    // `None` in headless/test mode (no event loop yet).
    pub(crate) _handle: Option<Arc<WinitWindow>>,
}

impl WindowRecord {
    fn apply_size(&mut self, w: u32, h: u32) {
        self.info.width = w;
        self.info.height = h;
        if let Some(handle) = &self._handle {
            let _ = handle.request_inner_size(winit::dpi::LogicalSize::new(w, h));
        }
    }
    fn apply_pos(&mut self, x: i32, y: i32) {
        self.info.x = x;
        self.info.y = y;
        if let Some(handle) = &self._handle {
            handle.set_outer_position(winit::dpi::LogicalPosition::new(x, y));
        }
    }
}

#[derive(Default)]
pub struct WindowManager {
    windows: Mutex<HashMap<String, WindowRecord>>,
}

impl WindowManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a window created by the winit event loop (real OS window).
    pub fn register_live(
        &self,
        id: String,
        opts: &WindowCreateOptions,
        handle: Arc<WinitWindow>,
    ) -> WindowInfo {
        let size = handle.inner_size();
        let pos = handle.outer_position().ok();
        let info = WindowInfo {
            id: id.clone(),
            title: opts.title.clone(),
            width: size.width,
            height: size.height,
            x: pos.map(|p| p.x).unwrap_or(opts.x.unwrap_or(100)),
            y: pos.map(|p| p.y).unwrap_or(opts.y.unwrap_or(100)),
            minimized: false,
            maximized: false,
            visible: opts.visible,
            focused: true,
        };
        self.windows.lock().unwrap().insert(
            id,
            WindowRecord { info: info.clone(), opts: opts.clone(), _handle: Some(handle) },
        );
        info
    }

    /// Headless registration (tests, `kitron dev --headless`, CI).
    pub fn create_headless(&self, opts: WindowCreateOptions) -> WindowInfo {
        let id = Uuid::new_v4().to_string();
        let info = WindowInfo {
            id: id.clone(),
            title: opts.title.clone(),
            width: opts.width,
            height: opts.height,
            x: opts.x.unwrap_or(100),
            y: opts.y.unwrap_or(100),
            minimized: false,
            maximized: false,
            visible: opts.visible,
            focused: false,
        };
        self.windows.lock().unwrap().insert(id, WindowRecord { info: info.clone(), opts: opts.clone(), _handle: None });
        info
    }

    pub fn get(&self, id: &str) -> Result<WindowInfo, WindowError> {
        self.windows
            .lock()
            .unwrap()
            .get(id)
            .map(|r| r.info.clone())
            .ok_or_else(|| WindowError::NotFound(id.into()))
    }

    pub fn all(&self) -> Vec<WindowInfo> {
        self.windows.lock().unwrap().values().map(|r| r.info.clone()).collect()
    }

    fn mutate(&self, id: &str, f: impl FnOnce(&mut WindowRecord)) -> Result<WindowInfo, WindowError> {
        let mut g = self.windows.lock().unwrap();
        let rec = g.get_mut(id).ok_or_else(|| WindowError::NotFound(id.into()))?;
        f(rec);
        Ok(rec.info.clone())
    }

    pub fn close(&self, id: &str) -> Result<(), WindowError> {
        let mut g = self.windows.lock().unwrap();
        g.remove(id).map(|_| ()).ok_or_else(|| WindowError::NotFound(id.into()))
    }

    pub fn minimize(&self, id: &str) -> Result<WindowInfo, WindowError> {
        self.mutate(id, |r| {
            r.info.minimized = true;
            if let Some(h) = &r._handle {
                h.set_minimized(true);
            }
        })
    }

    pub fn maximize(&self, id: &str) -> Result<WindowInfo, WindowError> {
        self.mutate(id, |r| {
            r.info.maximized = true;
            r.info.minimized = false;
            if let Some(h) = &r._handle {
                h.set_maximized(true);
            }
        })
    }

    pub fn restore(&self, id: &str) -> Result<WindowInfo, WindowError> {
        self.mutate(id, |r| {
            r.info.maximized = false;
            r.info.minimized = false;
            if let Some(h) = &r._handle {
                h.set_maximized(false);
                h.set_minimized(false);
            }
        })
    }

    pub fn focus(&self, id: &str) -> Result<WindowInfo, WindowError> {
        self.mutate(id, |r| {
            r.info.focused = true;
            if let Some(h) = &r._handle {
                h.focus_window();
            }
        })
    }

    pub fn show(&self, id: &str) -> Result<WindowInfo, WindowError> {
        self.mutate(id, |r| {
            r.info.visible = true;
            if let Some(h) = &r._handle {
                h.set_visible(true);
            }
        })
    }

    pub fn hide(&self, id: &str) -> Result<WindowInfo, WindowError> {
        self.mutate(id, |r| {
            r.info.visible = false;
            if let Some(h) = &r._handle {
                h.set_visible(false);
            }
        })
    }

    pub fn set_size(&self, id: &str, w: u32, h: u32) -> Result<WindowInfo, WindowError> {
        self.mutate(id, |r| r.apply_size(w, h))
    }

    pub fn set_position(&self, id: &str, x: i32, y: i32) -> Result<WindowInfo, WindowError> {
        self.mutate(id, |r| r.apply_pos(x, y))
    }

    pub fn set_title(&self, id: &str, title: String) -> Result<WindowInfo, WindowError> {
        self.mutate(id, |r| {
            r.info.title = title.clone();
            if let Some(h) = &r._handle {
                h.set_title(&title);
            }
        })
    }

    /// Ask the OS window to repaint (best-effort; no-op headless).
    pub fn redraw(&self, id: &str) {
        if let Some(h) = self.windows.lock().unwrap().get(id).and_then(|r| r._handle.clone()) {
            h.request_redraw();
        }
    }

    /// Repaint every live window (used after UI mutations).
    pub fn redraw_all(&self) {
        let handles: Vec<Arc<WinitWindow>> =
            self.windows.lock().unwrap().values().filter_map(|r| r._handle.clone()).collect();
        for h in handles {
            h.request_redraw();
        }
    }

    /// Headless-registered windows with no live OS window yet. The event
    /// loop drains this to open real windows on the main thread.
    pub fn unbound(&self) -> Vec<(String, WindowCreateOptions)> {
        self.windows
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, r)| r._handle.is_none())
            .map(|(id, r)| (id.clone(), r.opts.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_vertical_slice() {
        let wm = WindowManager::new();
        let info = wm.create_headless(WindowCreateOptions {
            width: 1200,
            height: 800,
            title: "My App".into(),
            ..Default::default()
        });
        assert_eq!(info.width, 1200);
        assert_eq!(wm.get(&info.id).unwrap().title, "My App");
        wm.set_size(&info.id, 800, 600).unwrap();
        assert_eq!(wm.get(&info.id).unwrap().width, 800);
        wm.close(&info.id).unwrap();
        assert!(matches!(wm.get(&info.id), Err(WindowError::NotFound(_))));
    }
}
