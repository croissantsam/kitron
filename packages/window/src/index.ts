/**
 * `window` namespace (spec §10): native window management through the Rust
 * runtime (AppKit on macOS). The windows are owned by the native core — JS
 * collects snapshots; native performs the ops.
 */

import {
  KitronError,
  events,
  getJsiHost,
  getNativeModule,
  parseEnvelope,
  throwIfFailed,
} from "@kitron/core";

export type {
  SetPositionArgs,
  SetSizeArgs,
  WindowCreateOptions,
  WindowEvent,
  WindowEventName,
  WindowFrame,
  WindowId,
  WindowSnapshot,
} from "./types.js";

import type {
  SetPositionArgs,
  SetSizeArgs,
  WindowCreateOptions,
  WindowEvent,
  WindowEventName,
  WindowSnapshot,
} from "./types.js";
import type { WindowId } from "./types.js";

function requireNative() {
  const native = getNativeModule();
  if (!native) {
    throw new KitronError({
      code: "KITRON_UNAVAILABLE",
      message: "window APIs require the Kitron native runtime",
    });
  }
  return native;
}

async function op(id: WindowId, name: string, args: object = {}): Promise<void> {
  throwIfFailed(await requireNative().windowOp(id, name, JSON.stringify(args)));
}

export const window = {
  /**
   * Creates a native window and returns its fresh snapshot. The raw
   * `windowPtr` inside the native envelope is implementation detail used by
   * the native bootstrap — never touch it from app code.
   */
  async create(options: WindowCreateOptions = {}): Promise<WindowSnapshot> {
    const created = parseEnvelope(
      await requireNative().windowCreate(JSON.stringify(options)),
    ) as { id: WindowId };
    return window.get(created.id);
  },

  /** Fetches the live snapshot for one window. */
  async get(id: WindowId): Promise<WindowSnapshot> {
    return parseEnvelope(await requireNative().windowGet(id)) as WindowSnapshot;
  },

  /** All live windows (from the JSI snapshot; sync). */
  list(): WindowSnapshot[] {
    const host = getJsiHost();
    if (!host) {
      throw new KitronError({
        code: "KITRON_UNAVAILABLE",
        message: "window.list requires the Kitron JSI runtime",
      });
    }
    return host.windows.list() as WindowSnapshot[];
  },

  close: (id: WindowId) => op(id, "close"),
  minimize: (id: WindowId) => op(id, "minimize"),
  maximize: (id: WindowId) => op(id, "maximize"),
  restore: (id: WindowId) => op(id, "restore"),
  focus: (id: WindowId) => op(id, "focus"),
  show: (id: WindowId) => op(id, "show"),
  hide: (id: WindowId) => op(id, "hide"),
  setSize: (id: WindowId, size: SetSizeArgs) => op(id, "setSize", size),
  setPosition: (id: WindowId, position: SetPositionArgs) => op(id, "setPosition", position),
  setTitle: (id: WindowId, title: string) => op(id, "setTitle", { title }),
  /** Keeps `windowShouldClose` from closing the window for one close request. */
  setPreventClose: (id: WindowId, prevent: boolean) => op(id, "setPreventClose", { preventClose: prevent }),

  /** Subscribes to window events, optionally filtered by window id. */
  on(name: WindowEventName, listener: (event: WindowEvent) => void) {
    return events.on(name, listener);
  },
};
