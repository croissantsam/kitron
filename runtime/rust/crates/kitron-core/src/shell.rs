//! The shell contract — what every platform shell must implement (spec §40).
//!
//! The core owns the *wire models*; a shell (kitron-macos today,
//! kitron-windows next) owns the *platform implementation*. Both sides speak
//! the exact same JSON:
//!
//! - the TypeScript SDK (`@kitron/*`) serializes one of the argument models
//!   below and hands it to the bridge;
//! - the shell parses it with this crate's models — never redefines a field;
//! - the shell answers with `{ ok, result | { error } }` envelopes built by
//!   `kitron-protocol`, whose `result` payloads use the result models here.
//!
//! Besides the models, a shell must (see docs/shell-contracts.md for the full
//! handbook):
//! 1. export the C ABI the bridge calls (`kitron_window_create`, …) with the
//!    same signatures documented in `runtime/macos/pod/Kitron/kitron.h`;
//! 2. install an event sink (`Core::set_event_sink`) so events are pushed to
//!    the UI instead of pooling in the bus;
//! 3. gate sensitive calls through `Core::require_permission`;
//! 4. answer the window snapshot shape (`WindowSnapshot`) for `window.list`
//!    — the JSI surface reads it directly, so the fields and casing are part
//!    of the public SDK (spec §11).

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

/// One live window, as the JSI `window.list` snapshot returns it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowSnapshot {
    pub id: u64,
    pub title: String,
    pub frame: WindowFrame,
    pub focused: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub visible: bool,
}

/// Frame in top-left screen coordinates (not AppKit's bottom-left).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowCreateArgs {
    pub title: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub focus: Option<bool>,
    pub resizable: Option<bool>,
    pub minimizable: Option<bool>,
    pub closable: Option<bool>,
}

/// Window ops share this wire name on every platform (`windowOp(id, op)`).
/// Mirrors the TypeScript surface in `packages/window` exactly — a name that
/// is not sent there must not exist here, and vice versa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowOp {
    Close,
    Minimize,
    Maximize,
    Restore,
    Focus,
    Show,
    Hide,
    SetSize,
    SetPosition,
    SetTitle,
    SetPreventClose,
    /// A shell-specific or unknown op — the shell decides how to answer.
    Unknown,
}

impl WindowOp {
    /// The wire name. Kept explicit so a second shell cannot drift from the
    /// TypeScript `window.*` surface.
    pub fn from_wire(value: &str) -> Self {
        match value {
            "close" => Self::Close,
            "minimize" => Self::Minimize,
            "maximize" => Self::Maximize,
            "restore" => Self::Restore,
            "focus" => Self::Focus,
            "show" => Self::Show,
            "hide" => Self::Hide,
            "setSize" => Self::SetSize,
            "setPosition" => Self::SetPosition,
            "setTitle" => Self::SetTitle,
            "setPreventClose" => Self::SetPreventClose,
            _ => Self::Unknown,
        }
    }

    pub fn as_wire(self) -> Option<&'static str> {
        Some(match self {
            Self::Close => "close",
            Self::Minimize => "minimize",
            Self::Maximize => "maximize",
            Self::Restore => "restore",
            Self::Focus => "focus",
            Self::Show => "show",
            Self::Hide => "hide",
            Self::SetSize => "setSize",
            Self::SetPosition => "setPosition",
            Self::SetTitle => "setTitle",
            Self::SetPreventClose => "setPreventClose",
            Self::Unknown => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// Menus
// ---------------------------------------------------------------------------

/// The application menu, sent from `menu.setApplicationMenu` (spec §18).
#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MenuDefinition {
    pub label: String,
    pub items: Vec<MenuItemDefinition>,
}

#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MenuItemDefinition {
    /// "separator" renders a divider.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub label: Option<String>,
    /// Action id dispatched to JS (`menu.action`) when the item is clicked.
    pub action: Option<String>,
    /// Accelerator, e.g. "CmdOrCtrl+N".
    pub accelerator: Option<String>,
    /// Nested items → a submenu.
    pub items: Option<Vec<MenuItemDefinition>>,
    pub enabled: Option<bool>,
    pub checked: Option<bool>,
}

// ---------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DialogMessageArgs {
    pub title: Option<String>,
    pub message: Option<String>,
    pub detail: Option<String>,
    /// Button labels, in order. Defaults to a single "OK".
    pub buttons: Option<Vec<String>>,
    /// "info" | "warning" | "critical".
    pub style: Option<String>,
    /// Present as a sheet attached to this window instead of app-modal.
    pub window_id: Option<u64>,
}

#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DialogOpenFileArgs {
    pub title: Option<String>,
    pub multiple: Option<bool>,
    pub directory: Option<String>,
    pub extensions: Option<Vec<String>>,
    /// Present as a sheet attached to this window instead of app-modal.
    pub window_id: Option<u64>,
}

#[derive(Debug, Default, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DialogSaveFileArgs {
    pub title: Option<String>,
    pub default_name: Option<String>,
    pub directory: Option<String>,
    /// Present as a sheet attached to this window instead of app-modal.
    pub window_id: Option<u64>,
}

/// Result payload builders — the JSON keys are the TypeScript `RequestResult`
/// contract, so they live here rather than in a shell.
pub mod dialog_results {
    use serde_json::{json, Value};

    /// `dialog.message` → the label of the clicked button (first = confirmed).
    #[must_use]
    pub fn message(labels: &[String], response: isize) -> Value {
        let first_button_code: isize = 1000; // AppKit NSAlertFirstButtonReturn.
        let index = (response - first_button_code).max(0) as usize;
        let button = labels
            .get(index)
            .cloned()
            .unwrap_or_else(|| "OK".to_string());
        json!({ "button": button, "index": index, "confirmed": index == 0 })
    }

    /// `dialog.openFile` / `dialog.saveFile` — a canceled panel keeps the
    /// shape (`canceled: true`) with no path.
    #[must_use]
    pub fn canceled_open() -> Value {
        json!({ "canceled": true, "paths": [] })
    }

    #[must_use]
    pub fn chose_open(paths: Vec<String>) -> Value {
        json!({ "canceled": false, "paths": paths })
    }

    #[must_use]
    pub fn canceled_save() -> Value {
        json!({ "canceled": true, "path": Value::Null })
    }

    #[must_use]
    pub fn chose_save(path: Option<String>) -> Value {
        json!({ "canceled": false, "path": path })
    }
}

// ---------------------------------------------------------------------------
// Contract tests — the norms a second shell must keep.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_definition_round_trips() {
        let json = r#"{
            "label": "File",
            "items": [
                { "label": "New", "action": "file.new", "accelerator": "CmdOrCtrl+N" },
                { "type": "separator" },
                { "label": "Special", "items": [ { "label": "Deep", "action": "file.deep" } ] },
                { "label": "Readonly", "enabled": false, "checked": true,
                  "accelerator": "CmdOrCtrl+R" }
            ]
        }"#;
        let menu: MenuDefinition = serde_json::from_str(json).expect("menu must parse");
        assert_eq!(menu.label, "File");
        assert_eq!(menu.items.len(), 4);
        assert_eq!(menu.items[0].action.as_deref(), Some("file.new"));
        assert_eq!(menu.items[1].kind.as_deref(), Some("separator"));
        let nested = menu.items[2].items.as_ref().expect("submenu");
        assert_eq!(nested.len(), 1);
        assert_eq!(menu.items[3].checked, Some(true));
    }

    #[test]
    fn dialog_args_parse_camel_case() {
        // The TypeScript SDK sends camelCase (`windowId`, `defaultName`).
        let message: DialogMessageArgs = serde_json::from_str(
            r#"{ "title": "t", "style": "warning", "windowId": 1, "buttons": ["OK"] }"#,
        )
        .expect("message args");
        assert_eq!(message.window_id, Some(1));
        assert_eq!(message.style.as_deref(), Some("warning"));

        let save: DialogSaveFileArgs =
            serde_json::from_str(r#"{ "defaultName": "out.txt", "windowId": 2 }"#)
                .expect("save args");
        assert_eq!(save.default_name.as_deref(), Some("out.txt"));
        assert_eq!(save.window_id, Some(2));

        // Absent windowId → app-modal on any platform.
        let plain: DialogOpenFileArgs = serde_json::from_str("{}").expect("open args");
        assert!(plain.window_id.is_none());
        assert!(plain.multiple.is_none());
    }

    #[test]
    fn dialog_result_shapes_match_the_sdk() {
        // The TS `MessageResult` reads button/index/confirmed.
        let message = crate::shell::dialog_results::message(
            &["OK".to_string(), "Cancel".to_string()],
            1000,
        );
        assert_eq!(message["button"], "OK");
        assert_eq!(message["index"], 0);
        assert_eq!(message["confirmed"], true);

        let second = crate::shell::dialog_results::message(
            &["OK".to_string(), "Cancel".to_string()],
            1001,
        );
        assert_eq!(second["button"], "Cancel");
        assert_eq!(second["confirmed"], false);

        let canceled = crate::shell::dialog_results::canceled_save();
        assert_eq!(canceled["canceled"], true);
        assert!(canceled["path"].is_null());

        let saved = crate::shell::dialog_results::chose_save(Some("/tmp/a".into()));
        assert_eq!(saved["path"], "/tmp/a");
        let opened = crate::shell::dialog_results::chose_open(vec!["/tmp/a".into()]);
        assert_eq!(opened["paths"].as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn window_snapshot_field_casing_is_public() {
        // The JSI `window.list` surface consumes this JSON directly; the field
        // names are part of the SDK contract (spec §11).
        let snapshot: WindowSnapshot = serde_json::from_str(
            r#"{
                "id": 1, "title": "w", "focused": true, "minimized": false,
                "maximized": false, "visible": true,
                "frame": { "x": 1, "y": 2, "width": 800, "height": 600 }
            }"#,
        )
        .expect("snapshot");
        assert_eq!(snapshot.frame.width, 800.0);
        let rendered = serde_json::to_value(&snapshot).expect("serialize");
        assert!(rendered["frame"]["width"].is_number());
        assert!(rendered.get("windowPtr").is_none());
    }

    #[test]
    fn window_ops_match_the_typescript_names() {
        // Exactly the ops `packages/window` sends — no more, no less.
        for (wire, op) in [
            ("close", WindowOp::Close),
            ("minimize", WindowOp::Minimize),
            ("maximize", WindowOp::Maximize),
            ("restore", WindowOp::Restore),
            ("focus", WindowOp::Focus),
            ("show", WindowOp::Show),
            ("hide", WindowOp::Hide),
            ("setSize", WindowOp::SetSize),
            ("setPosition", WindowOp::SetPosition),
            ("setTitle", WindowOp::SetTitle),
            ("setPreventClose", WindowOp::SetPreventClose),
        ] {
            assert_eq!(WindowOp::from_wire(wire), op, "wire name {wire}");
            assert_eq!(op.as_wire(), Some(wire));
        }
        assert_eq!(WindowOp::from_wire("unmaximize"), WindowOp::Unknown);
        assert_eq!(WindowOp::from_wire("blink"), WindowOp::Unknown);
        assert_eq!(WindowOp::Unknown.as_wire(), None);
    }
}
