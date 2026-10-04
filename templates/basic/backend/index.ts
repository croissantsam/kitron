import { defineBackend } from "@kitron/node/backend";

// Local Node backend — started automatically by Kitron (§29).
// Keep filesystem/HTTP/crypto here; keep native OS access in Rust.
// Procedures are invoked from UI code via `backend.invoke("users.get", ...)` (§30).
defineBackend({
  "users.get": (params: { id: number }) => ({ id: params.id, name: "Ada" }),
  "ping": () => ({ pong: true }),
});

console.error("[backend] ready");
