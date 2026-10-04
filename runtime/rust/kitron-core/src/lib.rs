//! kitron-core — shared wire types.
//!
//! MUST stay in sync with `packages/core/src/ipc.ts` and `packages/core/src/errors.ts`.
//! Wire format:
//!   Request  JS→Rust:  { id, domain, method, params }
//!   Response Rust→JS:  { id, ok: true, result } | { id, ok: false, error }
//!   Event    Rust→JS:  { type: "event", name, payload }
//!   Log      Rust→JS:  { type: "log", entry }

use serde::{Deserialize, Serialize};

pub mod permissions;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcRequest {
    pub id: u64,
    pub domain: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KitronErrorBody {
    pub code: String,
    pub message: String,
    pub platform: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IpcResponse {
    Ok { id: u64, ok: bool, result: serde_json::Value },
    Err { id: u64, ok: bool, error: KitronErrorBody },
}

impl IpcResponse {
    pub fn ok(id: u64, result: serde_json::Value) -> Self {
        IpcResponse::Ok { id, ok: true, result }
    }
    pub fn err(id: u64, code: &str, message: impl Into<String>) -> Self {
        IpcResponse::Err {
            id,
            ok: false,
            error: KitronErrorBody {
                code: code.into(),
                message: message.into(),
                platform: current_platform().into(),
                details: None,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum InboundPush {
    Event { name: String, payload: serde_json::Value },
    Log { entry: LogEntry },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub level: String,
    pub scope: String,
    pub message: String,
    pub time: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

pub fn current_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    }
}

// ---- Window types (mirror packages/window) ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowCreateOptions {
    #[serde(default = "d_width")]
    pub width: u32,
    #[serde(default = "d_height")]
    pub height: u32,
    #[serde(default = "d_title")]
    pub title: String,
    pub x: Option<i32>,
    pub y: Option<i32>,
    #[serde(default = "d_true")]
    pub center: bool,
    #[serde(rename = "minWidth")]
    pub min_width: Option<u32>,
    #[serde(rename = "minHeight")]
    pub min_height: Option<u32>,
    #[serde(default = "d_true")]
    pub resizable: bool,
    #[serde(default = "d_true")]
    pub decorations: bool,
    #[serde(default = "d_true")]
    pub visible: bool,
}

fn d_width() -> u32 {
    1200
}
fn d_height() -> u32 {
    800
}
fn d_title() -> String {
    "Kitron App".into()
}
fn d_true() -> bool {
    true
}

impl Default for WindowCreateOptions {
    fn default() -> Self {
        Self {
            width: 1200,
            height: 800,
            title: "Kitron App".into(),
            x: None,
            y: None,
            center: true,
            min_width: None,
            min_height: None,
            resizable: true,
            decorations: true,
            visible: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowInfo {
    pub id: String,
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub minimized: bool,
    pub maximized: bool,
    pub visible: bool,
    pub focused: bool,
}

// ---- App config (mirror kitron.config.ts) ----

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KitronConfig {
    #[serde(default)]
    pub app: AppConfig,
    #[serde(default)]
    pub window: WindowDefaults,
    #[serde(default)]
    pub node: NodeConfig,
    /// Granted sensitive domains (§23), e.g. `["shell", "process"]`.
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "app_name")]
    pub name: String,
    #[serde(default = "app_id")]
    pub identifier: String,
    #[serde(default = "app_ver")]
    pub version: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: app_name(),
            identifier: app_id(),
            version: app_ver(),
        }
    }
}

fn app_name() -> String {
    "Kitron App".into()
}
fn app_id() -> String {
    "com.kitron.app".into()
}
fn app_ver() -> String {
    "0.1.0".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowDefaults {
    #[serde(default = "d_width")]
    pub width: u32,
    #[serde(default = "d_height")]
    pub height: u32,
}

impl Default for WindowDefaults {
    fn default() -> Self {
        Self { width: 1200, height: 800 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NodeConfig {
    pub entry: Option<String>,
}

/// Update feed entry (§22). Mirrors TS `UpdateInfo` in `@kitron/updater`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateFeed {
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipc_request_roundtrip_matches_ts_shape() {
        let req = IpcRequest {
            id: 1,
            domain: "window".into(),
            method: "create".into(),
            params: serde_json::json!({"width": 1200, "height": 800}),
        };
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"domain\":\"window\""));
        let back: IpcRequest = serde_json::from_str(&s).unwrap();
        assert_eq!(back.id, 1);
    }

    #[test]
    fn error_response_shape() {
        let r = IpcResponse::err(7, "WINDOW_CREATION_FAILED", "nope");
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"]["code"], "WINDOW_CREATION_FAILED");
    }
}
