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

setTimeout(() => {
  logger.info("backend ready — handlers: math.add, users.get, report.run");
}, 0);
