//! Native dialogs — `NSAlert`, `NSOpenPanel`, `NSSavePanel` (spec §12, phase 2).
//!
//! Main-thread only (AppKit). Gated by the `dialog` permission (spec §23).
//!
//! Presentation: app-modal by default, or **attached to a window as a sheet**
//! when the call passes `windowId`. Sheets run a bounded local event pump
//! (NSModalPanelRunLoopMode) so the synchronous call can return the same
//! response `runModal()` would have answered with — the JS promise semantics
//! are unchanged, only the visual presentation differs.

use std::sync::{Arc, atomic::{AtomicBool, AtomicIsize, Ordering}};

use block2::RcBlock;
use kitron_protocol::KitronError;
use objc2::rc::{Retained};
use objc2_app_kit::{
    NSAlert, NSAlertStyle, NSApplication, NSModalPanelRunLoopMode, NSModalResponse,
    NSModalResponseCancel, NSOpenPanel, NSSavePanel, NSWindow,
};
use objc2_foundation::{NSArray, NSDate, NSString, NSURL};
use objc2_uniform_type_identifiers::UTType;

// The wire models live in the portable core — a shell only parses and fills
// them; result builders keep the JSON keys the TypeScript SDK reads.
use kitron_core::shell::{
    dialog_results, DialogMessageArgs as MessageArgs, DialogOpenFileArgs as OpenFileArgs,
    DialogSaveFileArgs as SaveFileArgs,
};

use crate::{core, window};

fn require_main_thread(feature: &str) -> Result<(), KitronError> {
    if objc2::MainThreadMarker::new().is_none() {
        return Err(KitronError::code(
            "THREAD_MAIN_REQUIRED",
            format!("{feature} must run on the main thread"),
        ));
    }
    Ok(())
}

fn alert_style(name: Option<&str>) -> NSAlertStyle {
    match name {
        Some("warning") => NSAlertStyle::Warning,
        Some("critical") | Some("error") => NSAlertStyle::Critical,
        _ => NSAlertStyle::Informational,
    }
}

/// The window a sheet should attach to, when the call asks for one.
///
/// `windowId` is optional: `None` (or an unknown id handed back with a clear
/// error) leaves the caller free to fall back to the app-modal presentation.
fn resolve_sheet_window(window_id: Option<u64>) -> Result<Option<Retained<NSWindow>>, KitronError> {
    let Some(id) = window_id else {
        return Ok(None);
    };
    let Some((win, _, _)) = window::get_handle(id) else {
        return Err(KitronError::code(
            "WINDOW_NOT_FOUND",
            format!("no window {id} to attach the dialog to"),
        ));
    };
    Ok(Some(win))
}

///
/// Presents a dialog as a **sheet** attached to `window` and blocks — pumping
/// AppKit's modal-panel run loop — until the user dismisses it, then answers
/// with the same modal response `runModal()` would have produced.
///
/// The pump must keep dispatching events (the sheet is served in
/// NSModalPanelRunLoopMode) while the Rust call is parked; untilDate
/// distantFuture makes the pump block on the run loop instead of spinning.
fn run_sheet<F>(mtm: objc2::MainThreadMarker, begin: F) -> NSModalResponse
where
    F: FnOnce(&RcBlock<dyn Fn(NSModalResponse)>),
{
    let answered = Arc::new(AtomicIsize::new(0));
    let finished = Arc::new(AtomicBool::new(false));

    let response_cell = Arc::clone(&answered);
    let done_flag = Arc::clone(&finished);
    let block = RcBlock::<dyn Fn(NSModalResponse)>::new(move |response: NSModalResponse| {
        response_cell.store(response, Ordering::SeqCst);
        done_flag.store(true, Ordering::SeqCst);
    });

    begin(&block);

    let app = NSApplication::sharedApplication(mtm);
    while !finished.load(Ordering::SeqCst) {
        let Some(event) = app.nextEventMatchingMask_untilDate_inMode_dequeue(
            objc2_app_kit::NSEventMask::Any,
            Some(&NSDate::distantFuture()),
            // SAFETY: `NSModalPanelRunLoopMode` is a plain extern static — an
            // NSString constant AppKit owns and never frees.
            unsafe { NSModalPanelRunLoopMode },
            true,
        ) else {
            continue;
        };
        app.sendEvent(&event);
    }
    answered.load(Ordering::SeqCst)
}

/// Shows a native message/alert. Returns the label of the clicked button.
pub(crate) fn message(args_json: &str) -> Result<serde_json::Value, KitronError> {
    core().require_permission("dialog")?;
    require_main_thread("dialog.message")?;
    let args: MessageArgs = serde_json::from_str(args_json).unwrap_or_default();

    let Some(mtm) = objc2::MainThreadMarker::new() else {
        unreachable!("main thread checked above");
    };
    let alert = NSAlert::new(mtm);
    if let Some(title) = &args.title {
        alert.setMessageText(&NSString::from_str(title));
    }
    if let Some(message) = &args.message {
        alert.setInformativeText(&NSString::from_str(message));
    }
    alert.setAlertStyle(alert_style(args.style.as_deref()));

    let labels: Vec<String> = args.buttons.clone().unwrap_or_else(|| vec!["OK".into()]);
    for label in &labels {
        alert.addButtonWithTitle(&NSString::from_str(label));
    }

    let response = match resolve_sheet_window(args.window_id)? {
        // Attached: the alert slides in from the window's title bar.
        Some(win) => run_sheet(mtm, |block| {
            alert.beginSheetModalForWindow_completionHandler(&win, Some(block));
        }),
        None => alert.runModal(),
    };
    // An NSAlert answers `NSAlertFirstButtonReturn + index` (1000+), so the
    // "confirmed" flag is "the first button was clicked".
    Ok(dialog_results::message(&labels, response))
}

/// Opens the native file picker. Returns the selected paths.
pub(crate) fn open_file(args_json: &str) -> Result<serde_json::Value, KitronError> {
    core().require_permission("dialog")?;
    require_main_thread("dialog.openFile")?;
    let args: OpenFileArgs = serde_json::from_str(args_json).unwrap_or_default();

    let Some(mtm) = objc2::MainThreadMarker::new() else {
        unreachable!("main thread checked above");
    };
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setAllowsMultipleSelection(args.multiple.unwrap_or(false));
    if let Some(title) = &args.title {
        panel.setTitle(Some(&NSString::from_str(title)));
    }
    if let Some(directory) = &args.directory {
        let path = NSString::from_str(directory);
        let url = NSURL::fileURLWithPath_isDirectory(&path, true);
        panel.setDirectoryURL(Some(&url));
    }
    if let Some(extensions) = &args.extensions {
        let types: Vec<_> = extensions
            .iter()
            .filter_map(|ext| UTType::typeWithFilenameExtension(&NSString::from_str(ext)))
            .collect();
        if !types.is_empty() {
            let list = NSArray::from_retained_slice(&types);
            panel.setAllowedContentTypes(&list);
        }
    }

    let response = match resolve_sheet_window(args.window_id)? {
        Some(win) => run_sheet(mtm, |block| {
            panel.beginSheetModalForWindow_completionHandler(&win, block);
        }),
        None => panel.runModal(),
    };
    if response == NSModalResponseCancel {
        return Ok(dialog_results::canceled_open());
    }
    let urls = panel.URLs();
    let mut paths = Vec::new();
    for url in urls.iter() {
        if let Some(path) = url.path() {
            paths.push(path.to_string());
        }
    }
    Ok(dialog_results::chose_open(paths))
}

/// Opens the native save panel. Returns the chosen path.
pub(crate) fn save_file(args_json: &str) -> Result<serde_json::Value, KitronError> {
    core().require_permission("dialog")?;
    require_main_thread("dialog.saveFile")?;
    let args: SaveFileArgs = serde_json::from_str(args_json).unwrap_or_default();

    let Some(mtm) = objc2::MainThreadMarker::new() else {
        unreachable!("main thread checked above");
    };
    let panel: Retained<NSSavePanel> = NSSavePanel::savePanel(mtm);
    if let Some(title) = &args.title {
        panel.setTitle(Some(&NSString::from_str(title)));
    }
    if let Some(name) = &args.default_name {
        panel.setNameFieldStringValue(&NSString::from_str(name));
    }
    if let Some(directory) = &args.directory {
        let path = NSString::from_str(directory);
        let url = NSURL::fileURLWithPath_isDirectory(&path, true);
        panel.setDirectoryURL(Some(&url));
    }

    let response = match resolve_sheet_window(args.window_id)? {
        Some(win) => run_sheet(mtm, |block| {
            panel.beginSheetModalForWindow_completionHandler(&win, block);
        }),
        None => panel.runModal(),
    };
    if response == NSModalResponseCancel {
        return Ok(dialog_results::canceled_save());
    }
    let path = panel
        .URL()
        .and_then(|url| url.path().map(|value| value.to_string()));
    Ok(dialog_results::chose_save(path))
}
