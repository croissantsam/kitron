/**
 * The JS↔native bridge layer (spec §8).
 *
 * Two native surfaces share this module:
 *
 * 1. **JSI host object** — `globalThis.__kitron`, installed by the Rust core
 *    through `RCTHostRuntimeDelegate.didInitializeRuntime`. Synchronous, zero
 *    async plumbing; powers the *snapshot getters* (`app.info`, `window.list`).
 *
 * 2. **Native module** — `KitronRuntime`, a classic RCT module that hops to the
 *    main queue and resolves promises. Async ops (window create/close, node
 *    invoke) use it. The JSON payload cost is acceptable for these infrequent
 *    calls (spec §31); hot paths stay on the JSI surface.
 *
 * In a plain Node process (Kitron CLI tests) there is no native bridge; the
 * resolvers return `null` and callers surface a clear `KITRON_UNAVAILABLE`.
 */

/** Shape of the JSI host object installed by the native runtime. */
export interface KitronJsiHost {
  version: string;
  platform: string;
  apiVersion: number;
  app: {
    bits: () => { pid: number; name: string; identifier: string; version: string; platform: string; dev: boolean; rootDir?: string };
  };
  windows: {
    list: () => unknown;
  };
  node: {
    status: () => unknown;
  };
}

/** The promise-based native module surface (declared here for typing). */
export interface KitronNativeModule {
  appQuit(): Promise<string>;
  appCancelTermination(): Promise<string>;
  windowCreate(argsJSON: string): Promise<string>;
  windowGet(id: number): Promise<string>;
  windowOp(id: number, op: string, argsJSON: string): Promise<string>;
  eventsDrain(): Promise<string>;
  nodeStart(): Promise<string>;
  nodeStop(): Promise<string>;
  nodeInvoke(method: string, payloadJSON: string, timeoutMs: number): Promise<string>;
  // Phase-2 native APIs (spec §42) — gated by explicit permissions (§23).
  clipboardRead(): Promise<string>;
  clipboardWrite(text: string): Promise<string>;
  shellOpenUrl(url: string): Promise<string>;
  shellOpenPath(path: string): Promise<string>;
  shellShowItemInFolder(path: string): Promise<string>;
  dialogMessage(argsJSON: string): Promise<string>;
  dialogOpenFile(argsJSON: string): Promise<string>;
  dialogSaveFile(argsJSON: string): Promise<string>;
}

type TurboModuleProxy = {
  get: (name: string) => unknown | undefined;
};

type NativeModulesBag = Record<string, KitronNativeModule | undefined>;

let cachedModule: KitronNativeModule | null | undefined;

/**
 * Resolves the async native bridge. Returns `null` when running without the
 * Kitron native layer (plain Node process); callers throw `KITRON_UNAVAILABLE`.
 */
export function getNativeModule(): KitronNativeModule | null {
  if (cachedModule) {
    return cachedModule;
  }

  const globalObject = globalThis as Record<string, unknown>;

  // 1. TurboModule registry (RN ≤0.76 bridged / new-arch interop).
  const turboProxy = globalObject.__turboModuleProxy as TurboModuleProxy | undefined;
  if (typeof turboProxy?.get === "function") {
    const candidate = turboProxy.get("KitronRuntime") as KitronNativeModule | undefined;
    if (candidate) {
      cachedModule = candidate;
      return cachedModule;
    }
  }

  // 2. RN 0.77+ bridgeless: TurboModuleBinding installs the proxy as the
  //    read-only global `nativeModuleProxy` (TurboModuleBinding.cpp — the
  //    `RN$Bridgeless` branch). `NativeModules` itself is undefined there;
  //    the proxy resolves modules via property access and returns `null` on
  //    a miss.
  const nativeModuleProxy = globalObject.nativeModuleProxy as
    | Record<string, KitronNativeModule | null | undefined>
    | undefined;
  if (nativeModuleProxy) {
    const candidate = nativeModuleProxy.KitronRuntime;
    if (candidate) {
      cachedModule = candidate;
      return cachedModule;
    }
  }

  // 3. Classic NativeModules (legacy macOS bridge flow).
  const nativeModules = globalObject.NativeModules as NativeModulesBag | undefined;
  if (nativeModules && nativeModules.KitronRuntime) {
    cachedModule = nativeModules.KitronRuntime;
    return cachedModule;
  }

  // A miss is not cached: the module may still be installed when the runtime
  // finishes booting, so the next call retries the lookup.
  return null;
}

/** Resolves the JSI host object, or null when unavailable. */
export function getJsiHost(): KitronJsiHost | null {
  const candidate = (globalThis as Record<string, unknown>).__kitron as
    | KitronJsiHost
    | undefined;
  return candidate ?? null;
}

/** Parses a JSON result envelope from native and unwraps `{ ok, result, error }`. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function parseEnvelope(raw: string): any {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let envelope: any;
  try {
    envelope = JSON.parse(raw);
  } catch (error) {
    const kitronError = new Error(`bridge returned non-JSON: ${String(error)}`);
    kitronError.name = "KitronError";
    throw kitronError;
  }
  if (envelope && envelope.ok === false) {
    throw envelope.error;
  }
  return envelope?.result;
}

export function isKitronAppProcess(): boolean {
  return getJsiHost() !== null || getNativeModule() !== null;
}
