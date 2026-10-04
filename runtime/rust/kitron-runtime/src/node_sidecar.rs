//! Embedded Node.js sidecar (§28-§29).
//! Kitron starts `node backend/index.js` automatically and supervises it.
//! RN ↔ Node uses Kitron IPC (`backend.invoke`), not forced HTTP localhost.
//!
//! Backend wire protocol (NDJSON over the child's stdio):
//!   Rust → Node: `{ "id": 1, "procedure": "users.get", "params": {...} }`
//!   Node → Rust: `{ "id": 1, "ok": true, "result": ... }`
//!              | `{ "id": 1, "ok": false, "error": { "code": "...", "message": "..." } }`
//!
//! A backend entrypoint handles requests with `@kitron/node/backend`
//! (`defineBackend`) — see `templates/basic/backend/index.ts`.
//! Invokes while no backend runs fail with `BACKEND_INVOKE_FAILED` (§33).

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin};
use tokio::sync::{Mutex, oneshot};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Stopped,
    Starting,
    Running,
    Failed,
}

#[derive(Debug, Clone)]
struct StartConfig {
    entry: String,
    args: Vec<String>,
    env: HashMap<String, String>,
    cwd: Option<PathBuf>,
}

struct SidecarState {
    child: Option<Child>,
    writer: Option<ChildStdin>,
    next_id: u64,
    pending: HashMap<u64, oneshot::Sender<BackendResult>>,
    status: NodeStatus,
    config: Option<StartConfig>,
}

type BackendResult = Result<serde_json::Value, (String, String)>;

pub struct NodeSidecar {
    state: Arc<Mutex<SidecarState>>,
}

impl NodeSidecar {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(SidecarState {
                child: None,
                writer: None,
                next_id: 0,
                pending: HashMap::new(),
                status: NodeStatus::Stopped,
                config: None,
            })),
        }
    }

    pub async fn status(&self) -> (NodeStatus, Option<u32>) {
        let s = self.state.lock().await;
        let pid = s.child.as_ref().and_then(|c| c.id());
        (s.status.clone(), pid)
    }

    pub async fn start(
        &self,
        entry: String,
        args: Vec<String>,
        env: HashMap<String, String>,
        cwd: Option<PathBuf>,
    ) -> Result<(), String> {
        // Stop any previous child first so `start` is idempotent.
        self.stop_inner().await;
        {
            let mut s = self.state.lock().await;
            s.status = NodeStatus::Starting;
        }
        let mut cmd = tokio::process::Command::new("node");
        cmd.arg(&entry)
            .args(&args)
            .envs(&env)
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if let Some(cwd) = &cwd {
            cmd.current_dir(cwd);
        }
        match cmd.spawn() {
            Ok(mut child) => {
                let writer = child.stdin.take();
                let stdout = child.stdout.take();
                {
                    let mut s = self.state.lock().await;
                    s.child = Some(child);
                    s.writer = writer;
                    s.status = NodeStatus::Running;
                    s.config = Some(StartConfig { entry, args, env, cwd });
                }
                if let Some(stdout) = stdout {
                    Self::spawn_reader(self.state.clone(), BufReader::new(stdout));
                }
                Ok(())
            }
            Err(e) => {
                self.state.lock().await.status = NodeStatus::Failed;
                Err(format!("NODE_START_FAILED: {e}"))
            }
        }
    }

    pub async fn stop(&self) -> Result<(), String> {
        self.stop_inner().await;
        Ok(())
    }

    async fn stop_inner(&self) {
        let child = {
            let mut s = self.state.lock().await;
            s.writer = None;
            // Fail in-flight invokes instead of hanging them.
            for (_, tx) in s.pending.drain() {
                let _ = tx.send(Err((
                    "BACKEND_INVOKE_FAILED".into(),
                    "node backend stopped".into(),
                )));
            }
            s.child.take()
        };
        if let Some(mut child) = child {
            let _ = child.kill().await;
        }
        self.state.lock().await.status = NodeStatus::Stopped;
    }

    pub async fn restart(
        &self,
        entry: Option<String>,
        args: Option<Vec<String>>,
        env: Option<HashMap<String, String>>,
    ) -> Result<(), String> {
        let cfg = self.state.lock().await.config.clone();
        let new_entry = entry
            .or_else(|| cfg.as_ref().map(|c| c.entry.clone()))
            .unwrap_or_else(|| "./backend/index.js".into());
        let new_args = args.or_else(|| cfg.as_ref().map(|c| c.args.clone())).unwrap_or_default();
        let new_env = env.or_else(|| cfg.as_ref().map(|c| c.env.clone())).unwrap_or_default();
        let cwd = cfg.and_then(|c| c.cwd.clone());
        self.start(new_entry, new_args, new_env, cwd).await
    }

    /// Forward a procedure call to the Node backend.
    pub async fn invoke(
        &self,
        procedure: String,
        params: serde_json::Value,
        timeout_ms: u64,
    ) -> Result<serde_json::Value, (String, String)> {
        if procedure.trim().is_empty() {
            return Err((
                "KITRON_INVALID_PARAMS".into(),
                "backend.invoke: procedure is required".into(),
            ));
        }
        let (id, rx) = {
            let mut s = self.state.lock().await;
            if s.writer.is_none() {
                return Err((
                    "BACKEND_INVOKE_FAILED".into(),
                    "node backend is not running (call node.start first)".into(),
                ));
            }
            s.next_id = s.next_id.wrapping_add(1).max(1);
            let id = s.next_id;
            let frame = serde_json::json!({ "id": id, "procedure": procedure, "params": params });
            let mut line = serde_json::to_string(&frame).unwrap_or_default();
            line.push('\n');
            let writer = s.writer.as_mut().expect("checked above");
            if writer.write_all(line.as_bytes()).await.is_err() {
                return Err((
                    "BACKEND_INVOKE_FAILED".into(),
                    "failed to write to node backend".into(),
                ));
            }
            let (tx, rx) = oneshot::channel();
            s.pending.insert(id, tx);
            (id, rx)
        };
        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), rx).await {
            Ok(Ok(r)) => r,
            Ok(Err(_)) => Err((
                "BACKEND_INVOKE_FAILED".into(),
                "node backend stopped".into(),
            )),
            Err(_) => {
                self.state.lock().await.pending.remove(&id);
                Err((
                    "BACKEND_INVOKE_FAILED".into(),
                    format!("backend.invoke timed out: {procedure}"),
                ))
            }
        }
    }

    fn spawn_reader(
        state: Arc<Mutex<SidecarState>>,
        mut reader: BufReader<tokio::process::ChildStdout>,
    ) {
        tokio::spawn(async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break, // EOF: child exited
                    Ok(_) => {
                        let v: serde_json::Value = match serde_json::from_str(line.trim()) {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        let id = v.get("id").and_then(|i| i.as_u64());
                        let ok = v.get("ok").and_then(|o| o.as_bool());
                        if let (Some(id), Some(ok)) = (id, ok) {
                            let tx = state.lock().await.pending.remove(&id);
                            if let Some(tx) = tx {
                                if ok {
                                    let result = v.get("result").cloned().unwrap_or(serde_json::Value::Null);
                                    let _ = tx.send(Ok(result));
                                } else {
                                    let e = v.get("error").cloned().unwrap_or(serde_json::Value::Null);
                                    let code = e
                                        .get("code")
                                        .and_then(|c| c.as_str())
                                        .unwrap_or("BACKEND_INVOKE_FAILED")
                                        .to_string();
                                    let msg = e
                                        .get("message")
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("backend procedure failed")
                                        .to_string();
                                    let _ = tx.send(Err((code, msg)));
                                }
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            // Child gone: fail everything still in flight.
            let mut s = state.lock().await;
            for (_, tx) in s.pending.drain() {
                let _ =
                    tx.send(Err(("BACKEND_INVOKE_FAILED".into(), "node backend exited".into())));
            }
            if s.status == NodeStatus::Running {
                s.status = NodeStatus::Failed;
            }
        });
    }
}

impl Default for NodeSidecar {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invoke_without_backend_fails_typed() {
        let n = NodeSidecar::new();
        let r = n.invoke("users.get".into(), serde_json::json!({}), 500).await;
        let (code, _) = r.unwrap_err();
        assert_eq!(code, "BACKEND_INVOKE_FAILED");
    }

    #[tokio::test]
    async fn invoke_roundtrip_with_fake_backend() {
        // Fake backend: a node one-liner speaking the NDJSON protocol.
        let n = NodeSidecar::new();
        let script = "const rl=require('readline').createInterface({input:process.stdin});rl.on('line',l=>{try{const m=JSON.parse(l);if(m.procedure==='echo'){console.log(JSON.stringify({id:m.id,ok:true,result:m.params}));}else{console.log(JSON.stringify({id:m.id,ok:false,error:{code:'UNKNOWN_PROCEDURE',message:'nope'}}));}}catch(e){}});";
        n.start("-e".into(), vec![script.into()], Default::default(), None).await.unwrap();
        // NOTE: `node -e script` ignores the entry-as-file path because -e
        // consumes the script; entry here IS the `-e` flag trick — start runs
        // `node <entry> <args...>`, so entry="-e", args=[script].
        let ok = n.invoke("echo".into(), serde_json::json!({"a":1}), 5000).await.unwrap();
        assert_eq!(ok, serde_json::json!({"a":1}));
        let err = n.invoke("nope".into(), serde_json::json!({}), 5000).await.unwrap_err();
        assert_eq!(err.0, "UNKNOWN_PROCEDURE");
        n.stop().await.unwrap();
    }
}
