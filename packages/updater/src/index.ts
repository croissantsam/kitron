/**
 * `@kitron/updater` — check for, download and install updates (spec §42).
 *
 * macOS checks a remote version endpoint, downloads the release bundle into
 * the user's cache, verifies its SHA-256 (when provided) and stages an
 * install that swaps the running `.app` on the next launch. Gated by the
 * `updater` permission (spec §23).
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

export interface DownloadResult {
  path: string;
  verified: boolean;
  size: number;
}

export interface InstallResult {
  scheduled: boolean;
  target: string;
}

export const updater = {
  /** Checks whether a newer version is available. */
  async check(url: string): Promise<UpdateInfo> {
    return parseEnvelope(
      await requireNative().updaterCheck(JSON.stringify({ url })),
    ) as UpdateInfo;
  },

  /** Downloads the release bundle. Returns its local path + verification. */
  async download(url: string, sha256?: string): Promise<DownloadResult> {
    return parseEnvelope(
      await requireNative().updaterDownload(JSON.stringify({ url, sha256 })),
    ) as DownloadResult;
  },

  /** Stages an install of a downloaded bundle (swaps on next launch). */
  async install(path: string): Promise<InstallResult> {
    return parseEnvelope(
      await requireNative().updaterInstall(JSON.stringify({ path })),
    ) as InstallResult;
  },
};
