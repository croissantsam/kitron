import { existsSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { join } from "node:path";
import type { KitronConfig } from "@kitron/types";
import { resolveConfig } from "@kitron/types";

const CANDIDATES = ["kitron.config.ts", "kitron.config.mts", "kitron.config.js", "kitron.config.mjs"];

/** Load `kitron.config.ts` from the project root (§21). Falls back to defaults. */
export async function loadConfig(cwd: string): Promise<ReturnType<typeof resolveConfig>> {
  for (const file of CANDIDATES) {
    const full = join(cwd, file);
    if (!existsSync(full)) continue;
    try {
      const mod = await import(pathToFileURL(full).href);
      const raw: KitronConfig = (mod.default ?? mod.config ?? {}) as KitronConfig;
      return resolveConfig(raw);
    } catch (err) {
      console.warn(`[Kitron][Node] Failed to load ${file}: ${(err as Error).message}`);
    }
  }
  return resolveConfig({});
}
