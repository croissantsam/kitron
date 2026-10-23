//! Process utilities (spec §42): environment + spawning child processes.
//!
//! Pure Rust (no AppKit) so it works off the main thread. Gated by the
//! `process` permission (spec §23).

use kitron_protocol::KitronError;
use serde_json::Value;
use std::collections::BTreeMap;
use std::process::Command;

use crate::core;

pub(crate) fn env(name: &str) -> Result<Option<String>, KitronError> {
    core().require_permission("process")?;
    Ok(std::env::var(name).ok())
}

pub(crate) fn spawn(args_json: &str) -> Result<Value, KitronError> {
    core().require_permission("process")?;
    let parsed: Value = serde_json::from_str(args_json).unwrap_or(Value::Null);
    let command = parsed
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    if command.is_empty() {
        return Err(KitronError::code(
            "PROCESS_INVALID_ARGS",
            "spawn requires a `command` string",
        ));
    }
    let args: Vec<String> = parsed
        .get("args")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let cwd = parsed.get("cwd").and_then(Value::as_str);
    let env_vars: BTreeMap<String, String> = parsed
        .get("env")
        .and_then(Value::as_object)
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let mut cmd = Command::new(&command);
    cmd.args(&args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    for (k, v) in &env_vars {
        cmd.env(k, v);
    }
    let output = cmd
        .output()
        .map_err(|e| KitronError::code("PROCESS_SPAWN_FAILED", e.to_string()))?;
    Ok(serde_json::json!({
        "status": output.status.code(),
        "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
        "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
        "success": output.status.success(),
    }))
}