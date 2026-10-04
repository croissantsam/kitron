//! Process manager — thin native `spawn` for cases where
//! `node:child_process` is insufficient (§16). Prefer Node otherwise.

use std::collections::HashMap;
use tokio::process::Child;

pub struct ProcessManager {
    procs: tokio::sync::Mutex<HashMap<u32, Child>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self { procs: tokio::sync::Mutex::new(HashMap::new()) }
    }

    pub async fn spawn(
        &self,
        command: String,
        args: Vec<String>,
        cwd: Option<String>,
        env: Option<HashMap<String, String>>,
    ) -> Result<u32, String> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args);
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        if let Some(env) = env {
            cmd.envs(env);
        }
        let child = cmd.spawn().map_err(|e| format!("PROCESS_SPAWN_FAILED: {e}"))?;
        let pid = child.id().ok_or_else(|| "PROCESS_SPAWN_FAILED: no pid".to_string())?;
        self.procs.lock().await.insert(pid, child);
        Ok(pid)
    }

    pub async fn kill(&self, pid: u32) -> Result<(), String> {
        let child = self.procs.lock().await.remove(&pid).ok_or_else(|| format!("PROCESS_NOT_FOUND: {pid}"))?;
        // `Child` kills on drop only with kill_on_drop; do it explicitly.
        let mut child = child;
        child.kill().await.map_err(|e| format!("PROCESS_NOT_FOUND: {e}"))
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}
