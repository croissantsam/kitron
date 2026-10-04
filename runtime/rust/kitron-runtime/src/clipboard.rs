//! Clipboard via NSPasteboard (macOS) / Win32 clipboard (Windows),
//! through the `arboard` abstraction.

use std::sync::Mutex;

pub struct Clipboard {
    inner: Mutex<arboard::Clipboard>,
}

impl Clipboard {
    pub fn new() -> anyhow_like::Result<Self> {
        Ok(Self { inner: Mutex::new(arboard::Clipboard::new().map_err(|e| e.to_string())?) })
    }
    pub fn write_text(&self, text: String) -> Result<(), String> {
        self.inner.lock().map_err(|e| e.to_string())?.set_text(text).map_err(|e| e.to_string())
    }
    pub fn read_text(&self) -> Result<String, String> {
        self.inner.lock().map_err(|e| e.to_string())?.get_text().map_err(|e| e.to_string())
    }
}

mod anyhow_like {
    pub type Result<T> = std::result::Result<T, String>;
}
