/**
 * @kitron/core — typed app config (§37) + capability declaration (§23).
 *
 * ```ts
 * // kitron.config.ts
 * import { defineConfig } from "@kitron/core/config";
 *
 * export default defineConfig({
 *   app: { name: "My App", identifier: "com.example.myapp", version: "1.0.0" },
 *   window: { width: 1200, height: 800 },
 *   node: { entry: "./backend/index.ts" },
 *   // Sensitive domains are denied unless listed here.
 *   permissions: ["shell"],
 * });
 * ```
 */

export type SensitivePermission = "shell" | "process";

export interface KitronAppConfig {
  app: {
    name: string;
    identifier: string;
    version: string;
  };
  window?: {
    width?: number;
    height?: number;
  };
  node?: {
    entry?: string;
  };
  /**
   * Granted sensitive domains. Anything not listed here (and not benign)
   * is rejected with `PERMISSION_DENIED` — there is no wildcard.
   */
  permissions?: SensitivePermission[];
}

export function defineConfig(config: KitronAppConfig): KitronAppConfig {
  return config;
}
