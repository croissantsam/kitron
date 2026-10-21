//! Native dialogs — `NSAlert`, `NSOpenPanel`, `NSSavePanel` (spec §12, phase 2).
//!
//! Main-thread only (AppKit). The panels are modal: the call returns once the
//! user dismisses them. Gated by the `dialog` permission (spec §23).

use kitron_protocol::KitronError;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSAlert, NSAlertStyle, NSModalResponse, NSModalResponseCancel, NSOpenPanel, NSSavePanel,
};
use objc2_foundation::{NSArray, NSString, NSURL};
use objc2_uniform_type_identifiers::UTType;
use serde::Deserialize;

use crate::core;

/// AppKit's `NSAlertFirstButtonReturn` — the return code of the first button.
const NS_ALERT_FIRST_BUTTON_RETURN: NSModalResponse = 1000;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct MessageArgs {
    title: Option<String>,
    message: Option<String>,
    detail: Option<String>,
    buttons: Option<Vec<String>>,
    /// "info" | "warning" | "critical"
    style: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct OpenFileArgs {
    title: Option<String>,
    multiple: Option<bool>,
    directory: Option<String>,
    extensions: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct SaveFileArgs {
    title: Option<String>,
    default_name: Option<String>,
    directory: Option<String>,
}

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

    let response = alert.runModal();
    // An NSAlert answers with `NSAlertFirstButtonReturn + index` (1000+),
    // not with `NSModalResponseOK` — the "confirmed" flag is therefore
    // "the first button was clicked".
    let index = response - NS_ALERT_FIRST_BUTTON_RETURN;
    let index = if index < 0 { 0 } else { index };
    let button = labels
        .get(index as usize)
        .cloned()
        .unwrap_or_else(|| "OK".to_string());
    Ok(serde_json::json!({
        "button": button,
        "index": index,
        "confirmed": index == 0,
    }))
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

    let response = panel.runModal();
    if response == NSModalResponseCancel {
        return Ok(serde_json::json!({ "canceled": true, "paths": [] }));
    }
    let urls = panel.URLs();
    let mut paths = Vec::new();
    for url in urls.iter() {
        if let Some(path) = url.path() {
            paths.push(path.to_string());
        }
    }
    Ok(serde_json::json!({ "canceled": false, "paths": paths }))
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

    let response = panel.runModal();
    if response == NSModalResponseCancel {
        return Ok(serde_json::json!({ "canceled": true, "path": serde_json::Value::Null }));
    }
    let path = panel
        .URL()
        .and_then(|url| url.path().map(|value| value.to_string()));
    Ok(serde_json::json!({ "canceled": false, "path": path }))
}
