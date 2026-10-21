//! OS shell integration through `NSWorkspace` (spec §15, phase 2).
//!
//! Opens URLs and files with the user's default handler, and reveals files in
//! Finder. Main-thread only (AppKit). Gated by the `shell` permission
//! (spec §23).

use kitron_protocol::KitronError;
use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSArray, NSString, NSURL};

use crate::core;

fn require_main_thread(feature: &str) -> Result<(), KitronError> {
    if objc2::MainThreadMarker::new().is_none() {
        return Err(KitronError::code(
            "THREAD_MAIN_REQUIRED",
            format!("{feature} must run on the main thread"),
        ));
    }
    Ok(())
}

/// Opens a URL (http, file, custom scheme) with the default handler.
pub(crate) fn open_url(url: &str) -> Result<(), KitronError> {
    core().require_permission("shell")?;
    require_main_thread("shell.openUrl")?;
    if url.contains(char::is_whitespace) {
        return Err(KitronError::code(
            "SHELL_INVALID_URL",
            format!("not a valid URL: {url}"),
        ));
    }
    let value = NSString::from_str(url);
    let Some(target) = NSURL::URLWithString(&value) else {
        return Err(KitronError::code(
            "SHELL_INVALID_URL",
            format!("not a valid URL: {url}"),
        ));
    };
    // `NSURL` happily parses relative strings ("some/file") — Kitron only
    // opens absolute URLs (http/https/file or a custom scheme).
    let scheme = target.scheme().map(|value| value.to_string()).unwrap_or_default();
    if scheme.is_empty() {
        return Err(KitronError::code(
            "SHELL_INVALID_URL",
            format!("not an absolute URL: {url}"),
        ));
    }
    let opened = NSWorkspace::sharedWorkspace().openURL(&target);
    if !opened {
        return Err(KitronError::code(
            "SHELL_OPEN_FAILED",
            format!("no handler could open {url}"),
        ));
    }
    Ok(())
}

/// Opens a file or folder with the default handler.
pub(crate) fn open_path(path: &str) -> Result<(), KitronError> {
    core().require_permission("shell")?;
    require_main_thread("shell.openPath")?;
    let value = NSString::from_str(path);
    let target = NSURL::fileURLWithPath_isDirectory(&value, false);
    let opened = NSWorkspace::sharedWorkspace().openURL(&target);
    if !opened {
        return Err(KitronError::code(
            "SHELL_OPEN_FAILED",
            format!("no handler could open {path}"),
        ));
    }
    Ok(())
}

/// Reveals a file in Finder.
pub(crate) fn show_item_in_folder(path: &str) -> Result<(), KitronError> {
    core().require_permission("shell")?;
    require_main_thread("shell.showItemInFolder")?;
    let value = NSString::from_str(path);
    let target = NSURL::fileURLWithPath_isDirectory(&value, false);
    let urls = NSArray::from_retained_slice(&[target]);
    NSWorkspace::sharedWorkspace().activateFileViewerSelectingURLs(&urls);
    Ok(())
}
