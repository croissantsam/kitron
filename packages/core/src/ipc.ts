import type { IPCInvokeOptions } from "@kitron/types";
import { getPeer } from "./transport.js";

/**
 * Node-side IPC registration — §12.
 *
 * ```ts
 * import { ipc } from "@kitron/core/node";
 * ipc.handle("app.getVersion", async () => "1.0.0");
 * ```
 */
export const ipc = {
  handle<TPayload = unknown, TResult = unknown>(
    channel: string,
    handler: (payload: TPayload) => Promise<TResult> | TResult,
  ): () => void {
    return getPeer().handle(channel, handler);
  },

  removeHandler(channel: string): void {
    getPeer().removeHandler(channel);
  },

  invoke<T = unknown>(channel: string, payload?: unknown, opts?: IPCInvokeOptions): Promise<T> {
    return getPeer().invoke<T>(channel, payload, opts);
  },

  emit(channel: string, payload?: unknown): void {
    getPeer().emit(channel, payload);
  },

  subscribe<T = unknown>(channel: string, listener: (payload: T) => void): () => void {
    return getPeer().subscribe(channel, listener);
  },
};
