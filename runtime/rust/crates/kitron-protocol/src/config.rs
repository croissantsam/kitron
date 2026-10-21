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
    pub node: NodeConfig,
    /// Permission grants from the app (spec §23). Values are explicit module
    /// names; presence in this list means "this app is allowed to use the API".
    pub permissions: Vec<String>,
    /// Unrecognized keys preserved for forward compatibility.
    #[serde(flatten)]
    pub extra: Value,
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
}
