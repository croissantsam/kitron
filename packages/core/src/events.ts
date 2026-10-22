/**
 * Runtime event plumbing (spec §32 bridge, §11/§20 subscriptions).
 *
 * The native runtime **pushes** events: the UI parks a promise on
 * `nextEvent()` and the runtime resolves it the moment an event happens (a
 * window moved, a menu item was clicked, the backend pushed an event). There
 * is no polling — an event reaches the app in the same tick it happened.
 *
 * Outside a Kitron runtime (plain Node, tests) there is no native module, so
 * the pump simply stays idle and local dispatch still works.
 */

import { getNativeModule } from "./bridge.js";

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
  | "backend.event"
  | "menu.action";

/** `window.*` events carry `windowId`; `node.log` carries level/message etc. */
export type KitronEvent = {
  name: KitronEventName;
  windowId?: number | null;
  payload: unknown;
};

type Listener<T = unknown> = (event: T) => void;

export class KitronEventEmitter {
  private listeners = new Map<string, Set<Listener<never>>>();
  private pumpRunning = false;
  private stopped = false;

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

  /** Starts the push loop when the first listener arrives. */
  private ensurePump(): void {
    if (this.pumpRunning || this.stopped) {
      return;
    }
    this.pumpRunning = true;
    void this.pump();
  }

  /**
   * The push loop: park a promise on the native side and dispatch whatever
   * comes back, then park the next one.
   */
  private async pump(): Promise<void> {
    const native = getNativeModule();
    if (!native) {
      // No native runtime (plain Node) — local dispatch still works.
      this.pumpRunning = false;
      return;
    }
    while (!this.stopped) {
      try {
        const raw = await native.nextEvent();
        if (!raw) {
          break;
        }
        const event = JSON.parse(raw) as { name?: string; payload?: unknown };
        if (typeof event.name === "string") {
          this.emitLocal(event.name, {
            name: event.name as KitronEventName,
            payload: event.payload,
          });
        }
      } catch {
        // A failed delivery must never kill the loop — the native side may be
        // tearing down (app quit).
        break;
      }
    }
    this.pumpRunning = false;
  }

  /** Test/teardown hook. */
  stop(): void {
    this.stopped = true;
  }
}

/** Process-wide event hub shared by app/window/node namespaces. */
export const events = new KitronEventEmitter();
