/**
 * Kitron application configuration (`kitron.config.ts`) — §21.
 */

export interface KitronPermissions {
  filesystem?: boolean;
  childProcess?: boolean;
  notifications?: boolean;
  globalShortcuts?: boolean;
  clipboard?: boolean;
  dialogs?: boolean;
  tray?: boolean;
  power?: boolean;
}

export interface KitronDevConfig {
  /** Dev server port. When null, a free port is picked automatically. */
  port?: number | null;
  /** Host to bind. Defaults to 127.0.0.1. */
  host?: string;
  /** Open DevTools automatically in dev. */
  devtools?: boolean;
}

export interface KitronBuildConfig {
  /** Frontend dist dir relative to project root. */
  frontendDist?: string;
  /** Backend entry relative to project root. */
  backendEntry?: string;
  /** Output dir for the bundled app. */
  outDir?: string;
  /** App id (reverse domain). */
  appId?: string;
  /** Product name shown in OS. */
  productName?: string;
  /** App version. */
  version?: string;
}

export interface KitronConfig {
  permissions?: KitronPermissions;
  dev?: KitronDevConfig;
  build?: KitronBuildConfig;
}

export function defineConfig(config: KitronConfig): KitronConfig {
  return config;
}

export const DEFAULT_PERMISSIONS: Required<KitronPermissions> = {
  filesystem: true,
  childProcess: true,
  notifications: true,
  globalShortcuts: false,
  clipboard: true,
  dialogs: true,
  tray: true,
  power: true,
};

export interface ResolvedKitronConfig {
  permissions: Required<KitronPermissions>;
  dev: {
    port: number | null;
    host: string;
    devtools: boolean;
  };
  build: {
    frontendDist: string;
    backendEntry: string;
    outDir: string;
    appId: string;
    productName: string;
    version: string;
  };
}

export function resolveConfig(input: KitronConfig = {}): ResolvedKitronConfig {
  return {
    permissions: { ...DEFAULT_PERMISSIONS, ...(input.permissions ?? {}) },
    dev: {
      port: input.dev?.port ?? null,
      host: input.dev?.host ?? "127.0.0.1",
      devtools: input.dev?.devtools ?? false,
    },
    build: {
      frontendDist: input.build?.frontendDist ?? "dist",
      backendEntry: input.build?.backendEntry ?? "src/main.kitron.ts",
      outDir: input.build?.outDir ?? "release",
      appId: input.build?.appId ?? "com.kitron.app",
      productName: input.build?.productName ?? "Kitron App",
      version: input.build?.version ?? "0.1.0",
    },
  };
}
