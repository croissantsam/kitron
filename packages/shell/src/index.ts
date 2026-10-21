/**
 * `@kitron/shell` — OS shell integration (spec §15).
 *
 * macOS uses `NSWorkspace` through the native runtime; the calls are gated by
 * the `shell` permission declared in `kitron.config.ts` (spec §23).
 */

import {
  KitronError,
  getNativeModule,
  throwIfFailed,
} from "@kitron/core";

function requireNative() {
  const native = getNativeModule();
  if (!native) {
    throw new KitronError({
      code: "KITRON_UNAVAILABLE",
      message: "shell APIs require the Kitron native runtime",
    });
  }
  return native;
}

export const shell = {
  /** Opens a URL with the default handler (browser, mail client, …). */
  async openUrl(url: string): Promise<void> {
    if (typeof url !== "string" || url.length === 0) {
      throw new KitronError({
        code: "KITRON_ARGS_INVALID",
        message: "shell.openUrl requires a URL",
      });
    }
    throwIfFailed(await requireNative().shellOpenUrl(url));
  },

  /** Opens a file or folder with the default handler. */
  async openPath(path: string): Promise<void> {
    if (typeof path !== "string" || path.length === 0) {
      throw new KitronError({
        code: "KITRON_ARGS_INVALID",
        message: "shell.openPath requires a path",
      });
    }
    throwIfFailed(await requireNative().shellOpenPath(path));
  },

  /** Reveals a file in Finder. */
  async showItemInFolder(path: string): Promise<void> {
    if (typeof path !== "string" || path.length === 0) {
      throw new KitronError({
        code: "KITRON_ARGS_INVALID",
        message: "shell.showItemInFolder requires a path",
      });
    }
    throwIfFailed(await requireNative().shellShowItemInFolder(path));
  },
};
