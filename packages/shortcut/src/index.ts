/**
 * `@kitron/shortcut` — global keyboard shortcuts (spec §42).
 *
 * macOS uses Carbon `RegisterEventHotKey`. Gated by the `shortcut` permission
 * (spec §23). Registered shortcuts fire even when the app is in the
 * background; the handler is invoked with the accelerator string.
 */

import {
  KitronError,
  getNativeModule,
  events,
} from "@kitron/core";

function requireNative() {
  const native = getNativeModule();
  if (!native) {
    throw new KitronError({
      code: "KITRON_UNAVAILABLE",
      message: "shortcut APIs require the Kitron native runtime",
    });
  }
  return native;
}

const handlers = new Map<string, Set<() => void>>();

let wired = false;
function wire() {
  if (wired) return;
  wired = true;
  events.on("shortcut.triggered", (payload: { accelerator: string }) => {
    const set = handlers.get(payload?.accelerator);
    if (set) for (const h of set) h();
  });
}

export const shortcut = {
  /** Registers a global shortcut. */
  async register(accelerator: string, handler: () => void): Promise<void> {
    requireNative();
    wire();
    if (!handlers.has(accelerator)) handlers.set(accelerator, new Set());
    handlers.get(accelerator)!.add(handler);
    await requireNative().shortcutRegister(accelerator);
  },

  /** Unregisters a global shortcut. */
  async unregister(accelerator: string): Promise<void> {
    handlers.delete(accelerator);
    await requireNative().shortcutUnregister(accelerator);
  },
};
