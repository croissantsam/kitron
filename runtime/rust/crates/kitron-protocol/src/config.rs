//! Kitron configuration schema (spec §37).
//!
//! The developer writes `kitron.config.ts` (TypeScript). The Kitron CLI loads it
//! through Node's native TypeScript support and hands a serialized snapshot to
//! the Rust core via the `KITRON_CONFIG_JSON` environment variable, so JSON
//! serialization happens exactly once, at process start.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KitronConfig {
    pub app: AppConfig,
    pub window: WindowConfig,
    /// The app's backend (spec §28/§29). `node` is the legacy spelling and
    /// still works; `backend` covers both TypeScript (Node) and Rust backends.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backend: Option<BackendConfig>,
    pub node: NodeConfig,
    /// Permission grants from the app (spec §23). Values are explicit module
    /// names; presence in this list means "this app is allowed to use the API".
    pub permissions: Vec<String>,
    /// Unrecognized keys preserved for forward compatibility.
    #[serde(flatten)]
    pub extra: Value,
}

/// The app's backend process (spec §29).
///
/// `kind` selects the runtime:
/// - `"node"` (default) — a TypeScript/JavaScript backend run by the embedded
///   Node host (`backend/index.ts`).
/// - `"binary"` — a compiled backend executable (e.g. a Rust crate built with
///   `cargo build --release`). It speaks the same protocol, so the UI cannot
///   tell the difference.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BackendConfig {
    /// `"node"` (default) or `"binary"`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Backend entry: the TypeScript file for `node`, the executable for
    /// `binary`.
    pub entry: Option<String>,
    /// When false, Kitron does not spawn the backend at app start.
    pub auto_start: Option<bool>,
}

impl BackendConfig {
    /// The backend runtime kind, defaulting to Node.
    pub fn kind(&self) -> &str {
        self.kind.as_deref().unwrap_or("node")
    }

    /// Whether the backend should be spawned automatically (default: true).
    pub fn auto_start(&self) -> bool {
        self.auto_start.unwrap_or(true)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppConfig {
    pub name: String,
    pub identifier: String,
    pub version: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            name: "Kitron App".into(),
            identifier: "com.kitron.app".into(),
            version: "0.0.0".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WindowConfig {
    pub width: f64,
    pub height: f64,
    pub title: Option<String>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            width: 1200.0,
            height: 800.0,
            title: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NodeConfig {
    /// Backend entry, e.g. `./backend/index.ts` (spec §28/§29).
    pub entry: Option<String>,
    /// When false, Kitron does not spawn Node automatically at app start.
    #[serde(default = "default_true")]
    pub auto_start: bool,
}

impl Default for NodeConfig {
    fn default() -> Self {
        Self {
            entry: None,
            auto_start: true,
        }
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spec_example() {
        let json = r#"{
          "app": { "name": "My App", "identifier": "com.example.myapp", "version": "1.0.0" },
          "window": { "width": 1200, "height": 800 },
          "node": { "entry": "./backend/index.ts" },
          "futureKey": { "a": 1 }
        }"#;
        let cfg: KitronConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.app.name, "My App");
        assert_eq!(cfg.window.width, 1200.0);
        assert_eq!(cfg.node.entry.as_deref(), Some("./backend/index.ts"));
        assert!(cfg.node.auto_start, "auto_start defaults to true");
        assert!(cfg.extra.is_object());
    }

    #[test]
    fn parses_a_binary_backend() {
        let json = r#"{
          "app": { "name": "Rust App", "identifier": "com.example.rust", "version": "1.0.0" },
          "backend": { "type": "binary", "entry": "/apps/demo/backend", "autoStart": true }
        }"#;
        let cfg: KitronConfig = serde_json::from_str(json).unwrap();
        let backend = cfg.backend.expect("the backend section is parsed");
        assert_eq!(backend.kind(), "binary");
        assert_eq!(backend.entry.as_deref(), Some("/apps/demo/backend"));
        assert!(backend.auto_start());
    }

    #[test]
    fn the_legacy_node_section_still_works() {
        let json = r#"{
          "app": { "name": "App", "identifier": "com.example.app", "version": "1.0.0" },
          "node": { "entry": "./backend/index.ts" }
        }"#;
        let cfg: KitronConfig = serde_json::from_str(json).unwrap();
        assert!(cfg.backend.is_none());
        assert_eq!(cfg.node.entry.as_deref(), Some("./backend/index.ts"));
        assert!(cfg.node.auto_start);
    }
}
