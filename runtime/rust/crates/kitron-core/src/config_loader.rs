//! `kitron.config.ts` import path (spec §37).
//!
//! Developer experience: config is TypeScript. The Kitron CLI evaluates it with
//! Node's native TypeScript support and sets `KITRON_CONFIG_JSON` before
//! launching the app binary, so the Rust core deserializes a snapshot once at
//! startup — zero config parsing logic in Rust.

use kitron_protocol::{KitronConfig, KitronError};

/// Reads `KITRON_CONFIG_JSON`. Falls back to defaults when unset (useful for
/// unit tests and `kitron doctor`).
pub fn load_config_from_env() -> KitronConfig {
    match std::env::var("KITRON_CONFIG_JSON") {
        Ok(raw) if !raw.is_empty() => match serde_json::from_str(&raw) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("[error] [kitron:config] invalid KITRON_CONFIG_JSON: {error}");
                KitronConfig::default()
            }
        },
        _ => KitronConfig::default(),
    }
}

/// Parses a config with an explicit error (used by tests + `doctor`).
pub fn parse_config(json: &str) -> Result<KitronConfig, KitronError> {
    serde_json::from_str(json)
        .map_err(|e| KitronError::code("CONFIG_INVALID", format!("invalid kitron.config: {e}")))
}

/// Resolves a config-relative path (e.g. `./backend/index.ts`) against the app
/// root, normalizing `.` and `..` components.
pub fn resolve_app_path(root_dir: &str, relative: &str) -> String {
    let path = if relative.starts_with('/') {
        relative.to_string()
    } else {
        format!("{root_dir}/{relative}")
    };
    let mut out: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "." => {}
            ".." => {
                out.pop();
            }
            p => out.push(p),
        }
    }
    out.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_paths() {
        assert_eq!(
            resolve_app_path("/apps/demo", "./backend/index.ts"),
            "/apps/demo/backend/index.ts"
        );
        assert_eq!(
            resolve_app_path("/apps/demo", "./src/../backend/index.ts"),
            "/apps/demo/backend/index.ts"
        );
    }
}
