/**
 * Kitron native process helpers (phase 2).
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

export const process = {
  /** Reads an environment variable of the app process. */
  env: (_name: string): string | undefined => phase2("process.env"),
  /** Spawns a child process through the native layer. */
  spawn: (_options: unknown): void => phase2("process.spawn"),
};
