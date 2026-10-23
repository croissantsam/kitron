/**
 * `@kitron/worker` — background workers backed by Node's `worker_threads`
 * (spec §43). A worker runs a TypeScript/JavaScript entry file in a separate
 * thread and exchanges messages with the UI through the backend.
 *
 * The backend template registers the `worker.*` handlers; apps can also
 * implement their own. Gated by no special permission (uses the backend).
 */

import { events } from "@kitron/core";
import { backend } from "@kitron/node";

export interface WorkerHandle {
  id: number;
  postMessage(message: unknown): Promise<void>;
  onMessage(listener: (message: unknown) => void): () => void;
  terminate(): Promise<void>;
}

export const worker = {
  /** Spawns a worker running `entry` (path relative to app root). */
  async spawn(entry: string): Promise<WorkerHandle> {
    const result = (await backend.invoke<{ id: number }>("worker.spawn", { entry })) as {
      id: number;
    };
    const id = result.id;

    const handle: WorkerHandle = {
      id,
      async postMessage(message: unknown) {
        await backend.invoke("worker.post", { id, message });
      },
      onMessage(listener) {
        const off = events.on("backend.event", (event: unknown) => {
          const payload = (event as { payload?: unknown }).payload as
            | { name?: string; payload?: { id?: number; data?: unknown } }
            | undefined;
          if (
            payload?.name === "worker.message" &&
            payload.payload?.id === id
          ) {
            listener(payload.payload.data);
          }
        });
        return off;
      },
      async terminate() {
        await backend.invoke("worker.terminate", { id });
      },
    };
    return handle;
  },
};
