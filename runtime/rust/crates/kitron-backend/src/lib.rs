//! `kitron-backend` — write a Kitron app's backend in Rust.
//!
//! The backend is a **separate process** that speaks the Kitron NDJSON
//! protocol (spec §28/§30) on its stdio. A TypeScript backend (Node) and a
//! Rust backend are interchangeable: the UI calls `backend.invoke(method,
//! payload)` either way, and the runtime supervises the process the same way.
//!
//! ```no_run
//! use kitron_backend::prelude::*;
//!
//! fn main() -> std::io::Result<()> {
//!     let mut backend = Backend::new();
//!
//!     backend.on_invoke("math.add", |payload| {
//!         let a = payload.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
//!         let b = payload.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
//!         Ok(serde_json::json!({ "sum": a + b }))
//!     });
//!
//!     backend.on_invoke("users.get", |payload| {
//!         Err(HandlerError::new("USER_NOT_FOUND", "no such user"))
//!     });
//!
//!     backend.run()
//! }
//! ```
//!
//! # stdout is the protocol channel
//!
//! Frames are written to stdout, so **`println!` would corrupt the stream**.
//! Use [`BackendHandle::log`] (or the [`info!`]/[`warn!`]/[`error!`] macros) —
//! they emit a proper log frame that the runtime forwards to the unified log.

use std::collections::HashMap;
use std::io::{self, BufRead, BufWriter, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use serde_json::Value;

use kitron_protocol::{
    ErrorFrame, EventFrame, Frame, HelloFrame, InvokeFrame, LogFrame, ReadyFrame, ResultFrame,
    ShutdownFrame, PROTOCOL_VERSION,
};

pub use kitron_protocol::LogLevel;

/// Version reported in the `hello` frame.
pub const HOST_VERSION: &str = "0.1.0";

/// Error code returned when the UI calls a method no handler registered.
pub const CODE_NO_HANDLER: &str = "NODE_NO_HANDLER";
/// Error code used when a handler fails without an explicit code.
pub const CODE_HANDLER_ERROR: &str = "NODE_HANDLER_ERROR";
/// Error code used when the runtime sends a malformed frame.
pub const CODE_BAD_FRAME: &str = "NODE_BAD_FRAME";

/// A handler failure — surfaced to the UI as a typed error (spec §33).
#[derive(Debug, Clone)]
pub struct HandlerError {
    pub code: String,
    pub message: String,
    pub details: Option<Value>,
}

impl HandlerError {
    /// A handler error with an explicit machine-readable code.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: None,
        }
    }

    /// Attaches structured details to the error.
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }
}

impl std::fmt::Display for HandlerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for HandlerError {}

/// Result of a backend handler.
pub type HandlerResult = Result<Value, HandlerError>;

type Handler = Arc<dyn Fn(Value) -> HandlerResult + Send + Sync>;

/// Writes frames to the protocol channel (stdout by default).
struct Writer {
    out: Mutex<Box<dyn Write + Send>>,
}

impl Writer {
    fn send(&self, frame: &Frame) {
        let Ok(mut out) = self.out.lock() else {
            return;
        };
        if let Ok(line) = serde_json::to_string(frame) {
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
        }
    }

    /// Swaps the protocol channel (used by the tests to drive the loop over
    /// in-memory streams).
    fn replace(&self, writer: Box<dyn Write + Send>) {
        if let Ok(mut out) = self.out.lock() {
            *out = writer;
        }
    }
}

/// A cloneable handle for emitting events and logs from anywhere — including
/// background threads started by the app.
#[derive(Clone)]
pub struct BackendHandle {
    writer: Arc<Writer>,
}

impl BackendHandle {
    /// Pushes a one-way event to the app (`app.on("backend.event", …)`).
    pub fn emit(&self, name: &str, payload: Value) {
        self.writer.send(&Frame::Event(EventFrame {
            name: name.to_string(),
            payload: Some(payload),
        }));
    }

    /// Sends a log line to the unified log stream (spec §34).
    pub fn log(&self, level: LogLevel, message: &str) {
        self.writer.send(&Frame::Log(LogFrame {
            level,
            message: message.to_string(),
        }));
    }

    /// Convenience: an info-level log line.
    pub fn info(&self, message: &str) {
        self.log(LogLevel::Info, message);
    }

    /// Convenience: a warning-level log line.
    pub fn warn(&self, message: &str) {
        self.log(LogLevel::Warn, message);
    }

    /// Convenience: an error-level log line.
    pub fn error(&self, message: &str) {
        self.log(LogLevel::Error, message);
    }
}

/// The backend: a set of handlers plus the protocol loop.
pub struct Backend {
    handlers: HashMap<String, Handler>,
    writer: Arc<Writer>,
    pending: Arc<AtomicUsize>,
}

/// Handle of the running backend, used by the log macros.
static CURRENT: OnceLock<BackendHandle> = OnceLock::new();

impl Default for Backend {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend {
    /// The handle of the running backend — used by the [`info!`]/[`warn!`]/[`error!`]
    /// macros.
    pub fn current() -> Option<BackendHandle> {
        CURRENT.get().cloned()
    }
    /// Creates an empty backend.
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
            writer: Arc::new(Writer {
                out: Mutex::new(Box::new(BufWriter::new(io::stdout()))),
            }),
            pending: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Registers a handler for `method`. The UI calls it with
    /// `backend.invoke(method, payload)`.
    pub fn on_invoke<F>(&mut self, method: &str, handler: F) -> &mut Self
    where
        F: Fn(Value) -> HandlerResult + Send + Sync + 'static,
    {
        self.handlers
            .insert(method.to_string(), Arc::new(handler));
        self
    }

    /// Number of registered handlers.
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// A handle for emitting events and logs (usable from other threads).
    pub fn handle(&self) -> BackendHandle {
        BackendHandle {
            writer: self.writer.clone(),
        }
    }

    /// Announces the backend and serves the protocol loop until the runtime
    /// asks it to stop.
    pub fn run(mut self) -> io::Result<()> {
        self.serve(io::BufReader::new(io::stdin()), Box::new(io::stdout()))
    }

    /// The protocol loop over arbitrary streams (used by the tests).
    pub fn serve<R: BufRead>(
        &mut self,
        reader: R,
        writer: Box<dyn Write + Send>,
    ) -> io::Result<()> {
        self.writer.replace(writer);
        let entry = std::env::args().nth(1).unwrap_or_default();
        let _ = CURRENT.set(self.handle());

        // Handshake: hello → ready (mirrors the Node host).
        self.writer.send(&Frame::Hello(HelloFrame {
            protocol: PROTOCOL_VERSION,
            pid: std::process::id(),
            host_version: HOST_VERSION.to_string(),
            entry,
        }));
        self.writer.send(&Frame::Ready(ReadyFrame {
            handlers: self.handlers.len() as u32,
        }));

        for line in reader.lines() {
            let line = match line {
                Ok(line) => line,
                Err(error) => {
                    self.writer.send(&Frame::Log(LogFrame {
                        level: LogLevel::Error,
                        message: format!("failed to read frame: {error}"),
                    }));
                    break;
                }
            };
            if line.trim().is_empty() {
                continue;
            }
            let frame: Frame = match serde_json::from_str(&line) {
                Ok(frame) => frame,
                Err(_) => {
                    self.writer.send(&Frame::Log(LogFrame {
                        level: LogLevel::Warn,
                        message: format!("malformed frame: {}", truncate(&line, 120)),
                    }));
                    continue;
                }
            };
            match frame {
                Frame::Invoke(invoke) => self.handle_invoke(invoke),
                Frame::Shutdown(ShutdownFrame { reason }) => {
                    self.shutdown(reason);
                    break;
                }
                other => {
                    self.writer.send(&Frame::Log(LogFrame {
                        level: LogLevel::Debug,
                        message: format!("ignored frame: {}", frame_type(&other)),
                    }));
                }
            }
        }
        Ok(())
    }

    /// Runs one invoke: dispatches to the handler and writes the reply.
    fn handle_invoke(&mut self, invoke: InvokeFrame) {
        let Some(handler) = self.handlers.get(&invoke.method).cloned() else {
            self.writer.send(&Frame::Error(ErrorFrame {
                id: invoke.id,
                code: CODE_NO_HANDLER.to_string(),
                message: format!("no backend handler for \"{}\"", invoke.method),
                details: None,
            }));
            return;
        };
        let writer = self.writer.clone();
        let pending = self.pending.clone();
        pending.fetch_add(1, Ordering::SeqCst);
        std::thread::spawn(move || {
            let payload = invoke.payload.unwrap_or(Value::Null);
            let reply = match handler(payload) {
                Ok(result) => Frame::Result(ResultFrame {
                    id: invoke.id,
                    result,
                }),
                Err(error) => Frame::Error(ErrorFrame {
                    id: invoke.id,
                    code: error.code,
                    message: error.message,
                    details: error.details,
                }),
            };
            writer.send(&reply);
            pending.fetch_sub(1, Ordering::SeqCst);
        });
    }

    /// Waits for the in-flight handlers, then returns.
    fn shutdown(&self, reason: Option<String>) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
        while self.pending.load(Ordering::SeqCst) > 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        self.writer.send(&Frame::Log(LogFrame {
            level: LogLevel::Info,
            message: format!("backend shutting down ({})", reason.unwrap_or_default()),
        }));
    }
}

fn frame_type(frame: &Frame) -> &'static str {
    match frame {
        Frame::Hello(_) => "hello",
        Frame::Ready(_) => "ready",
        Frame::Invoke(_) => "invoke",
        Frame::Result(_) => "result",
        Frame::Error(_) => "error",
        Frame::Log(_) => "log",
        Frame::Event(_) => "event",
        Frame::Shutdown(_) => "shutdown",
    }
}

fn truncate(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        text.to_string()
    } else {
        format!("{}…", &text[..limit])
    }
}

/// The prelude an app backend imports.
pub mod prelude {
    pub use crate::{
        info, warn, error, Backend, BackendHandle, HandlerError, HandlerResult, LogLevel,
    };
}

/// Emits an info-level log frame (never use `println!` — stdout is the
/// protocol channel).
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::Backend::current().map(|backend| backend.info(&format!($($arg)*)))
    };
}

/// Emits a warning-level log frame.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::Backend::current().map(|backend| backend.warn(&format!($($arg)*)))
    };
}

/// Emits an error-level log frame.
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::Backend::current().map(|backend| backend.error(&format!($($arg)*)))
    };
}
