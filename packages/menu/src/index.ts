/**
 * Kitron native application menus (phase 2).
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

export const menu = {
  /** Builds and installs the native application menu. */
  create: (_definition: unknown): void => phase2("menu.create"),
  /** Replaces the native application menu. */
  setApplicationMenu: (_definition: unknown): void => phase2("menu.setApplicationMenu"),
};
