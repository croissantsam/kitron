//! Platform-neutral Kitron runtime core (spec §40: `runtime/rust`).
//!
//! Responsibilities (spec §7):
//! - app lifecycle state hooks
//! - runtime events bus shared by platform shells and the Node supervisor
//! - unified logger (spec §34)
//! - configuration loading (spec §37)
//! - embedded Node.js backend supervision (spec §28/§29)
//!
//! Platform-specific behavior lives in shell crates (`kitron-macos` today,
//! `kitron-windows` in phase 4). They call into this crate; this crate never
//! touches AppKit/Win32 so the core stays portable and testable.

use std::sync::{Arc, Mutex, atomic::AtomicBool};

use kitron_protocol::{KitronConfig, KitronError};

pub mod config_loader;
pub mod event_bus;
pub mod logger;
pub mod node;

pub use event_bus::{EventName, RuntimeEvent};
pub use logger::SharedLogger;

/// Sink that delivers runtime events to the UI (push delivery).
pub type EventSink = Arc<dyn Fn(EventName, serde_json::Value) + Send + Sync>;

/// The runtime core instance shared between the platform shell, the JSI bridge
/// and the Node supervisor.
pub struct Core {
    pub config: KitronConfig,
    /// Absolute path the app runs from (project root in dev, bundle Resources
    /// in production).
    pub root_dir: String,
    /// `true` when launched by `kitron dev`.
    pub dev_mode: bool,
    pub logger: Arc<SharedLogger>,
    events: Arc<event_bus::EventBus>,
    /// Optional push sink: when installed, events are delivered to the UI
    /// immediately instead of waiting in the bus for a drain.
    event_sink: Mutex<Option<EventSink>>,
    node: Mutex<Option<Arc<kitron_node_supervisor::NodeSupervisor>>>,
    /// Set by the platform shell when termination has been requested
    /// (`applicationShouldTerminate`).
    pub termination_requested: AtomicBool,
    /// Set by the bridge when JS vetoed termination (spec §11 moment).
    pub termination_cancelled: AtomicBool,
}

impl Core {
    /// Builds the core from process-environment inputs provided by the CLI or
    /// the embedded bootstrap. This is the single constructor used in
    /// production and tests.
    pub fn from_env() -> Self {
        let config = config_loader::load_config_from_env();
        let root_dir = std::env::var("KITRON_ROOT")
            .or_else(|_| std::env::current_dir().map(|cwd| cwd.display().to_string()))
            .unwrap_or_else(|_| ".".to_string());
        let dev_mode = matches!(std::env::var("KITRON_DEV").as_deref(), Ok("1" | "true"));
        Self {
            logger: Arc::new(SharedLogger::new(&config.app.name)),
            config,
            root_dir,
            dev_mode,
            events: Arc::new(event_bus::EventBus::new()),
            event_sink: Mutex::new(None),
            node: Mutex::new(None),
            termination_requested: AtomicBool::new(false),
            termination_cancelled: AtomicBool::new(false),
        }
    }

    pub fn events_handle(&self) -> Arc<event_bus::EventBus> {
        self.events.clone()
    }

    pub fn events(&self) -> &event_bus::EventBus {
        &self.events
    }

    pub fn emit(&self, name: EventName, payload: serde_json::Value) {
        // A sink (installed by the platform shell) delivers the event to the UI
        // immediately — no polling. Without one, the event waits in the bus
        // and the UI drains it.
        if let Some(sink) = self.event_sink.lock().unwrap().clone() {
            sink(name, payload);
            return;
        }
        self.events.push(name, payload);
    }

    /// Installs the sink that delivers events to the UI (push delivery).
    pub fn set_event_sink(&self, sink: Option<EventSink>) {
        *self.event_sink.lock().unwrap() = sink;
    }

    /// Emits an event that belongs to a window.
    pub fn emit_windowed(
        &self,
        name: EventName,
        window_id: Option<u64>,
        payload: serde_json::Value,
    ) {
        if let Some(sink) = self.event_sink.lock().unwrap().clone() {
            sink(name, payload);
            return;
        }
        self.events.push_windowed(name, window_id, payload);
    }

    /// Permission gate for the sensitive native APIs (spec §23).
    ///
    /// Permissions are declared explicitly in `kitron.config.ts`; an app never
    /// gets the whole native surface implicitly. The check lives in the native
    /// layer so it cannot be bypassed from JavaScript.
    pub fn require_permission(&self, permission: &str) -> Result<(), KitronError> {
        if self
            .config
            .permissions
            .iter()
            .any(|granted| granted == permission)
        {
            return Ok(());
        }
        Err(KitronError::code(
            "PERMISSION_DENIED",
            format!(
                "the \"{permission}\" permission is not granted — add it to `permissions` in kitron.config.ts"
            ),
        )
        .with_details(serde_json::json!({ "permission": permission })))
    }

    /// App-lifecycle helper (spec §11): mark termination as requested.
    pub fn request_termination(&self) {
        self.termination_requested
            .store(true, std::sync::atomic::Ordering::SeqCst);
        self.emit(EventName::AppBeforeQuit, serde_json::json!({}));
    }

    /// JS vetoes a pending termination (`app.preventDefaultExit()`).
    pub fn cancel_termination(&self) {
        self.termination_cancelled
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Whether the platform shell should proceed with exiting.
    pub fn should_terminate(&self) -> bool {
        !(self.termination_cancelled.load(std::sync::atomic::Ordering::SeqCst))
    }

    // -- Backend lifecycle (spec §28) -----------------------------------------

    /// Starts (or returns the existing) backend process. The backend is
    /// either a TypeScript entry run by the embedded Node host or a compiled
    /// binary (Rust) — both speak the same protocol.
    pub fn start_node_from_config(&self) -> Result<Arc<kitron_node_supervisor::NodeSupervisor>, KitronError> {
        if let Some(sup) = self.node.lock().unwrap().clone() {
            return Ok(sup);
        }
        let backend = node::resolve_backend_config(self);
        match backend.entry.as_deref() {
            Some(entry) if !entry.is_empty() => {}
            _ => {
                return Err(KitronError::code(
                    "NODE_NOT_CONFIGURED",
                    "kitron.config.ts has no backend.entry (or node.entry)",
                ));
            }
        };
        if !backend.auto_start() {
            return Err(KitronError::code(
                "NODE_AUTO_START_DISABLED",
                "backend.autoStart is disabled in the app config",
            ));
        }

        let opts = node::spawn_options_from_config(self, &backend);
        let supervisor =
            kitron_node_supervisor::NodeSupervisor::spawn(&opts).inspect_err(|e| {
                self.logger.error("node", &format!("spawn failed: {}", e.message));
            })?;
        let supervisor = Arc::new(supervisor);
        *self.node.lock().unwrap() = Some(supervisor.clone());

        self.emit(EventName::NodeStarted, serde_json::json!({
            "pid": supervisor.status().pid,
        }));
        // Relay supervisor events into the runtime bus for the bridge pump.
        node::spawn_event_relayer(self, supervisor.clone())?;
        Ok(supervisor)
    }

    pub fn node_supervisor(&self) -> Option<Arc<kitron_node_supervisor::NodeSupervisor>> {
        self.node.lock().unwrap().clone()
    }

    pub fn stop_node(&self, reason: &str) {
        if let Some(sup) = self.node_supervisor() {
            self.emit(EventName::NodeStopping, serde_json::json!({ "reason": reason }));
            sup.stop(reason);
        }
    }
}
