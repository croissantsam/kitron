/**
 * Kitron native dialogs (phase 2).
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

export const dialog = {
  /** Shows a native message box. */
  message: (_options: unknown): void => phase2("dialog.message"),
  file: {
    /** Opens the native file picker. */
    open: (_options?: unknown): void => phase2("dialog.file.open"),
    /** Opens the native save panel. */
    save: (_options?: unknown): void => phase2("dialog.file.save"),
  },
};
