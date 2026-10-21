//! Integration test for the Node supervisor: spawns the real embedded Node
//! host, waits for the backend handshake and round-trips an invoke (spec §30).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use kitron_node_supervisor::{NodeEvent, NodeSupervisor, SpawnOptions};
use kitron_protocol::LogLevel;

fn repo_path(relative: &str) -> PathBuf {
    // crates/kitron-node-supervisor → runtime/rust/crates → runtime/rust → runtime
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("..")
        .join(relative)
        .canonicalize()
        .expect("fixture path resolves")
}

fn wait_for_ready(supervisor: &NodeSupervisor) -> bool {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if supervisor.status().phase == "ready" {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn spawns_the_node_host_and_round_trips_an_invoke() {
    let Ok(node_path) = std::env::var("KITRON_NODE_PATH") else {
        // No Node available in this environment — nothing to assert.
        eprintln!("skipping: KITRON_NODE_PATH is not set");
        return;
    };

    let host_script = repo_path("node/host.mjs");
    let entry = repo_path("node/test-backend.mjs");
    let workdir = repo_path("node");

    let supervisor = NodeSupervisor::spawn(&SpawnOptions {
        node_path,
        host_script: host_script.to_string_lossy().to_string(),
        entry: entry.to_string_lossy().to_string(),
        workdir: workdir.to_string_lossy().to_string(),
        env: Default::default(),
    })
    .expect("node host spawns");

    assert!(
        wait_for_ready(&supervisor),
        "backend never became ready (status: {:?})",
        supervisor.status()
    );

    let result = supervisor
        .invoke_wait(
            "math.add",
            Some(serde_json::json!({ "a": 19, "b": 23 })),
            Duration::from_secs(10),
        )
        .expect("invoke resolves");
    assert_eq!(result["sum"], 42);

    // Typed error surfaces with its code (spec §33).
    let error = supervisor
        .invoke_wait(
            "users.get",
            Some(serde_json::json!({ "id": 404 })),
            Duration::from_secs(10),
        )
        .expect_err("handler error is surfaced");
    assert_eq!(error.code, "USER_NOT_FOUND");
    assert_eq!(error.platform, kitron_protocol::platform_name());

    // Unknown handler → typed error too.
    let missing = supervisor
        .invoke_wait("does.notExist", None, Duration::from_secs(10))
        .expect_err("unknown handler is an error");
    assert_eq!(missing.code, "NODE_NO_HANDLER");

    supervisor.stop("test-done");
}

#[test]
fn reports_backend_events() {
    let Ok(node_path) = std::env::var("KITRON_NODE_PATH") else {
        eprintln!("skipping: KITRON_NODE_PATH is not set");
        return;
    };

    let supervisor = NodeSupervisor::spawn(&SpawnOptions {
        node_path,
        host_script: repo_path("node/host.mjs").to_string_lossy().to_string(),
        entry: repo_path("node/test-backend.mjs").to_string_lossy().to_string(),
        workdir: repo_path("node").to_string_lossy().to_string(),
        env: Default::default(),
    })
    .expect("node host spawns");

    let receiver = supervisor.take_event_receiver().expect("event receiver");
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut saw_ready = false;
    let mut saw_log = false;
    while Instant::now() < deadline && !(saw_ready && saw_log) {
        match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(NodeEvent::Ready { handlers }) => {
                assert!(handlers > 0, "the backend registered handlers");
                saw_ready = true;
            }
            Ok(NodeEvent::Log { level, message }) => {
                assert_eq!(level, LogLevel::Info);
                assert!(message.contains("smoke backend loaded"));
                saw_log = true;
            }
            Ok(_) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    assert!(saw_ready, "the backend never reported ready");
    assert!(saw_log, "the backend log was never relayed");
    supervisor.stop("test-done");
}
