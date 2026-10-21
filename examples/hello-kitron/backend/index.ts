/**
 * Example backend — runs inside the embedded Node.js host (spec §28-§30).
 *
 * The UI calls these handlers with `backend.invoke(method, payload)`; events
 * pushed with `backend.emit` arrive in the UI as `app.on(name, …)`.
 */

import { backend, logger } from "kitron";

backend.onInvoke("math.add", (payload) => {
  const a = Number((payload as { a?: unknown }).a ?? 0);
  const b = Number((payload as { b?: unknown }).b ?? 0);
  return { sum: a + b };
});

backend.onInvoke("report.run", (payload) => {
  const steps = Number((payload as { steps?: unknown }).steps ?? 3);
  for (let step = 0; step < steps; step += 1) {
    backend.emit("report.progress", { step, of: steps });
  }
  return { done: true };
});

backend.onInvoke("ping", () => ({ pong: true, at: Date.now() }));

setTimeout(() => {
  backend.emit("backend.event", { message: "backend is alive" });
  logger.info("example backend ready");
}, 0);
