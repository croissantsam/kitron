/**
 * App configuration shape (spec §21) — TS mirror of
 * `@kitron/protocol::KitronConfig` (camelCase on the wire).
 */

export interface KitronAppConfig {
  name: string;
  identifier: string;
  version: string;
}

export interface KitronWindowConfig {
  width?: number;
  height?: number;
  title?: string;
}

export interface KitronNodeConfig {
  /** Backend entry point, relative to the app root. */
  entry?: string;
  /** Spawn the backend automatically at app boot (default: true). */
  autoStart?: boolean;
}

export interface KitronConfig {
  app: KitronAppConfig;
  window?: KitronWindowConfig;
  node?: KitronNodeConfig;
  permissions?: string[];
  extra?: Record<string, unknown>;
}
