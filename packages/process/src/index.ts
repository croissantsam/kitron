/**
 * `@kitron/process` — environment + child process spawning (spec §42).
 *
 * Pure Rust (no AppKit) on macOS. Gated by the `process` permission (spec §23).
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
      message: "process APIs require the Kitron native runtime",
    });
  }
  return native;
}

export interface SpawnOptions {
  command: string;
  args?: string[];
  cwd?: string;
  env?: Record<string, string>;
}

export interface SpawnResult {
  status: number | null;
  stdout: string;
  stderr: string;
  success: boolean;
}

export const process = {
  /** Reads an environment variable (undefined when unset). */
  async env(name: string): Promise<string | undefined> {
    const result = parseEnvelope(
      await requireNative().processEnv(name),
    ) as { value: string | null };
    return result.value ?? undefined;
  },

  /** Spawns a child process and awaits its completion. */
  async spawn(options: SpawnOptions): Promise<SpawnResult> {
    return parseEnvelope(
      await requireNative().processSpawn(JSON.stringify(options ?? {})),
    ) as SpawnResult;
  },
};
