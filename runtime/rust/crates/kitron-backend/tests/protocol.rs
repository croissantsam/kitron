//! Protocol test for the Rust backend: the crate's reference binary must
//! behave exactly like the TypeScript reference backend (spec §30).

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;

const BINARY: &str = env!("CARGO_BIN_EXE_kitron-backend-example");

/// Spawns the reference backend and returns a handle to its stdio.
fn spawn_backend() -> (
    std::process::Child,
    impl BufRead,
    impl Write,
) {
    let mut child = Command::new(BINARY)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("backend binary spawns");

    let stdout = child.stdout.take().expect("stdout pipe");
    let stdin = child.stdin.take().expect("stdin pipe");
    (child, BufReader::new(stdout), stdin)
}

fn read_frame(reader: &mut impl BufRead) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).expect("frame read");
    serde_json::from_str(line.trim()).expect("frame is JSON")
}

fn send(writer: &mut impl Write, frame: Value) {
    writeln!(writer, "{frame}").expect("frame write");
    writer.flush().expect("flush");
}

#[test]
fn speaks_the_kitron_protocol() {
    let (mut child, mut reader, mut writer) = spawn_backend();

    // 1. Handshake: hello → ready.
    let hello = read_frame(&mut reader);
    assert_eq!(hello["type"], "hello");
    assert_eq!(hello["protocol"], 1);

    let ready = read_frame(&mut reader);
    assert_eq!(ready["type"], "ready");
    assert_eq!(ready["handlers"], 4);

    // 2. Successful invoke.
    send(
        &mut writer,
        serde_json::json!({ "type": "invoke", "id": 1, "method": "math.add", "payload": { "a": 19, "b": 23 } }),
    );
    let result = read_frame(&mut reader);
    assert_eq!(result["type"], "result");
    assert_eq!(result["id"], 1);
    assert_eq!(result["result"]["sum"].as_f64(), Some(42.0));

    // 3. Async-ish invoke.
    send(
        &mut writer,
        serde_json::json!({ "type": "invoke", "id": 2, "method": "users.get", "payload": { "id": 7 } }),
    );
    let user = read_frame(&mut reader);
    assert_eq!(user["type"], "result");
    assert_eq!(user["result"]["name"], "Ada");

    // 4. Typed handler error.
    send(
        &mut writer,
        serde_json::json!({ "type": "invoke", "id": 3, "method": "users.get", "payload": { "id": 404 } }),
    );
    let not_found = read_frame(&mut reader);
    assert_eq!(not_found["type"], "error");
    assert_eq!(not_found["code"], "USER_NOT_FOUND");

    // 5. Unknown handler.
    send(
        &mut writer,
        serde_json::json!({ "type": "invoke", "id": 4, "method": "does.notExist" }),
    );
    let missing = read_frame(&mut reader);
    assert_eq!(missing["type"], "error");
    assert_eq!(missing["code"], "NODE_NO_HANDLER");

    // 6. Thrown handler.
    send(
        &mut writer,
        serde_json::json!({ "type": "invoke", "id": 5, "method": "echo.throw" }),
    );
    let thrown = read_frame(&mut reader);
    assert_eq!(thrown["type"], "error");
    assert_eq!(thrown["code"], "NODE_HANDLER_ERROR");

    // 7. Events pushed by the backend.
    send(
        &mut writer,
        serde_json::json!({ "type": "invoke", "id": 6, "method": "backend.event" }),
    );
    let mut saw_event = false;
    let mut saw_result = false;
    for _ in 0..4 {
        let frame = read_frame(&mut reader);
        if frame["type"] == "event" {
            assert_eq!(frame["name"], "backend.event");
            saw_event = true;
        }
        if frame["type"] == "result" && frame["id"] == 6 {
            saw_result = true;
        }
        if saw_event && saw_result {
            break;
        }
    }
    assert!(saw_event, "the backend event was not delivered");
    assert!(saw_result, "the invoke reply was not delivered");

    // 8. Graceful shutdown.
    send(
        &mut writer,
        serde_json::json!({ "type": "shutdown", "reason": "test-done" }),
    );
    let status = child.wait().expect("backend exits");
    assert_eq!(status.code(), Some(0));
}

/// The runtime supervisor must be able to drive a Rust backend exactly like a
/// Node one — that is what makes the backend language a free choice.
#[test]
fn the_runtime_supervisor_drives_the_rust_backend() {
    use kitron_node_supervisor::{NodeSupervisor, SpawnOptions};

    let supervisor = NodeSupervisor::spawn(&SpawnOptions {
        command: BINARY.to_string(),
        args: Vec::new(),
        entry: BINARY.to_string(),
        workdir: std::env::temp_dir().to_string_lossy().to_string(),
        env: Default::default(),
    })
    .expect("the Rust backend spawns");

    // Wait for the handshake.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while supervisor.status().phase != "ready" && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(supervisor.status().phase, "ready");
    assert_eq!(supervisor.status().handlers, Some(4));

    // A round trip through the runtime's own transport.
    let result = supervisor
        .invoke_wait(
            "math.add",
            Some(serde_json::json!({ "a": 19, "b": 23 })),
            Duration::from_secs(10),
        )
        .expect("invoke resolves");
    assert_eq!(result["sum"].as_f64(), Some(42.0));

    // Typed errors travel back like they do from a Node backend.
    let error = supervisor
        .invoke_wait(
            "users.get",
            Some(serde_json::json!({ "id": 404 })),
            Duration::from_secs(10),
        )
        .expect_err("handler error is surfaced");
    assert_eq!(error.code, "USER_NOT_FOUND");

    supervisor.stop("test-done");
}
