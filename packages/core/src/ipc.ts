/**
 * @kitron/core — IPC wire protocol (§8, §30).
 *
 * The wire format is shared with `runtime/rust/kitron-core`.
 * Do NOT change field names without updating the Rust side.
 *
 * Request  (JS → Rust):  { id, domain, method, params }
 * Response (Rust → JS):  { id, ok: true, result } | { id, ok: false, error }
 * Event    (Rust → JS):  { type: "event", name, payload }
 * Log      (Rust → JS):  { type: "log", entry }
 */

import type { KitronErrorDetails } from "./errors.js";

export interface IpcRequest {
  id: number;
  domain: string;
  method: string;
  params?: unknown;
}

export type IpcResponse =
  | { id: number; ok: true; result: unknown }
  | { id: number; ok: false; error: KitronErrorDetails };

export type IpcInbound =
  | IpcResponse
  | { type: "event"; name: string; payload: unknown }
  | { type: "log"; entry: { level: string; scope: string; message: string; time: string; data?: unknown } };

export interface IpcTransport {
  /** Send a request, resolve with the native result or reject with KitronError. */
  invoke(domain: string, method: string, params?: unknown): Promise<unknown>;
  /** Subscribe to a native event (e.g. `window.closed`, `app.deepLink`). */
  onEvent?(listener: (name: string, payload: unknown) => void): () => void;
  close?(): void;
}

let nextId = 1;

export function createRequestId(): number {
  const id = nextId;
  nextId = nextId >= Number.MAX_SAFE_INTEGER ? 1 : nextId + 1;
  return id;
}

export function encodeRequest(domain: string, method: string, params?: unknown): IpcRequest {
  return { id: createRequestId(), domain, method, params };
}
