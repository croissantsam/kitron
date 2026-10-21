/**
 * `app` namespace (spec §11): process-level info, lifecycle events, quit.
 *
 * Sync bits come from the JSI host object; mutations (quit) go through the
 * async native module. In plain Node (e.g. CLI), `app.info()` works because
 * the host object data is also reproducible from env — but lifecycle calls are
 * native-only and surface typed errors.
 */

import { events } from "./events.js";
import { getJsiHost, getNativeModule, parseEnvelope } from "./bridge.js";
import { KitronError, throwIfFailed } from "./error.js";
import type { KitronEvent, KitronEventName } from "./events.js";

export interface AppInfo {
  pid: number;
  name: string;
  identifier: string;
  version: string;
  platform: string;
  dev: boolean;
  rootDir?: string;
}

let appReadyFired = false;

/** Emits `app.ready` to subscribers exactly once, whether native now or later. */
export function watchAppReady(): void {
  events.once("app.ready", () => {
    appReadyFired = true;
  });
}

export const app = {
  /** One-shot app information (synchronous JSI read). */
  info(): AppInfo {
    const host = getJsiHost();
    if (host) {
      return host.app.bits();
    }
    // Plain Node fallback (kitron doctor / tests); no PID cross-check needed.
    return {
      pid: process.pid,
      name: process.env.KITRON_APP_NAME ?? "kitron",
      identifier: process.env.KITRON_APP_IDENTIFIER ?? "dev.kitron.cli",
      version: process.env.KITRON_APP_VERSION ?? "0.0.0",
      platform: process.platform === "darwin" ? "macos" : process.platform,
      dev: process.env.KITRON_DEV === "1",
    };
  },

  /**
   * Resolves when `app.ready` has fired. In the MVP the event is queued by the
   * runtime at window-creation time, so this resolves as soon as the pump
   * delivers it (usually immediately after JS boot).
   */
  onReady(): Promise<void> {
    if (appReadyFired) {
      return Promise.resolve();
    }
    return new Promise((resolve) => {
      events.once("app.ready", () => resolve());
    });
  },

  /** Subscribes to any app event (see spec §11). */
  on(name: KitronEventName, listener: (event: KitronEvent) => void) {
    return events.on(name, listener);
  },

  /** Requests app termination. Fires `app.beforeQuit` to JS first. */
  async quit(): Promise<void> {
    const native = getNativeModule();
    if (!native) {
      throw new KitronError({
        code: "KITRON_UNAVAILABLE",
        message: "app.quit requires the Kitron native runtime",
      });
    }
    throwIfFailed(parseEnvelope(await native.appQuit()));
  },

  /**
   * Vetoes a pending termination (`applicationShouldTerminate` will cancel).
   * Advisory in the MVP promise-free path; honored for the runtime-managed
   * quit requests invoked between beforeQuit and teardown.
   */
  async preventDefaultExit(): Promise<void> {
    const native = getNativeModule();
    if (!native) {
      throw new KitronError({
        code: "KITRON_UNAVAILABLE",
        message: "app.preventDefaultExit requires the Kitron native runtime",
      });
    }
    throwIfFailed(parseEnvelope(await native.appCancelTermination()));
  },
};
