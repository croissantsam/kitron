//! Node sidecar wiring between `Core` and `kitron_node_supervisor`.

use std::{collections::HashMap, sync::Arc};

use crate::{config_loader::resolve_app_path, Core, EventName};
use kitron_node_supervisor::{NodeEvent, NodeStatus, SpawnOptions};
use kitron_protocol::KitronError;

/// Builds spawn options for the embedded Node process from env + config:
/// - `KITRON_NODE_PATH`: node binary (absolute path)
/// - `KITRON_RUNTIME_NODE_DIR`: directory containing Kitron's `host.mjs`
/// - app root: `Core::root_dir`
///
/// The backend entry from the app config is resolved against the app root.
pub fn spawn_options_from_env(core: &Core, entry_rel: &str) -> SpawnOptions {
    let node_path = node_binary_path();
    let runtime_dir = std::env::var("KITRON_RUNTIME_NODE_DIR")
        .unwrap_or_else(|_| format!("{}/runtime/node", core.root_dir));
    let entry_abs = resolve_app_path(&core.root_dir, entry_rel);

    let mut env: HashMap<String, String> = HashMap::new();
    env.insert("KITRON_NODE_HOST".to_string(), "1".to_string());
    env.insert(
        "KITRON_APP_META".to_string(),
        serde_json::json!({
            "name": core.config.app.name,
            "identifier": core.config.app.identifier,
            "version": core.config.app.version,
            "dev": core.dev_mode,
        })
        .to_string(),
    );
    if core.dev_mode {
        env.insert("KITRON_DEV".to_string(), "1".to_string());
    }
    // Anything extra the platform/CLI wanted to hand to the backend.
    for (key, value) in std::env::vars() {
        if key.starts_with("KITRON_APP_ENV_") {
            env.insert(key, value);
        }
    }

    SpawnOptions {
        node_path,
        host_script: format!("{runtime_dir}/host.mjs"),
        entry: entry_abs,
        workdir: core.root_dir.clone(),
        env,
    }
}

/// Node binary discovery: explicit env override, then PATH fallbacks.
pub fn node_binary_path() -> String {
    if let Ok(value) = std::env::var("KITRON_NODE_PATH") {
        if !value.trim().is_empty() {
            return value;
        }
    }
    "node".to_string()
}

pub fn spawn_event_relayer(
    core: &Core,
    supervisor: Arc<kitron_node_supervisor::NodeSupervisor>,
) -> Result<(), KitronError> {
    let Some(receiver) = supervisor.take_event_receiver() else {
        core.logger.warn("node", "event receiver already taken");
        return Ok(());
    };
    // The relayer thread only needs the event bus and logger handles; `Core`
    // is not 'static, so we capture what we need by Arc.
    let events = core.events_handle();
    let logger = core.logger.clone();
    std::thread::Builder::new()
        .name("kitron-node-relay".to_string())
        .spawn(move || {
            for event in receiver {
                match event {
                    NodeEvent::Hello { pid, entry, .. } => {
                        logger
                            .info("node", &format!("backend registered (pid {pid}, entry {entry})"));
                    }
                    NodeEvent::Ready { handlers } => {
                        logger.info("node", &format!("backend ready ({handlers} handlers)"));
                        events.push(
                            EventName::NodeReady,
                            serde_json::json!({ "handlers": handlers }),
                        );
                    }
                    NodeEvent::Log { level, message } => {
                        match level {
                            kitron_protocol::LogLevel::Debug => logger.debug("node", &message),
                            kitron_protocol::LogLevel::Info => logger.info("node", &message),
                            kitron_protocol::LogLevel::Warn => logger.warn("node", &message),
                            kitron_protocol::LogLevel::Error => logger.error("node", &message),
                        }
                        events.push(
                            EventName::NodeLog,
                            serde_json::json!({ "level": level, "message": message }),
                        );
                    }
                    NodeEvent::Event { name, payload } => {
                        logger.info("node", &format!("event {name}"));
                        events.push(
                            EventName::NodeEvent,
                            serde_json::json!({ "name": name, "payload": payload }),
                        );
                    }
                    NodeEvent::Exited { code, signal } => {
                        logger.warn(
                            "node",
                            &format!(
                                "backend exited (code {code}{})",
                                signal.map(|s| format!(", signal {s}")).unwrap_or_default()
                            ),
                        );
                        events.push(
                            EventName::NodeExited,
                            serde_json::json!({ "code": code, "signal": signal }),
                        );
                    }
                }
            }
        })
        .map_err(|e| KitronError::code("RELAYER_SPAWN_FAILED", e.to_string()))?;
        Ok(())
}

/// Snapshot copy of the node status for JSI sync access.
pub fn node_status(core: &Core) -> Option<NodeStatus> {
    core.node_supervisor().map(|supervisor| supervisor.status())
}

/// Convenience wrapper used by the bridge for a single blocking call.
pub fn invoke_supervisor(
    core: &Core,
    method: &str,
    payload: Option<serde_json::Value>,
    timeout: std::time::Duration,
) -> Result<serde_json::Value, KitronError> {
    let supervisor = core
        .node_supervisor()
        .ok_or_else(|| KitronError::code("NODE_NOT_STARTED", "backend not started"))?;
    supervisor.invoke_wait(method, payload, timeout)
}
