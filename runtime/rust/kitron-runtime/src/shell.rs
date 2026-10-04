//! Shell helpers (openExternal / openPath / showInFolder).
pub fn open_external(url: &str) -> Result<(), String> {
    if !(url.starts_with("http://")
        || url.starts_with("https://")
        || url.starts_with("mailto:")
        || url.contains("://"))
    {
        return Err("SHELL_OPEN_FAILED: unsupported URL scheme".into());
    }
    open::that(url).map_err(|e| format!("SHELL_OPEN_FAILED: {e}"))
}

pub fn open_path(path: &str) -> Result<(), String> {
    open::that(path).map_err(|e| format!("SHELL_OPEN_FAILED: {e}"))
}

pub fn show_item_in_folder(path: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .status()
            .map(|_| ())
            .map_err(|e| format!("SHELL_OPEN_FAILED: {e}"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        open::that(path).map_err(|e| format!("SHELL_OPEN_FAILED: {e}"))
    }
}
