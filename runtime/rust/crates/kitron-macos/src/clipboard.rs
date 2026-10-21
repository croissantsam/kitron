//! Clipboard access through `NSPasteboard` (spec §13, phase 2).
//!
//! Main-thread only (AppKit). The permission gate lives in the core
//! (`clipboard`), so an app must declare it in `kitron.config.ts` (spec §23).

use kitron_protocol::KitronError;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::NSString;

use crate::core;

/// Reads the clipboard as text (empty string when it holds no text).
pub(crate) fn read_text() -> Result<String, KitronError> {
    core().require_permission("clipboard")?;
    let Some(_mtm) = objc2::MainThreadMarker::new() else {
        return Err(KitronError::code(
            "THREAD_MAIN_REQUIRED",
            "clipboard access must run on the main thread",
        ));
    };
    let pasteboard = NSPasteboard::generalPasteboard();
    let text = pasteboard.stringForType(unsafe { NSPasteboardTypeString });
    Ok(text.map(|value| value.to_string()).unwrap_or_default())
}

/// Writes text to the clipboard, replacing its current content.
pub(crate) fn write_text(text: &str) -> Result<(), KitronError> {
    core().require_permission("clipboard")?;
    let Some(_mtm) = objc2::MainThreadMarker::new() else {
        return Err(KitronError::code(
            "THREAD_MAIN_REQUIRED",
            "clipboard access must run on the main thread",
        ));
    };
    let pasteboard = NSPasteboard::generalPasteboard();
    let value = NSString::from_str(text);
    pasteboard.clearContents();
    pasteboard.setString_forType(&value, unsafe { NSPasteboardTypeString });
    Ok(())
}
