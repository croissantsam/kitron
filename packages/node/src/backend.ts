/**
 * `@kitron/node` (spec §28-§30).
 *
 * Two hosts share this package:
 *
 * - **UI side** (inside the React Native app): `backend.invoke(method, payload)`
 *   proxies through the native bridge to the embedded Node backend (`invoke`
 *   frame → handler result, correlated by id).
 * - **Node side** (inside the backend process): `onInvoke(method, handler)` /
 *   `emit(name, payload)` delegate to the `KitronNode` global installed by the
 *   Rust-managed host bootstrap (`runtime/node/host.mjs`).
 *
 * The import path stays identical, so the same string works on either side.
 */

import { KitronError, getNativeModule, parseEnvelope, throwIfFailed } from "@kitron/core";

/** Subset of the host bootstrap's global (runtime/node/host.mjs). */
export type HostGlobalKitron = {
  protocolVersion: number;
  hostVersion: string;
  onInvoke: (method: string, handler: (payload: unknown) => unknown) => void;
  emit: (name: string, payload?: unknown) => void;
};

declare global {
  // Installed by the Kitron node host before the app entry is imported.
  // eslint-disable-next-line no-var
  var KitronNode: HostGlobalKitron | undefined;
}

export interface NodeStatusInfo {
  phase: "stopped" | "starting" | "ready" | "crashed" | "stopping";
  pid?: number | null;
  entry?: string | null;
  handlers?: number | null;
  lastError?: { code: string; message: string; platform: string } | null;
}

export type InvokeTimeoutMS = number;
export const DEFAULT_INVOKE_TIMEOUT_MS: InvokeTimeoutMS = 15_000;

function expectHostGlobal(): HostGlobalKitron {
  const hostGlobal = globalThis.KitronNode;
  if (!hostGlobal) {
    throw new KitronError({
      code: "KITRON_NODE_HOST_UNAVAILABLE",
      message: "the KitronNode global is only available inside a Kitron-managed Node backend",
    });
  }
  return hostGlobal;
}

export const backend = {
  /**
   * UI side: invokes a backend handler through the native bridge; the generic
   * lets the app type the result from its own declarations.
   */
  async invoke<TResult = unknown>(
    method: string,
    payload?: unknown,
    timeoutMs: InvokeTimeoutMS = DEFAULT_INVOKE_TIMEOUT_MS,
  ): Promise<TResult> {
    if (typeof method !== "string" || method.length === 0) {
      throw new KitronError({
        code: "KITRON_ARGS_INVALID",
        message: "backend.invoke requires a method name",
      });
    }
    const native = getNativeModule();
    if (!native) {
      throw new KitronError({
        code: "KITRON_UNAVAILABLE",
        message: "backend.invoke requires the Kitron native runtime",
      });
    }
    return parseEnvelope(
      await native.nodeInvoke(method, JSON.stringify(payload ?? null), timeoutMs),
    ) as TResult;
  },

  /** Starts the embedded backend per the app config (`node.entry`). */
  async start(): Promise<void> {
    const native = getNativeModule();
    if (!native) {
      throw new KitronError({
        code: "KITRON_UNAVAILABLE",
        message: "backend.start requires the Kitron native runtime",
      });
    }
    throwIfFailed(await native.nodeStart());
  },

  /** Stops the embedded backend gracefully. */
  async stop(): Promise<void> {
    const native = getNativeModule();
    if (!native) {
      throw new KitronError({
        code: "KITRON_UNAVAILABLE",
        message: "backend.stop requires the Kitron native runtime",
      });
    }
    throwIfFailed(await native.nodeStop());
  },

  /** Sync snapshot of backend status (JSI surface). */
  status(): NodeStatusInfo | null {
    const jsi = (globalThis as { __kitron?: { node?: { status?: () => unknown } } }).__kitron;
    const status = jsi?.node?.status?.();
    if (status === undefined || status === null) {
      return null;
    }
    return status as NodeStatusInfo;
  },

  /** Node side: registers a handler (`KitronNode.onInvoke` proxy). */
  onInvoke(method: string, handler: (payload: unknown) => unknown): void {
    expectHostGlobal().onInvoke(method, handler);
  },

  /** Node side: emit an event to the app (`KitronNode.emit` proxy). */
  emit(name: string, payload?: unknown): void {
    expectHostGlobal().emit(name, payload);
  },

  /** Node side: whether the process runs under the Kitron host. */
  isHostedBackend(): boolean {
    return (globalThis as { KitronNode?: unknown }).KitronNode !== undefined;
  },
};
