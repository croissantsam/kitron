/**
 * Kitron IPC protocol — §11, §12, §13.
 *
 * All communications are asynchronous. Every request carries a unique id
 * so responses can be correlated. Events are fire-and-forget broadcasts.
 */

export interface IPCRequest {
  id: string;
  channel: string;
  payload: unknown;
}

export interface IPCResponse {
  id: string;
  success: boolean;
  payload?: unknown;
  error?: IPCError;
}

export interface IPCEvent {
  channel: string;
  payload: unknown;
}

export interface IPCError {
  code: string;
  message: string;
  details?: unknown;
}

/** Well-known error codes — §44. */
export const KitronErrorCodes = [
  "NODE_START_FAILED",
  "NODE_CRASHED",
  "IPC_FAILED",
  "IPC_TIMEOUT",
  "WEBVIEW_FAILED",
  "WINDOW_FAILED",
  "NATIVE_API_FAILED",
  "PERMISSION_DENIED",
  "PLATFORM_UNSUPPORTED",
  "BUILD_FAILED",
  "INVALID_CHANNEL",
  "HANDLER_NOT_FOUND",
  "INVALID_PAYLOAD",
] as const;

export type KitronErrorCode = (typeof KitronErrorCodes)[number] | (string & {});

export type IPCPayload<T = unknown> = T;

/** Channel naming convention: `<domain>.<action>`. */
export type IPCChannel = string;

/** Handler invoked on the receiving side. */
export type IPCHandler<TPayload = unknown, TResult = unknown> = (
  payload: TPayload,
) => Promise<TResult> | TResult;

/** Options for invoke() calls. */
export interface IPCInvokeOptions {
  /** Timeout in ms. Defaults to 30_000. */
  timeout?: number;
  /** Optional AbortSignal to cancel the call. */
  signal?: AbortSignal;
}

/** Listener for subscribed events. */
export type IPCEventListener<T = unknown> = (payload: T) => void;

/** Unsubscribe function returned by subscribe(). */
export type IPCUnsubscribe = () => void;

/** Peers participating in IPC — §10. */
export type IPCPeerKind = "renderer" | "node" | "rust";

/** Serialized NDJSON frame helpers (one JSON object per line). */
export function encodeIPCFrame(value: unknown): string {
  return `${JSON.stringify(value)}\n`;
}

export function isIPCRequest(value: unknown): value is IPCRequest {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return (
    typeof v["id"] === "string" &&
    typeof v["channel"] === "string" &&
    "payload" in v
  );
}

export function isIPCResponse(value: unknown): value is IPCResponse {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return typeof v["id"] === "string" && typeof v["success"] === "boolean";
}

export function isIPCEvent(value: unknown): value is IPCEvent {
  if (typeof value !== "object" || value === null) return false;
  const v = value as Record<string, unknown>;
  return typeof v["channel"] === "string" && "payload" in v && !("id" in v);
}
