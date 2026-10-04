//! Crash reporting (Phase 4).
//!
//! `init()` installs a panic hook that persists a JSON report per panic to
//! `<temp>/kitron-crashes/` (never blocks shutdown, never exfiltrates).
//! Reports are listed/read/cleared through the typed `crash.*` IPC domain
//! so `kitron doctor` and app UIs can surface them. Upload/aggregation is
//! intentionally out of scope — reports stay on-device by default.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

static HOOK_INSTALLED: AtomicBool = AtomicBool::new(false);

fn crash_dir() -> PathBuf {
    std::env::temp_dir().join("kitron-crashes")
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CrashReport {
    pub id: String,
    pub time: String,
    pub app_version: String,
    pub platform: String,
    pub message: String,
    pub location: Option<String>,
}

/// Install the panic hook. Idempotent; safe to call on every boot.
pub fn init(app_version: String) {
    if HOOK_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let version = app_version;
    std::panic::set_hook(Box::new(move |info| {
        let message = info.payload_as_str().unwrap_or("unknown panic").to_string();
        let location = info.location().map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let report = CrashReport {
            id: uuid::Uuid::new_v4().to_string(),
            time: chrono::Utc::now().to_rfc3339(),
            app_version: version.clone(),
            platform: kitron_core::current_platform().into(),
            message,
            location,
        };
        if let Err(e) = persist(&report) {
            eprintln!("[kitron] crash report persist failed: {e}");
        }
        eprintln!("[kitron] panic captured in crash report {}", report.id);
    }));
}

fn persist(report: &CrashReport) -> Result<PathBuf, String> {
    let dir = crash_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{}.json", report.id));
    let body = serde_json::to_string_pretty(report).map_err(|e| e.to_string())?;
    std::fs::write(&path, body).map_err(|e| e.to_string())?;
    Ok(path)
}

pub struct CrashStore;

impl CrashStore {
    pub fn new() -> Self {
        Self
    }

    pub fn list(&self) -> Result<Vec<CrashReport>, String> {
        let dir = crash_dir();
        let mut out = Vec::new();
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => return Ok(vec![]),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(body) = std::fs::read_to_string(&path) {
                if let Ok(report) = serde_json::from_str::<CrashReport>(&body) {
                    out.push(report);
                }
            }
        }
        out.sort_by(|a, b| b.time.cmp(&a.time));
        Ok(out)
    }

    pub fn get(&self, id: &str) -> Result<CrashReport, String> {
        if id.contains('/') || id.contains('\\') || id.contains('.') {
            return Err("CRASH_NOT_FOUND".into());
        }
        let path = crash_dir().join(format!("{id}.json"));
        let body = std::fs::read_to_string(&path).map_err(|_| "CRASH_NOT_FOUND".to_string())?;
        serde_json::from_str(&body).map_err(|_| "CRASH_NOT_FOUND".to_string())
    }

    pub fn clear(&self) -> Result<usize, String> {
        let dir = crash_dir();
        let mut n = 0;
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if std::fs::remove_file(&path).is_ok() {
                        n += 1;
                    }
                }
            }
        }
        Ok(n)
    }
}

impl Default for CrashStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: these run serially in one test: the store is a shared temp dir
    // and parallel tests would race on clear().
    #[test]
    fn store_lifecycle() {
        let store = CrashStore::new();
        let _ = store.clear();
        assert!(store.list().unwrap().is_empty());
        let report = CrashReport {
            id: "test-id".into(),
            time: "2026-01-01T00:00:00Z".into(),
            app_version: "0.1.0".into(),
            platform: "test".into(),
            message: "boom".into(),
            location: None,
        };
        persist(&report).unwrap();
        let back = store.get("test-id").unwrap();
        assert_eq!(back.message, "boom");
        assert_eq!(store.list().unwrap().len(), 1);
        assert!(store.get("nope").is_err());
        assert!(store.get("../evil").is_err());
        assert_eq!(store.clear().unwrap(), 1);
        assert!(store.list().unwrap().is_empty());
    }
}
