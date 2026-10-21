/**
 * Kitron OS shell integration (phase 2).
 *
 * Part of the Kitron SDK package layout (spec §24). The namespace is reserved
 * by the runtime but its native implementation lands after the MVP gate
 * (spec §41) — calling it throws a typed `KITRON_PHASE2` error so apps can
 * feature-detect instead of branching on versions.
 */

import { KitronError } from "@kitron/core";

function phase2(feature: string): never {
  throw new KitronError({
    code: "KITRON_PHASE2",
    message:
      `${feature} ships after the MVP gate (spec §41). ` +
      "Track progress on the Kitron roadmap; feature-detect with try/catch.",
  });
}

export const shell = {
  /** Opens a URL with the default handler (NSWorkspace). */
  openUrl: (_url: string): void => phase2("shell.openUrl"),
  /** Opens a file or folder with the default handler. */
  openPath: (_path: string): void => phase2("shell.openPath"),
  /** Reveals a file in Finder. */
  showItemInFolder: (_path: string): void => phase2("shell.showItemInFolder"),
};
