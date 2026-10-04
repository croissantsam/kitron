//! Fallback for unsupported targets.

use anyhow::Result;

use super::{DialogMessageArgs, DialogOpenArgs, DialogSaveArgs};

pub fn platform_name() -> &'static str {
    "unsupported"
}

pub fn webview_backend() -> &'static str {
    "headless"
}

pub fn notification_show(
    _title: &str,
    _body: &str,
    _silent: bool,
    _sound: Option<&str>,
) -> Result<()> {
    anyhow::bail!("notifications unsupported on this platform")
}

pub fn menu_set() -> Result<()> {
    Ok(())
}

pub fn tray_create(tooltip: &str) -> Result<String> {
    Ok(format!("tray-{tooltip}"))
}

pub fn dialog_open(_args: &DialogOpenArgs) -> Result<(bool, Vec<String>)> {
    Ok((true, vec![]))
}

pub fn dialog_save(_args: &DialogSaveArgs) -> Result<(bool, Vec<String>)> {
    Ok((true, vec![]))
}

pub fn dialog_message(_args: &DialogMessageArgs) -> Result<(bool, usize)> {
    Ok((true, 0))
}

pub fn clipboard_read_image() -> Result<Option<Vec<u8>>> {
    Ok(None)
}

pub fn clipboard_write_image(_png: &[u8]) -> Result<()> {
    anyhow::bail!("image clipboard unsupported on this platform")
}

pub fn capture_window_rect(_x: i32, _y: i32, _w: u32, _h: u32) -> Result<Vec<u8>> {
    anyhow::bail!("capture unsupported on this platform")
}

pub fn clipboard_read() -> Result<String> {
    Ok(String::new())
}

pub fn clipboard_write(_text: &str) -> Result<()> {
    Ok(())
}

pub fn clipboard_read_format(_format: &str) -> Result<String> {
    Ok(String::new())
}

pub fn clipboard_write_format(_format: &str, _text: &str) -> Result<()> {
    Ok(())
}
