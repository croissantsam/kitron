//! Platform isolation (§34): OS-specific code lives behind this module.
//! Shared bridge code calls these functions; each file implements them for
//! one OS (AppKit / WebKit on macOS, WebView2 on Windows, WebKitGTK on Linux).

#[cfg(target_os = "macos")]
pub mod macos_menu;
#[cfg(target_os = "macos")]
pub use macos_menu::{
    apply_menu_template, popup_menu, tray_create, tray_destroy, window_list,
};

#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
pub use windows::*;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub mod fallback;
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
pub use fallback::*;

use anyhow::Result;

/// Run a command, return trimmed stdout or an error with stderr context.
pub fn run_cmd(cmd: &str, args: &[&str]) -> Result<String> {
    let out = std::process::Command::new(cmd).args(args).output()?;
    if !out.status.success() {
        anyhow::bail!(
            "{} failed: {}",
            cmd,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// Unique temp path for PNG payloads.
pub fn temp_png(prefix: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "kitron-{}-{}-{}.png",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    p
}

#[derive(Debug, Clone, Default)]
pub struct DialogOpenArgs {
    pub title: Option<String>,
    pub default_path: Option<String>,
    pub filters: Vec<(String, Vec<String>)>,
    pub multiple: bool,
    pub directory: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DialogSaveArgs {
    pub title: Option<String>,
    pub default_path: Option<String>,
    pub default_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DialogMessageArgs {
    pub title: Option<String>,
    pub message: String,
    pub detail: Option<String>,
    pub buttons: Vec<String>,
    pub kind: String,
}
