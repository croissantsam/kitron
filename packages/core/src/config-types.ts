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

/**
 * The app's backend (spec §29).
 *
 * `type` selects the runtime:
 * - `"node"` (default) — a TypeScript/JavaScript backend run by the embedded
 *   Node host.
 * - `"binary"` — a compiled backend executable (e.g. a Rust crate). It speaks
 *   the same protocol, so `backend.invoke` behaves identically.
 *
 * When the app ships a `backend/Cargo.toml`, the CLI builds it and points the
 * runtime at the resulting binary — the developer never configures this.
 */
export interface KitronBackendConfig {
  type?: "node" | "binary";
  /** Backend entry: the TypeScript file (`node`) or the executable (`binary`). */
  entry?: string;
  /** Spawn the backend automatically at app boot (default: true). */
  autoStart?: boolean;
}

export interface KitronConfig {
  app: KitronAppConfig;
  window?: KitronWindowConfig;
  /** The app's backend — takes precedence over the legacy `node` section. */
  backend?: KitronBackendConfig;
  /** Legacy spelling of `backend` (TypeScript backends only). */
  node?: KitronNodeConfig;
  permissions?: string[];
  extra?: Record<string, unknown>;
}
