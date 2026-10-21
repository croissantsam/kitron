//! Typed error model (spec §33).
//!
//! Every Kitron API surfaces errors shaped as:
//!
//! ```json
//! { "code": "WINDOW_CREATION_FAILED", "message": "...", "platform": "macos" }
//! ```
//!
//! The TypeScript layer rehydrates these into `KitronError` instances.

use serde::{Deserialize, Serialize};

pub const PLATFORM_MACOS: &str = "macos";
pub const PLATFORM_WINDOWS: &str = "windows";

/// Structured runtime error. Serialized into the JS layer verbatim.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KitronError {
    /// Stable machine-readable code, e.g. `"NODE_SPAWN_FAILED"`.
    pub code: String,
    /// Human readable explanation.
    pub message: String,
    /// Platform the error originated on, or `"kitron"` for runtime-internal errors.
    #[serde(default = "default_platform")]
    pub platform: String,
    /// Optional structured details surfaced to the developer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

fn default_platform() -> String {
    "kitron".to_string()
}

impl KitronError {
    /// Creates an error on the current platform (compile-time platform string).
    pub fn code(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            platform: platform_name().to_string(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("KitronError is always serializable")
    }
}

/// Compile-time platform name (see spec §25/§26).
pub const fn platform_name() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        PLATFORM_MACOS
    }
    #[cfg(all(not(target_os = "macos"), target_family = "windows"))]
    {
        PLATFORM_WINDOWS
    }
    #[cfg(not(any(target_os = "macos", target_family = "windows")))]
    {
        "kitron"
    }
}

pub type KitronResult<T> = Result<T, KitronError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_expected_shape() {
        let e = KitronError::code("WINDOW_CREATION_FAILED", "boom");
        let v = e.to_json();
        assert_eq!(v["code"], "WINDOW_CREATION_FAILED");
        assert_eq!(v["platform"], platform_name());
        assert!(v.get("details").is_none());
    }
}
