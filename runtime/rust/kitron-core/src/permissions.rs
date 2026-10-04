//! Capability model (§23).
//!
//! Sensitive domains are NEVER granted implicitly: the app must declare
//! them in `kitron.config` (`permissions: ["shell", "process"]`) or the
//! host must pass `--permissions shell,process`. Everything else falls
//! through to normal dispatch (unknown domains → `KITRON_NOT_IMPLEMENTED`).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// Domains gated behind explicit grants.
pub const SENSITIVE_DOMAINS: &[&str] = &["shell", "process"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Permissions {
    /// Explicitly granted sensitive domains, e.g. `["shell"]`.
    #[serde(default)]
    pub granted: HashSet<String>,
}

impl Permissions {
    pub fn new(granted: impl IntoIterator<Item = String>) -> Self {
        Self { granted: granted.into_iter().collect() }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_allowed(&self, domain: &str) -> bool {
        if SENSITIVE_DOMAINS.contains(&domain) {
            self.granted.contains(domain)
        } else {
            true
        }
    }

    /// Parse a `--permissions shell,process` flag value.
    pub fn parse_flag(value: Option<&str>) -> Self {
        let granted = value
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Self { granted }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_denied_by_default() {
        let p = Permissions::empty();
        assert!(!p.is_allowed("shell"));
        assert!(!p.is_allowed("process"));
        assert!(p.is_allowed("window"));
        assert!(p.is_allowed("anything-unknown"));
    }

    #[test]
    fn grants_allow_sensitive() {
        let p = Permissions::new(["shell".to_string()]);
        assert!(p.is_allowed("shell"));
        assert!(!p.is_allowed("process"));
    }

    #[test]
    fn flag_parsing() {
        let p = Permissions::parse_flag(Some("shell, process ,,"));
        assert!(p.is_allowed("shell") && p.is_allowed("process"));
        assert!(Permissions::parse_flag(None).granted.is_empty());
    }
}
