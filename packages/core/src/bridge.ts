/**
 * @kitron/core — bridge singleton (§8).
 *
 * JS/TS → (JSI / Native Module in production, IPC socket in MVP) → Rust → Native OS
 *
 * Rules:
 * - high-frequency calls MUST stay typed + async (§8, §31)
 * - never block the UI thread (§32): every method returns a Promise
 * - the bridge is swappable so tests and `kitron dev` (without a native
 *   binary) keep working via an in-memory transport.
 */

import { KitronError } from "./errors.js";
import type { IpcTransport } from "./ipc.js";

type EventListener = (payload: unknown) => void;

const eventListeners = new Map<string, Set<EventListener>>();
const transportEventOffs = new WeakSet<object>();

let transport: IpcTransport | null = null;

export function setBridgeTransport(t: IpcTransport | null): void {
  transport = t;
}

export function getBridgeTransport(): IpcTransport | null {
  return transport;
}

function ensureTransportSubscription(t: IpcTransport): void {
  if (transportEventOffs.has(t)) return;
  if (typeof t.onEvent === "function") {
    transportEventOffs.add(t);
    t.onEvent((name, payload) => dispatchEvent(name, payload));
  }
}

/**
 * Typed invoke. Rejects with KitronError — never a bare Error (§33).
 */
export async function invoke<T = unknown>(
  domain: string,
  method: string,
  params?: unknown,
  opts?: { timeoutMs?: number },
): Promise<T> {
  const t = transport;
  if (!t) {
    throw new KitronError({
      code: "KITRON_RUNTIME_UNAVAILABLE",
      message:
        `No Kitron native runtime connected (invoke ${domain}.${method}). ` +
        `Run the app via \`kitron dev\` / \`kitron run\`, or install a test transport with setBridgeTransport().`,
    });
  }
  ensureTransportSubscription(t);
  const timeoutMs = opts?.timeoutMs ?? 30_000;
  const task = t.invoke(domain, method, params) as Promise<T>;
  if (timeoutMs <= 0) return task;
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      task,
      new Promise<never>((_, reject) => {
        timer = setTimeout(
          () =>
            reject(
              new KitronError({
                code: "KITRON_IPC_TIMEOUT",
                message: `Timed out after ${timeoutMs}ms: ${domain}.${method}`,
              }),
            ),
          timeoutMs,
        );
      }),
    ]);
  } finally {
    if (timer) clearTimeout(timer);
  }
}

export function onNativeEvent(name: string, listener: EventListener): () => void {
  let set = eventListeners.get(name);
  if (!set) {
    set = new Set();
    eventListeners.set(name, set);
  }
  set.add(listener);
  return () => {
    set!.delete(listener);
    if (set!.size === 0) eventListeners.delete(name);
  };
}

/** @internal — called by transports and tests. */
export function dispatchEvent(name: string, payload: unknown): void {
  const set = eventListeners.get(name);
  if (!set) return;
  for (const l of [...set]) {
    try {
      l(payload);
    } catch {
      // listener errors must not break dispatch
    }
  }
}

/** @internal — test helper. */
export function __resetBridgeForTests(): void {
  transport = null;
  eventListeners.clear();
}
