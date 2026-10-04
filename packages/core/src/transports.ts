/**
 * @kitron/core — in-memory transports for tests and `kitron dev` fallback.
 */

import { KitronError } from "./errors.js";
import type { IpcTransport } from "./ipc.js";

export type Handler = (method: string, params: unknown) => unknown | Promise<unknown>;

/**
 * Route `domain.method` to local handlers. Used by unit tests and by the
 * CLI dev-server before the Rust binary is connected.
 */
export class InMemoryTransport implements IpcTransport {
  private handlers = new Map<string, Handler>();
  private eventListeners = new Set<(name: string, payload: unknown) => void>();

  on(domain: string, method: string, handler: Handler): this {
    this.handlers.set(`${domain}.${method}`, handler);
    return this;
  }

  onEvent(listener: (name: string, payload: unknown) => void): () => void {
    this.eventListeners.add(listener);
    return () => {
      this.eventListeners.delete(listener);
    };
  }

  emit(name: string, payload: unknown): void {
    for (const l of [...this.eventListeners]) l(name, payload);
  }

  async invoke(domain: string, method: string, params?: unknown): Promise<unknown> {
    const h = this.handlers.get(`${domain}.${method}`);
    if (!h) {
      throw new KitronError({
        code: "KITRON_NOT_IMPLEMENTED",
        message: `No handler for ${domain}.${method} (in-memory transport)`,
      });
    }
    return h(method, params);
  }
}

/**
 * Transport that always fails — makes "no runtime" explicit in errors (§33).
 */
export class UnavailableTransport implements IpcTransport {
  async invoke(domain: string, method: string): Promise<never> {
    throw new KitronError({
      code: "KITRON_RUNTIME_UNAVAILABLE",
      message: `Kitron runtime unavailable for ${domain}.${method}`,
    });
  }
}
