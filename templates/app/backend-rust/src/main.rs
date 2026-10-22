//! The app's backend, written in Rust.
//!
//! It runs as its own process and speaks the Kitron protocol, so the UI can
//! call it exactly like a TypeScript backend:
//!
//! ```ts
//! const result = await backend.invoke<{ sum: number }>("math.add", { a: 19, b: 23 });
//! ```
//!
//! `kitron dev` compiles this crate for you — you never run Cargo yourself.

use kitron_backend::prelude::*;

fn main() -> std::io::Result<()> {
    let mut backend = Backend::new();

    // A handler the UI can call: `backend.invoke("math.add", { a, b })`.
    backend.on_invoke("math.add", |payload| {
        let number = |key: &str| {
            payload
                .get(key)
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0)
        };
        Ok(serde_json::json!({ "sum": number("a") + number("b") }))
    });

    // A handler that fails: the UI receives a typed error
    // (`{ code: "USER_NOT_FOUND", message: "…" }`).
    backend.on_invoke("users.get", |payload| {
        let id = payload.get("id").cloned().unwrap_or(serde_json::Value::Null);
        if id == serde_json::json!(404) {
            return Err(HandlerError::new("USER_NOT_FOUND", "no such user"));
        }
        Ok(serde_json::json!({ "id": id, "name": "Ada Lovelace" }))
    });

    // Push an event to the UI: `app.on("backend.event", …)`.
    backend.on_invoke("report.run", |_payload| {
        let handle = Backend::current().expect("the backend is running");
        for step in 0..3 {
            handle.emit("report.progress", serde_json::json!({ "step": step, "of": 3 }));
        }
        Ok(serde_json::json!({ "done": true }))
    });

    backend.run()
}
