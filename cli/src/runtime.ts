/**
 * Shared dev/build/run plumbing: rebuild the Rust staticlib, sync the Kitron
 * pod, drive xcodebuild, and launch the app with the Kitron env contract
 * (spec §22 KITRON_* env vars, §37 CLI).
 */

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import child_process, { type ChildProcess } from "node:child_process";

import type { KitronConfig } from "@kitron/core";

import { log, serializeConfig, run, RUST_WORKSPACE, copyDir, buildRustStaticLib, runPodInstall } from "./lib.ts";

/** Launch env handed to the app binary (§22). */
export function kitronLaunchEnv(appRoot: string, config: KitronConfig, dev: boolean) {
  return {
    KITRON_ROOT: appRoot,
    KITRON_CONFIG_JSON: serializeConfig(config),
    KITRON_NODE_PATH: process.execPath,
    KITRON_RUNTIME_NODE_DIR: path.join(appRoot, ".kitron"),
    KITRON_DEV: dev ? "1" : "0",
  };
}

interface ProjectInfo {
  workspacePath: string;
  scheme: string;
  productName: string;
}

export function discoverProject(appRoot: string): ProjectInfo {
  const macosDir = path.join(appRoot, "macos");
  const workspace = fs
    .readdirSync(macosDir)
    .find((entry) => entry.endsWith(".xcworkspace") && !entry.startsWith("Pods"));
  const productName = workspace?.replace(/\.xcworkspace$/, "");
  if (!workspace || !productName) {
    throw new Error(`no .xcworkspace under ${macosDir} — run \`kitron create\` first`);
  }
  return {
    workspacePath: path.join(macosDir, workspace),
    scheme: `${productName}-macOS`,
    productName,
  };
}

/** Ensures the Rust staticlib + Kitron pod sources are current under macos/. */
export function syncPod(appRoot: string): void {
  log.info("building Rust runtime (cargo build --release)…");
  // Cargo artifacts live in the app's own directory (survives `pnpm install`,
  // unlike node_modules) and next to the Node host runtime.
  const staticLib = buildRustStaticLib(
    path.join(appRoot, ".kitron", "rust-target"),
  );

  const podDir = path.join(appRoot, "macos", "Kitron");
  fs.mkdirSync(path.join(podDir, "Kitron"), { recursive: true });
  fs.copyFileSync(staticLib, path.join(podDir, "libkitron_macos.a"));

  // Refresh bridge sources if developing the SDK in-repo.
  const podSource = path.join(
    RUST_WORKSPACE,
    "..",
    "macos",
    "pod",
    "Kitron",
  );
  for (const file of ["KitronBridge.h", "KitronBridge.mm", "kitron.h"]) {
    const source = path.join(podSource, file);
    if (fs.existsSync(source)) {
      fs.copyFileSync(source, path.join(podDir, "Kitron", file));
    }
  }
  const podspec = path.join(RUST_WORKSPACE, "..", "macos", "pod", "Kitron.podspec");
  if (fs.existsSync(podspec)) {
    fs.copyFileSync(podspec, path.join(podDir, "Kitron.podspec"));
  }
  // The podspec declares its license file — CocoaPods warns without it.
  const license = path.join(RUST_WORKSPACE, "..", "macos", "pod", "LICENSE");
  if (fs.existsSync(license)) {
    fs.copyFileSync(license, path.join(podDir, "LICENSE"));
  }

  // Ensure the Podfile references the Kitron pod (post-generate patch).
  const podfileIntent = "pod 'Kitron'";
  const podfile = path.join(appRoot, "macos", "Podfile");
  if (fs.existsSync(podfile)) {
    const source = fs.readFileSync(podfile, "utf8");
    if (!source.includes(podfileIntent)) {
      log.warn("Podfile missing the Kitron pod — add `pod 'Kitron', :path => 'Kitron'`");
    }
  }
}

/** Runs `pod install` when the workspace isn't set up yet (or forced). */
export function ensurePods(appRoot: string, force = false): void {
  const podsDir = path.join(appRoot, "macos", "Pods");
  const workspace = fs
    .readdirSync(path.join(appRoot, "macos"))
    .find((entry) => entry.endsWith(".xcworkspace"));
  if (!force && fs.existsSync(podsDir) && workspace) {
    return;
  }
  log.info("pod install (RCT_NEW_ARCH_ENABLED=1)…");
  if (!runPodInstall(path.join(appRoot, "macos"))) {
    throw new Error("pod install failed");
  }
}

export async function buildXcode(
  appRoot: string,
  configuration: "Debug" | "Release" = "Debug",
): Promise<string> {
  const project = discoverProject(appRoot);
  const derivedData = path.join(appRoot, "macos", "build", "derived");
  fs.mkdirSync(derivedData, { recursive: true });
  log.info(
    `xcodebuild (${configuration}) — first build pulls the Rust React pods and takes minutes…`,
  );
  const build = run(
    "xcodebuild",
    [
      "-workspace",
      project.workspacePath,
      "-scheme",
      project.scheme,
      "-configuration",
      configuration,
      "-derivedDataPath",
      derivedData,
      // The app entry is TypeScript (`index.ts`).
      "ENTRY_FILE=index.ts",
      // In Release the CLI bundles the JS itself (and embeds the assets), so
      // the Xcode bundle phase would only duplicate that work.
      ...(configuration === "Release" ? ["SKIP_BUNDLING=1"] : []),
      "build",
    ],
    { env: { RCT_NEW_ARCH_ENABLED: "1" } },
  );
  if (build.status !== 0) {
    throw new Error(
      `xcodebuild failed (exit ${build.status}) — rerun with --verbose for details`,
    );
  }
  const productsDir = path.join(
    derivedData,
    "Build",
    "Products",
    configuration,
  );
  const app = fs
    .readdirSync(productsDir)
    .filter((entry) => entry.endsWith(".app"))
    .sort()[0];
  if (!app) {
    throw new Error(`no .app produced under ${productsDir}`);
  }
  return path.join(productsDir, app);
}

/** Locates the most recently built .app for the given configuration. */
export function findBuiltApp(
  appRoot: string,
  configuration: "Debug" | "Release" = "Debug",
): string {
  const productsDir = path.join(
    appRoot,
    "macos",
    "build",
    "derived",
    "Build",
    "Products",
    configuration,
  );
  if (!fs.existsSync(productsDir)) {
    throw new Error(
      `no built app for ${configuration} — run \`kitron build\` first`,
    );
  }
  const app = fs
    .readdirSync(productsDir)
    .filter((entry) => entry.endsWith(".app"))
    .sort()[0];
  if (!app) {
    throw new Error(`no .app under ${productsDir}`);
  }
  return path.join(productsDir, app);
}

/** Reads CFBundleExecutable from an app bundle's Info.plist. */
function bundleExecutable(appPath: string): string {
  const plist = path.join(appPath, "Contents", "Info.plist");
  const result = run("/usr/libexec/PlistBuddy", ["-c", "Print :CFBundleExecutable", plist], {
    capture: true,
  });
  const name = result.stdout.trim();
  if (result.status !== 0 || !name) {
    // Fall back to the bundle name (the usual convention).
    return path.basename(appPath, ".app");
  }
  return name;
}

/** Spawns the app binary directly (env flows through; logs stream here). */
export function launchApp(appPath: string, env: Record<string, string>): ChildProcess {
  const executable = path.join(appPath, "Contents", "MacOS", bundleExecutable(appPath));
  log.info(`launching ${appPath}`);
  const child = child_process.spawn(executable, [], {
    env: { ...process.env, ...env },
    stdio: "inherit",
  });
  child.on("exit", (code) => {
    log.info(`app exited (${code})`);
  });
  process.on("SIGINT", () => {
    child.kill("SIGTERM");
    process.exit(0);
  });
  return child;
}

/** Starts Metro (react-native start) in the app root; resolves when ready.
 *  Returns null when a Metro instance is already serving the port. */
export async function startMetro(
  appRoot: string,
  port = 8081,
): Promise<ChildProcess | null> {
  // Already running?
  const probe = await fetch(`http://localhost:${port}/status`, { method: "GET" }).catch(
    () => null,
  );
  if (probe && probe.ok) {
    log.warn(
      `Metro already running on :${port} — if it serves another project, restart it or use --port <other>`,
    );
    return null;
  }
  log.info(`starting Metro on :${port}…`);
  const child = child_process.spawn(
    "npx",
    ["react-native", "start", "--port", String(port)],
    { cwd: appRoot, stdio: "inherit" },
  );
  // Await readiness (Metro /status endpoint).
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const status = await fetch(`http://localhost:${port}/status`).catch(() => null);
    if (status && status.ok) {
      log.info("Metro ready");
      return child;
    }
    await new Promise((resolve) => setTimeout(resolve, 300));
  }
  throw new Error(`Metro did not become ready within 30s (port ${port})`);
}
