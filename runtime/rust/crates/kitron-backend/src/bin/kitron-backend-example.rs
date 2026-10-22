//! Reference Kitron backend written in Rust.
//!
//! Doubles as the protocol fixture for the crate's integration tests: it
//! implements the same handlers as the TypeScript reference backend, so the
//! runtime behaves identically whichever language an app picks.
//!
//! Run it: `cargo run -p kitron-backend --bin kitron-backend-example`

use kitron_backend::prelude::*;
use serde_json::Value;

fn main() -> std::io::Result<()> {
    let mut backend = Backend::new();

    backend.on_invoke("math.add", |payload| {
        let number = |key: &str| {
            payload
                .get(key)
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0)
        };
        Ok(serde_json::json!({ "sum": number("a") + number("b") }))
    });

    backend.on_invoke("users.get", |payload| {
        let id = payload.get("id").cloned().unwrap_or(Value::Null);
        if id == serde_json::json!(404) {
            return Err(HandlerError::new("USER_NOT_FOUND", "no such user"));
        }
        Ok(serde_json::json!({ "id": id, "name": "Ada" }))
    });

    backend.on_invoke("echo.throw", |_payload| {
        Err(HandlerError::new("NODE_HANDLER_ERROR", "handler exploded"))
    });

    backend.on_invoke("backend.event", |_payload| {
        // Push an event to the UI, then acknowledge.
        backend_handle().emit(
            "backend.event",
            serde_json::json!({ "message": "hello from rust" }),
        );
        Ok(serde_json::json!({ "emitted": true }))
    });

    backend.run()
}

/// The handle is created after the backend exists; the closure captures it
/// lazily through a thread-local-free global.
fn backend_handle() -> BackendHandle {
    Backend::current().expect("backend is running")
}
