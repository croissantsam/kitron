//! Updater (spec §42): checks a remote version endpoint, downloads the
//! release bundle, verifies its checksum and stages an install.
//!
//! Gated by the `updater` permission (spec §23). `install` swaps the running
//! `.app` bundle on the next launch via a detached helper (the running app
//! cannot overwrite itself).

use kitron_protocol::KitronError;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use crate::core;

fn current_version() -> String {
    crate::core().config.app.version.clone()
}

fn current_app_path() -> Option<PathBuf> {
    // std::env::current_exe() → …/Foo.app/Contents/MacOS/Foo
    let exe = std::env::current_exe().ok()?;
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    if app.extension().and_then(|e| e.to_str()) == Some("app") {
        Some(app.to_path_buf())
    } else {
        None
    }
}

fn updates_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join("Library")
        .join("Caches")
        .join("kitron-updates")
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
    let url = parsed
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if url.is_empty() {
        return Err(KitronError::code(
            "UPDATER_NO_URL",
            "updater.download requires a `url`",
        ));
    }
    let sha256 = parsed.get("sha256").and_then(Value::as_str).map(str::to_string);

    fs::create_dir_all(updates_dir())
        .map_err(|e| KitronError::code("UPDATER_IO", e.to_string()))?;
    let file_name = url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or("update.bin");
    let dest = updates_dir().join(file_name);
    let status = Command::new("curl")
        .args(["-fsSL", "--max-time", "300", "-o", dest.to_str().unwrap_or(""), &url])
        .status()
        .map_err(|e| KitronError::code("UPDATER_DOWNLOAD_FAILED", e.to_string()))?;
    if !status.success() {
        return Err(KitronError::code(
            "UPDATER_DOWNLOAD_FAILED",
            format!("curl exited with {status}"),
        ));
    }
    let size = fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);

    // Optional SHA-256 verification.
    let mut verified = sha256.is_none();
    if let Some(expected) = &sha256 {
        let out = Command::new("shasum")
            .args(["-a", "256", dest.to_str().unwrap_or("")])
            .output();
        if let Ok(o) = out {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let actual = stdout.split_whitespace().next().unwrap_or("");
            verified = actual.eq_ignore_ascii_case(expected);
        }
        if !verified {
            let _ = fs::remove_file(&dest);
            return Err(KitronError::code(
                "UPDATER_CHECKSUM_MISMATCH",
                "downloaded file failed SHA-256 verification",
            ));
        }
    }

    Ok(serde_json::json!({
        "path": dest.to_string_lossy(),
        "verified": verified,
        "size": size,
    }))
}

pub(crate) fn install(args_json: &str) -> Result<Value, KitronError> {
    core().require_permission("updater")?;
    let parsed: Value = serde_json::from_str(args_json).unwrap_or(Value::Null);
    let bundle_path = parsed
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if bundle_path.is_empty() {
        return Err(KitronError::code(
            "UPDATER_NO_PATH",
            "updater.install requires a `path` (downloaded bundle)",
        ));
    }
    let current = current_app_path().ok_or_else(|| {
        KitronError::code("UPDATER_NO_APP", "could not resolve the running .app bundle")
    })?;

    // Stage a detached helper that waits for this process to exit, then swaps
    // the bundle and relaunches. Avoids overwriting a running executable.
    let script = updates_dir().join("install.sh");
    let _ = fs::create_dir_all(updates_dir());
    let contents = format!(
        "#!/bin/sh\nsleep 2\nrm -rf \"{current}\"\ncp -R \"{bundle}\" \"{current}\"\nopen \"{current}\"\n",
        current = current.display(),
        bundle = bundle_path,
    );
    fs::write(&script, contents)
        .map_err(|e| KitronError::code("UPDATER_IO", e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&script, fs::Permissions::from_mode(0o755));
    }
    Command::new("sh")
        .arg(&script)
        .spawn()
        .map_err(|e| KitronError::code("UPDATER_INSTALL_FAILED", e.to_string()))?;

    Ok(serde_json::json!({
        "scheduled": true,
        "target": current.to_string_lossy(),
    }))
}

fn version_gt(a: &str, b: &str) -> bool {
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