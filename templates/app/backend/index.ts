/**
 * Backend entry — runs inside the embedded Node.js host (spec §28-§30).
 *
 * Handlers registered here are invokable from the UI through
 * `backend.invoke(method, payload)`; events pushed with `backend.emit`
 * arrive in the UI as `app.on("backend.event", …)`.
 */

import { backend, logger } from "kitron";

backend.onInvoke("math.add", (payload) => {
  const a = Number((payload as { a?: unknown }).a ?? 0);
  const b = Number((payload as { b?: unknown }).b ?? 0);
  return { sum: a + b };
});

backend.onInvoke("users.get", async (payload) => {
  const id = (payload as { id?: unknown }).id;
  await new Promise((resolve) => setTimeout(resolve, 20));
  if (id === 404) {
    const error = new Error(`no user ${id}`);
    error.code = "USER_NOT_FOUND";
    throw error;
  }
  return { id, name: "Ada Lovelace" };
});

// Unary streaming example: chunked progress via events.
backend.onInvoke("report.run", (payload) => {
  const steps = Number((payload as { steps?: unknown }).steps ?? 3);
  for (let step = 0; step < steps; step += 1) {
    backend.emit("report.progress", { step, of: steps });
  }
  return { done: true };
});

// Worker support (spec §43): spawn Node worker_threads and bridge their
// messages to the UI as `worker.message` / `worker.error` / `worker.exit`.
import { Worker } from "node:worker_threads";

const _workers = new Map<number, Worker>();
let _nextWorkerId = 1;

backend.onInvoke("worker.spawn", (payload) => {
  const entry = String((payload as { entry?: unknown }).entry ?? "");
  if (!entry) throw new Error("worker.spawn requires an `entry` path");
  const id = _nextWorkerId++;
  const w = new Worker(entry, { execArgv: ["--experimental-strip-types"] });
  w.on("message", (data) => backend.emit("worker.message", { id, data }));
  w.on("error", (e) => backend.emit("worker.error", { id, message: String(e) }));
  w.on("exit", (code) => {
    backend.emit("worker.exit", { id, code });
    _workers.delete(id);
  });
  _workers.set(id, w);
  return { id };
});

backend.onInvoke("worker.post", (payload) => {
  const { id, message } = payload as { id: number; message: unknown };
  _workers.get(id)?.postMessage(message);
  return { ok: true };
});

backend.onInvoke("worker.terminate", (payload) => {
  const id = Number((payload as { id?: unknown }).id);
  _workers.get(id)?.terminate();
  _workers.delete(id);
  return { ok: true };
});

setTimeout(() => {
  logger.info("backend ready — handlers: math.add, users.get, report.run, worker.*");
}, 0);
