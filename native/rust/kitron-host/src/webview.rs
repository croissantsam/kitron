//! WebView abstraction (§8).
//!
//! Nothing outside this module may name WKWebView / WebView2 / WebKitGTK.
//! The `native-webview` feature swaps the headless implementation for a
//! real wry backend; the IPC surface is identical either way.

use anyhow::Result;

/// Common WebView capability surface.
pub trait WebView: Send {
    fn load_url(&self, url: &str) -> Result<()>;
    fn load_file(&self, path: &str) -> Result<()>;
    fn reload(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
    fn close(&self) -> Result<()>;
    fn set_title(&self, title: &str) -> Result<()>;
    fn open_devtools(&self) -> Result<()>;
}

/// Headless fallback: records navigation for tests / CI without a display.
#[derive(Debug, Default)]
pub struct HeadlessWebView {
    pub current: std::sync::Mutex<Option<String>>,
}

impl WebView for HeadlessWebView {
    fn load_url(&self, url: &str) -> Result<()> {
        *self.current.lock().unwrap() = Some(url.to_string());
        Ok(())
    }
    fn load_file(&self, path: &str) -> Result<()> {
        *self.current.lock().unwrap() = Some(format!("file://{path}"));
        Ok(())
    }
    fn reload(&self) -> Result<()> {
        Ok(())
    }
    fn stop(&self) -> Result<()> {
        Ok(())
    }
    fn close(&self) -> Result<()> {
        Ok(())
    }
    fn set_title(&self, _title: &str) -> Result<()> {
        Ok(())
    }
    fn open_devtools(&self) -> Result<()> {
        Ok(())
    }
}

#[cfg(feature = "native-webview")]
pub mod wry_backend {
    //! Real native WebViews via wry:
    //! - macOS → WKWebView (WebKit)
    //! - Windows → WebView2 (Edge runtime, provided by the OS)
    //! - Linux → WebKitGTK (Wayland / X11)
    use super::*;
    use std::sync::mpsc;

    pub enum UiCommand {
        LoadUrl(String),
        LoadFile(String),
        Reload,
        SetTitle(String),
        OpenDevTools,
        Close,
    }

    /// Handle posted to the tao event loop thread.
    #[derive(Clone)]
    pub struct WryHandle {
        pub tx: mpsc::Sender<UiCommand>,
    }

    impl WebView for WryHandle {
        fn load_url(&self, url: &str) -> Result<()> {
            self.tx.send(UiCommand::LoadUrl(url.to_string()))?;
            Ok(())
        }
        fn load_file(&self, path: &str) -> Result<()> {
            self.tx.send(UiCommand::LoadFile(path.to_string()))?;
            Ok(())
        }
        fn reload(&self) -> Result<()> {
            self.tx.send(UiCommand::Reload)?;
            Ok(())
        }
        fn stop(&self) -> Result<()> {
            Ok(())
        }
        fn close(&self) -> Result<()> {
            self.tx.send(UiCommand::Close)?;
            Ok(())
        }
        fn set_title(&self, title: &str) -> Result<()> {
            self.tx.send(UiCommand::SetTitle(title.to_string()))?;
            Ok(())
        }
        fn open_devtools(&self) -> Result<()> {
            self.tx.send(UiCommand::OpenDevTools)?;
            Ok(())
        }
    }
}
