/** CLI shared utilities: paths, process spawning, app config loading. */

import child_process from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

import type { KitronConfig } from "@kitron/core";

/** Absolute path of the CLI's own package dir (…/kitron/cli). */
export const CLI_PACKAGE_ROOT = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

/**
 * The Kitron checkout that ships the CLI. Normalize via KITRON_SDK_ROOT when
 * developing the runtime itself from a different checkout.
 */
export const SDK_ROOT = path.resolve(
  process.env.KITRON_SDK_ROOT ?? CLI_PACKAGE_ROOT,
  "..",
);

export const RUST_WORKSPACE = path.join(SDK_ROOT, "runtime", "rust");
export const POD_TEMPLATE_ROOT = path.join(SDK_ROOT, "runtime", "macos", "pod");
export const NODE_HOST_TEMPLATE = path.join(SDK_ROOT, "runtime", "node", "host.mjs");
export const APP_TEMPLATE_ROOT = path.join(SDK_ROOT, "templates", "app");

export type Logger = {
  info: (message: string) => void;
  warn: (message: string) => void;
  fail: (message: string) => never;
};

export const log: Logger = {
  info: (message) => console.log(`\x1b[36mkitron\x1b[0m ${message}`),
  warn: (message) => console.warn(`\x1b[33mkitron\x1b[0m ${message}`),
  fail: (message) => {
    console.error(`\x1b[31mkitron\x1b[0m ${message}`);
    throw new Error(message);
  },
};

/** Wrapper around child_process.spawnSync mirroring the caller's stdio. */
export function run(
  command: string,
  args: readonly string[],
  options: {
    cwd?: string;
    env?: Record<string, string | undefined>;
    capture?: boolean;
  } = {},
): { status: number; stdout: string; stderr: string } {
  const result = child_process.spawnSync(command, [...args], {
    cwd: options.cwd,
    env: { ...process.env, ...options.env },
    stdio: options.capture ? "pipe" : "inherit",
    encoding: "utf8",
  });
  return {
    status: result.status ?? 1,
    stdout: typeof result.stdout === "string" ? result.stdout : "",
    stderr: typeof result.stderr === "string" ? result.stderr : "",
  };
}

/** Rust target triples for the macOS architectures we ship. */
const RUST_TARGETS: Record<string, string> = {
  arm64: "aarch64-apple-darwin",
  x64: "x86_64-apple-darwin",
};

/**
 * Builds the Rust staticlib as a universal (arm64 + x86_64) archive.
 *
 * The Xcode Release configuration builds a universal binary (ARCHS_STANDARD),
 * so a single-architecture `.a` fails to link for the other slice. When the
 * second target's standard library is unavailable we fall back to the host
 * architecture and say so — a host-only build still runs locally.
 *
 * `targetDir` keeps the Cargo artifacts out of `node_modules` (which
 * `pnpm install` wipes) — the app's own build directory survives installs.
 */
export function buildRustStaticLib(targetDir?: string): string {
  const cargoTargetRoot = targetDir ?? path.join(RUST_WORKSPACE, "target");
  const cargoTarget = targetDir
    ? { CARGO_TARGET_DIR: targetDir }
    : ({} as Record<string, string>);
  const hostTriple = RUST_TARGETS[process.arch] ?? RUST_TARGETS.arm64!;
  const otherTriple =
    hostTriple === RUST_TARGETS.arm64 ? RUST_TARGETS.x64! : RUST_TARGETS.arm64!;

  const built: string[] = [];
  for (const triple of [hostTriple, otherTriple]) {
    const result = run(
      "cargo",
      ["build", "--release", "-p", "kitron-macos", "--target", triple],
      { cwd: RUST_WORKSPACE, env: cargoTarget },
    );
    const archive = path.join(
      cargoTargetRoot,
      triple,
      "release",
      "libkitron_macos.a",
    );
    if (result.status !== 0 || !fs.existsSync(archive)) {
      if (built.length > 0) {
        log.warn(
          `cargo build for ${triple} failed — shipping a ${built.length === 1 ? "single" : "partial"}-architecture runtime`,
        );
        break;
      }
      throw new Error(`cargo build -p kitron-macos --target ${triple} failed`);
    }
    built.push(archive);
  }

  if (built.length === 1) {
    return built[0]!;
  }

  const universalDir = path.join(cargoTargetRoot, "kitron-universal");
  fs.mkdirSync(universalDir, { recursive: true });
  const output = path.join(universalDir, "libkitron_macos.a");
  const lipo = run("lipo", ["-create", "-output", output, ...built], {
    capture: true,
  });
  if (lipo.status !== 0) {
    log.warn("lipo failed — falling back to the host architecture");
    return built[0]!;
  }
  return output;
}

/**
 * Runs `pod install` with retries.
 *
 * CocoaPods' CDN intermittently fails with "pathname contains null byte"
 * (CocoaPods#12866) — a transient network/CDN hiccup, not a project error.
 * Retrying a few times turns a hard failure into a short pause.
 */
export function runPodInstall(cwd: string, attempts = 3): boolean {
  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    const pod = run("pod", ["install"], {
      cwd,
      env: { RCT_NEW_ARCH_ENABLED: "1" },
    });
    if (pod.status === 0) {
      return true;
    }
    if (attempt < attempts) {
      log.warn(`pod install failed (attempt ${attempt}/${attempts}) — retrying…`);
      // Give the CDN a moment before the next attempt.
      child_process.spawnSync("sleep", ["3"]);
    }
  }
  return false;
}

/** Renders an unresolved config for `KITRON_CONFIG_JSON`. */
export function serializeConfig(config: KitronConfig): string {
  return JSON.stringify({
    app: config.app,
    window: config.window ?? {},
    node: config.node ?? {},
    permissions: config.permissions ?? [],
    extra: config.extra ?? {},
  });
}

/** Imports the app's `kitron.config.ts` via Node type-stripping. */
export async function loadAppConfig(appRoot: string): Promise<KitronConfig> {
  const entry = path.join(appRoot, "kitron.config.ts");
  if (!fs.existsSync(entry)) {
    log.fail(`missing kitron.config.ts in ${appRoot}`);
  }
  const module = await import(`file://${entry}`);
  const config = (module.default ?? module.config) as KitronConfig;
  if (!config?.app?.name || !config?.app?.identifier) {
    log.fail("kitron.config.ts must export {app:{name,identifier,…}}");
  }
  return config;
}

/** Where `kitron create`/`dev` got run against (default: cwd). */
export function resolveAppRoot(maybe?: string): string {
  return path.resolve(maybe ?? process.cwd());
}

/**
 * Copies a directory tree, preserving dotfiles (prunes nested node_modules).
 *
 * `ignore` lists directory names to skip anywhere in the tree (e.g. the Rust
 * `target/` build directory).
 */
export function copyDir(
  from: string,
  to: string,
  options: { ignore?: string[] } = {},
): void {
  const ignore = new Set(options.ignore ?? []);
  fs.cpSync(from, to, {
    recursive: true,
    force: true,
    // Only prune node_modules *inside* the copied tree — the source path itself
    // may legitimately live under a node_modules directory.
    filter: (source) => {
      const relative = path.relative(from, source);
      const segments = relative.split(path.sep);
      if (segments.some((segment) => ignore.has(segment))) {
        return false;
      }
      return !segments.includes("node_modules");
    },
  });
}
