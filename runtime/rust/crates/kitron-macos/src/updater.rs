//! Updater (spec §42): checks a remote version endpoint and reports
//! whether a newer release is available.
//!
//! Gated by the `updater` permission (spec §23). `download` opens the
//! release page in the user's browser; installing the downloaded bundle is
//! left to the platform installer (out of scope for the MVP).

use kitron_protocol::KitronError;
use serde_json::Value;
use std::process::Command;

use crate::core;

fn current_version() -> String {
    crate::core().config.app.version.clone()
}

pub(crate) fn check(args_json: &str) -> Result<Value, KitronError> {
    core().require_permission("updater")?;
    let parsed: Value = serde_json::from_str(args_json).unwrap_or(Value::Null);
    let url = parsed
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if url.is_empty() {
        return Err(KitronError::code(
            "UPDATER_NO_URL",
            "updater.check requires a `url` (version endpoint)",
        ));
    }
    // Fetch the remote version string via curl (handles HTTPS).
    let output = Command::new("curl")
        .args(["-fsSL", "--max-time", "15", &url])
        .output()
        .map_err(|e| KitronError::code("UPDATER_FETCH_FAILED", e.to_string()))?;
    if !output.status.success() {
        return Err(KitronError::code(
            "UPDATER_FETCH_FAILED",
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }
    let remote = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let latest = remote.lines().next().unwrap_or(&remote).trim().to_string();
    let current = current_version();
    let update_available = version_gt(&latest, &current);
    Ok(serde_json::json!({
        "current": current,
        "latest": latest,
        "updateAvailable": update_available,
    }))
}

pub(crate) fn download(args_json: &str) -> Result<Value, KitronError> {
    core().require_permission("updater")?;
    let parsed: Value = serde_json::from_str(args_json).unwrap_or(Value::Null);
    let url = parsed.get("url").and_then(Value::as_str).unwrap_or("");
    if !url.is_empty() {
        // Open the release page in the default browser.
        let _ = Command::new("open").arg(url).status();
    }
    Ok(serde_json::json!({ "opened": !url.is_empty() }))
}

fn version_gt(a: &str, b: &str) -> bool {
    // Simple numeric component comparison (e.g. "1.2.3" vs "1.2.0").
    let parse = |s: &str| -> Vec<u64> {
        s.trim_start_matches('v')
            .split('.')
            .filter_map(|p| p.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok())
            .collect()
    };
    let va = parse(a);
    let vb = parse(b);
    let n = va.len().max(vb.len());
    for i in 0..n {
        let x = va.get(i).copied().unwrap_or(0);
        let y = vb.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}