/**
 * `@kitron/updater` — check for and download updates (spec §42).
 *
 * macOS checks a remote version endpoint and opens the release page in the
 * browser. Gated by the `updater` permission (spec §23).
 */

import {
  KitronError,
  getNativeModule,
  parseEnvelope,
} from "@kitron/core";

function requireNative() {
  const native = getNativeModule();
  if (!native) {
    throw new KitronError({
      code: "KITRON_UNAVAILABLE",
      message: "updater APIs require the Kitron native runtime",
    });
  }
  return native;
}

export interface UpdateInfo {
  current: string;
  latest: string;
  updateAvailable: boolean;
}

export const updater = {
  /** Checks whether a newer version is available. */
  async check(url: string): Promise<UpdateInfo> {
    return parseEnvelope(
      await requireNative().updaterCheck(JSON.stringify({ url })),
    ) as UpdateInfo;
  },

  /** Opens the release page for download. */
  async download(url: string): Promise<void> {
    await requireNative().updaterDownload(JSON.stringify({ url }));
  },
};
