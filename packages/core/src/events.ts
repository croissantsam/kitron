/**
 * Runtime event plumbing (spec §32 bridge, §11/§20 subscriptions).
 *
 * Native pushes events into the core event bus; the bridge exposes
 * `eventsDrain()`. The JS layer polls the drain and fans out to subscribers
 * — 10 Hz is ample for window & backend lifecycle in the MVP (spec §41;
 * push-based delivery is the phase-2 upgrade once the native EventEmitter
 * pathway is wired).
 */

import { getNativeModule } from "./bridge.js";
import type { KitronErrorShape } from "./error.js";

export type KitronEventName =
  | "app.ready"
  | "app.activate"
  | "app.beforeQuit"
  | "app.quit"
  | "window.created"
  | "window.closed"
  | "window.focus"
  | "window.blur"
  | "window.resize"
  | "window.move"
  | "window.minimize"
  | "window.restore"
  | "node.started"
  | "node.ready"
  | "node.log"
  | "node.stopping"
  | "node.exited"
  | "backend.event";

/** `window.*` events carry `windowId`; `node.log` carries level/message etc. */
export type KitronEvent = {
  name: KitronEventName;
  windowId?: number | null;
  payload: unknown;
};

type Listener<T = unknown> = (event: T) => void;

const DRAIN_INTERVAL_MS = 100;

export class KitronEventEmitter {
  private listeners = new Map<string, Set<Listener<never>>>();
  private pump: ReturnType<typeof setInterval> | null = null;
  private drainInFlight = false;

  on<T>(name: string, listener: Listener<T>): () => void {
    let set = this.listeners.get(name);
    if (!set) {
      set = new Set();
      this.listeners.set(name, set);
    }
    set.add(listener as Listener<never>);
    this.ensurePump();
    return () => this.off(name, listener);
  }

  once<T>(name: string, listener: Listener<T>): () => void {
    const wrapped: Listener<T> = (event) => {
      off();
      listener(event);
    };
    const off = this.on(name, wrapped);
    return off;
  }

  off<T>(name: string, listener: Listener<T>): void {
    this.listeners.get(name)?.delete(listener as Listener<never>);
  }

  /** Local dispatch (used for pump-decoded events). */
  emitLocal(name: string, event: KitronEvent): void {
    const relevant = this.listeners.get(name);
    if (!relevant) {
      return;
    }
    for (const listener of relevant) {
      try {
        (listener as Listener<KitronEvent>)(event);
      } catch (error) {
        // Listener errors must never break the pump loop.
        console.error(`[kitron] listener for "${name}" threw`, error);
      }
    }
  }

  /** Starts the drain interval when the first listener arrives. */
  private ensurePump(): void {
    if (this.pump !== null) {
      return;
    }
    const startPump = () => {
      this.pump = setInterval(() => void this.drain(), DRAIN_INTERVAL_MS);
      void this.drain();
    };
    // Pump starts whenever a native module materializes; if there is no native
    // runtime (plain Node/test host), listeners for local events still work.
    startPump();
  }

  private async drain(): Promise<void> {
    if (this.drainInFlight) {
      return;
    }
    const native = getNativeModule();
    if (!native) {
      return;
    }
    this.drainInFlight = true;
    try {
      const raw = await native.eventsDrain();
      const envelope = JSON.parse(raw) as {
        ok: boolean;
        result?: KitronEvent[];
        error?: KitronErrorShape;
      };
      if (!envelope.ok || !envelope.result) {
        return;
      }
      for (const event of envelope.result) {
        this.emitLocal(event.name, event);
      }
    } catch {
      // Drains are best effort: transient bridge failures just retry next tick.
    } finally {
      this.drainInFlight = false;
    }
  }

  /** Test/teardown hook. */
  stop(): void {
    if (this.pump !== null) {
      clearInterval(this.pump);
      this.pump = null;
    }
  }
}

/** Process-wide event hub shared by app/window/node namespaces. */
export const events = new KitronEventEmitter();
