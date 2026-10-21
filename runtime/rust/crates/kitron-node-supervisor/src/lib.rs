//! Embedded Node.js backend process supervisor (spec §28).
//!
//! Kitron does not reimplement Node in Rust (spec §45). Node runs as a child
//! process with Kitron's host bootstrap (`runtime/node/host.mjs`) as its
//! entrypoint; the backend is loaded by the host through Node's native
//! TypeScript support.
//!
//! Communication is NDJSON over the child's stdout (see `kitron_protocol::wire`).
//! The supervisor is deliberately cheap: one reader thread, one poll thread,
//! mutex-serialized stdin writes. Invoke requests correlate by id and resolve
//! on the reader thread.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use kitron_protocol::{
    ErrorFrame, EventFrame, Frame, HelloFrame, InvokeFrame, KitronError, LogLevel, LogFrame,
    ReadyFrame, ResultFrame, ShutdownFrame,
};

/// Node binary discovery: explicit env override first, then PATH fallback.
pub fn node_binary_path() -> String {
    if let Ok(path) = std::env::var("KITRON_NODE_PATH") {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "node".to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodePhase {
    Stopped,
    Starting,
    Ready,
    Crashed,
    Stopping,
}

impl NodePhase {
    pub fn as_str(self) -> &'static str {
        match self {
            NodePhase::Stopped => "stopped",
            NodePhase::Starting => "starting",
            NodePhase::Ready => "ready",
            NodePhase::Crashed => "crashed",
            NodePhase::Stopping => "stopping",
        }
    }
}

/// Snapshot shape surfaced to JS (serialized to JSON by the bridge).
#[derive(Debug, Clone, Serialize)]
pub struct NodeStatus {
    pub phase: &'static str,
    pub pid: Option<u32>,
    pub entry: Option<String>,
    pub handlers: Option<u32>,
    pub last_error: Option<KitronError>,
}

/// Configuration used to start a Node host process.
#[derive(Debug, Clone)]
pub struct SpawnOptions {
    /// Absolute node binary path (`KITRON_NODE_PATH` or PATH fallback).
    pub node_path: String,
    /// Absolute path to the Kitron node host bootstrap script.
    pub host_script: String,
    /// Absolute path to the app's backend entry (TypeScript supported).
    pub entry: String,
    /// Working directory for the child process.
    pub workdir: String,
    /// Extra environment variables.
    pub env: HashMap<String, String>,
}

/// Observations relayed to the runtime core (consumed by its relayer thread).
#[derive(Debug)]
pub enum NodeEvent {
    /// The Node host reported its boot parameters.
    Hello { pid: u32, entry: String, host_version: String },
    /// The backend finished booting and serves handlers.
    Ready { handlers: u32 },
    /// Unified log line from the Node side (spec §34).
    Log { level: LogLevel, message: String },
    /// Backend-emitted event (`backend.event` for JS consumers).
    Event { name: String, payload: Option<serde_json::Value> },
    /// Node exited.
    Exited { code: i32, signal: Option<i32> },
}

pub type KitronResult = Result<serde_json::Value, KitronError>;
/// Reply channel for [`NodeSupervisor::invoke`].
pub type InvokeReply = std::sync::mpsc::Receiver<KitronResult>;

struct Shared {
    phase: Mutex<NodePhase>,
    pending: Mutex<HashMap<u64, std::sync::mpsc::Sender<KitronResult>>>,
    status_meta: Mutex<StatusMeta>,
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    stopping: AtomicBool,
}

#[derive(Default, Clone)]
struct StatusMeta {
    pid: Option<u32>,
    entry: Option<String>,
    handlers: Option<u32>,
    last_error: Option<KitronError>,
}

/// Node supervisor (callers wrap in `Arc`).
pub struct NodeSupervisor {
    shared: Arc<Shared>,
    next_id: AtomicU64,
    events: Arc<EventChannels>,
}

struct EventChannels {
    tx: Mutex<std::sync::mpsc::Sender<NodeEvent>>,
    rx: Mutex<Option<std::sync::mpsc::Receiver<NodeEvent>>>,
}

impl NodeSupervisor {
    /// Spawns the Node host process and starts its supervision threads.
    pub fn spawn(opts: &SpawnOptions) -> Result<NodeSupervisor, KitronError> {
        let mut cmd = Command::new(&opts.node_path);
        cmd.arg(&opts.host_script)
            .arg(&opts.entry)
            .current_dir(&opts.workdir)
            .env("KITRON_NODE_HOST", "1");
        for (key, value) in &opts.env {
            cmd.env(key, value);
        }
        cmd.stdout(Stdio::piped())
            .stdin(Stdio::piped())
            .stderr(Stdio::inherit());

        let mut child =
            cmd.spawn().map_err(|e| {
                KitronError::code(
                    "NODE_SPAWN_FAILED",
                    format!("failed to spawn node backend ({e})"),
                )
            })?;

        let Some(stdout) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(KitronError::code("NODE_SPAWN_FAILED", "node stdout pipe missing"));
        };
        let Some(stdin) = child.stdin.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(KitronError::code("NODE_SPAWN_FAILED", "node stdin pipe missing"));
        };

        let pid = child.id();
        let (events_tx, events_rx) = std::sync::mpsc::channel::<NodeEvent>();

        let supervisor = Self {
            shared: Arc::new(Shared {
                phase: Mutex::new(NodePhase::Starting),
                pending: Mutex::new(HashMap::new()),
                status_meta: Mutex::new(StatusMeta {
                    pid: Some(pid),
                    entry: Some(opts.entry.clone()),
                    handlers: None,
                    last_error: None,
                }),
                stdin: Mutex::new(Some(stdin)),
                child: Mutex::new(Some(child)),
                stopping: AtomicBool::new(false),
            }),
            next_id: AtomicU64::new(1),
            events: Arc::new(EventChannels {
                tx: Mutex::new(events_tx),
                rx: Mutex::new(Some(events_rx)),
            }),
        };

        let shared_reader = supervisor.shared.clone();
        let tx_reader = {
            let lock = supervisor.events.tx.lock().unwrap();
            lock.clone()
        };
        thread::Builder::new()
            .name("kitron-node-reader".to_string())
            .spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines().map_while(Result::ok) {
                    if line.trim().is_empty() {
                        continue;
                    }
                    match Frame::from_line(&line) {
                        Ok(frame) => handle_frame(&shared_reader, &tx_reader, frame),
                        Err(error) => {
                            eprintln!("[warn] [kitron:node] malformed frame: {error}");
                        }
                    }
                }
                // Child closed stdout: fail outstanding invokes here; the poll
                // thread records the final status.
                fail_all_pending(&shared_reader, "NODE_EXITED", "node closed stdout");
            })
            .map_err(|e| KitronError::code("NODE_SUPERVISOR_ERROR", e.to_string()))?;

        let shared_poll = supervisor.shared.clone();
        let tx_poll = {
            let lock = supervisor.events.tx.lock().unwrap();
            lock.clone()
        };
        thread::Builder::new()
            .name("kitron-node-poll".to_string())
            .spawn(move || loop {
                // Grace period for shutting-down children, then hard kill.
                if shared_poll.stopping.load(Ordering::SeqCst)
                    && shared_poll.child.lock().unwrap().is_some()
                {
                    thread::sleep(Duration::from_millis(1200));
                    if let Some(child) = shared_poll.child.lock().unwrap().as_mut() {
                        let _ = child.kill();
                        shared_poll.stopping.store(false, Ordering::SeqCst);
                        fail_all_pending(
                            &shared_poll,
                            "NODE_EXITED",
                            "node backend was force-killed",
                        );
                    }
                }
                let mut child_lock = shared_poll.child.lock().unwrap();
                let Some(child) = child_lock.as_mut() else { return };
                match child.try_wait() {
                    Ok(Some(status)) => {
                        let code_raw = status.code();
                        drop(child_lock);
                        set_phase(&shared_poll, NodePhase::Stopped);
                        fail_all_pending(&shared_poll, "NODE_EXITED", "node backend exited");
                        let _ = tx_poll.send(NodeEvent::Exited {
                            code: code_raw.unwrap_or(-1),
                            signal: exit_signal(&status),
                        });
                        return;
                    }
                    Ok(None) => {
                        drop(child_lock);
                        thread::sleep(Duration::from_millis(150));
                    }
                    Err(e) => {
                        drop(child_lock);
                        set_phase(&shared_poll, NodePhase::Crashed);
                        eprintln!("[error] [kitron:node] wait failed: {e}");
                        return;
                    }
                }
            })
            .map_err(|e| KitronError::code("NODE_SUPERVISOR_ERROR", e.to_string()))?;

        Ok(supervisor)
    }

    /// Hands over the event receiver. Exactly one consumer (the core relayer).
    pub fn take_event_receiver(&self) -> Option<std::sync::mpsc::Receiver<NodeEvent>> {
        self.events.rx.lock().unwrap().take()
    }

    /// Current status snapshot for JSI sync reads.
    pub fn status(&self) -> NodeStatus {
        let phase = self.shared.phase.lock().unwrap();
        let meta = self.shared.status_meta.lock().unwrap();
        NodeStatus {
            phase: phase.as_str(),
            pid: meta.pid,
            entry: meta.entry.clone(),
            handlers: meta.handlers,
            last_error: meta.last_error.clone(),
        }
    }

    /// Invokes a backend handler and returns a reply receiver (spec §30).
    pub fn invoke(
        &self,
        method: &str,
        payload: Option<serde_json::Value>,
    ) -> Result<InvokeReply, KitronError> {
        let phase = *self.shared.phase.lock().unwrap();
        if phase != NodePhase::Ready {
            return Err(KitronError::code(
                "NODE_NOT_READY",
                format!("backend not ready (phase: {})", phase.as_str()),
            ));
        }
        if self.shared.stopping.load(Ordering::SeqCst) {
            return Err(KitronError::code("NODE_STOPPING", "backend is shutting down"));
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = std::sync::mpsc::channel();
        self.shared.pending.lock().unwrap().insert(id, tx);
        let frame = Frame::Invoke(InvokeFrame {
            id,
            method: method.to_string(),
            payload,
        });
        self.write_frame(&frame);
        Ok(rx)
    }

    /// Convenience: block on an invoke with a timeout.
    pub fn invoke_wait(
        &self,
        method: &str,
        payload: Option<serde_json::Value>,
        timeout: Duration,
    ) -> Result<serde_json::Value, KitronError> {
        let rx = self.invoke(method, payload)?;
        match rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(KitronError::code(
                "NODE_TIMEOUT",
                format!("backend call '{method}' timed out after {timeout:?}"),
            )),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                Err(KitronError::code("NODE_DROPPED", "backend reply channel closed"))
            }
        }
    }

    /// Requests a graceful shutdown, with a hard-kill fallback (spec §16).
    pub fn stop(&self, reason: &str) {
        if self.shared.stopping.swap(true, Ordering::SeqCst) {
            return;
        }
        set_phase(&self.shared, NodePhase::Stopping);
        // Fail outstanding invokes immediately — the bridge surfaces the error.
        fail_all_pending(&self.shared, "NODE_STOPPING", "backend requested to stop");
        self.write_frame(&Frame::Shutdown(ShutdownFrame {
            reason: Some(reason.to_string()),
        }));
    }

    fn write_frame(&self, frame: &Frame) {
        let mut guard = self.shared.stdin.lock().unwrap();
        match guard.as_mut() {
            Some(stdin) => {
                if let Err(e) = stdin
                    .write_all(frame.to_line().as_bytes())
                    .and_then(|()| stdin.flush())
                {
                    eprintln!("[warn] [kitron:node] write failed: {e}");
                }
            }
            None => eprintln!("[warn] [kitron:node] backend stdin already closed"),
        }
    }
}

fn set_phase(shared: &Arc<Shared>, phase: NodePhase) {
    *shared.phase.lock().unwrap() = phase;
}

/// Signal that terminated a child, if any (unix).
fn exit_signal(status: &std::process::ExitStatus) -> Option<i32> {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.signal()
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// Fails and removes every outstanding invoke request.
fn fail_all_pending(shared: &Arc<Shared>, code: &str, message: &str) {
    let mut pending = shared.pending.lock().unwrap();
    let ids: Vec<u64> = pending.keys().copied().collect();
    for id in ids {
        if let Some(tx) = pending.remove(&id) {
            let _ = tx.send(Err(KitronError::code(code, message)));
        }
    }
}

/// Applies a frame from the child process to shared state and events.
fn handle_frame(shared: &Arc<Shared>, tx: &std::sync::mpsc::Sender<NodeEvent>, frame: Frame) {
    match frame {
        Frame::Hello(HelloFrame { pid, entry, host_version, .. }) => {
            {
                let mut meta = shared.status_meta.lock().unwrap();
                meta.pid = Some(pid);
                meta.entry = Some(entry.clone());
            }
            let _ = tx.send(NodeEvent::Hello { pid, entry, host_version });
        }
        Frame::Ready(ReadyFrame { handlers }) => {
            shared.status_meta.lock().unwrap().handlers = Some(handlers);
            set_phase(shared, NodePhase::Ready);
            let _ = tx.send(NodeEvent::Ready { handlers });
        }
        Frame::Result(ResultFrame { id, result }) => {
            if let Some(sender) = shared.pending.lock().unwrap().remove(&id) {
                let _ = sender.send(Ok(result));
            }
        }
        Frame::Error(ErrorFrame { id, code, message, details }) => {
            if let Some(sender) = shared.pending.lock().unwrap().remove(&id) {
                let mut error = KitronError::code(code, message);
                error.details = details;
                let _ = sender.send(Err(error));
            }
        }
        Frame::Log(LogFrame { level, message }) => {
            let _ = tx.send(NodeEvent::Log { level, message });
        }
        Frame::Event(EventFrame { name, payload }) => {
            let _ = tx.send(NodeEvent::Event { name, payload });
        }
        Frame::Shutdown(_) => {} // Node never sends this frame
        Frame::Invoke(_) => {}   // Node → JS invoke is phase 3 (skipped in MVP)
    }
}

impl Drop for NodeSupervisor {
    fn drop(&mut self) {
        if !self.shared.stopping.load(Ordering::SeqCst) {
            self.stop("supervisor-drop");
        }
    }
}
