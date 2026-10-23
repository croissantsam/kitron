/** Kitron CLI dispatch + the dev/build/run commands (spec §37/§41). */

import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

import type { KitronConfig } from "@kitron/core";

import { loadAppConfig, log, resolveAppRoot, copyDir, run, resolveBackend, resolveUpdateSourceRoot } from "./lib.ts";
import { doctor } from "./doctor.ts";
import {
  create,
  syncNodeHost,
  vendorRuntime,
  vendorSdkPackages,
  vendorWindowsTemplate,
  loadTypescript,
} from "./create.ts";
import {
  buildXcode,
  devWindows,
  discoverProject,
  ensurePods,
  findBuiltApp,
  kitronLaunchEnv,
  launchApp,
  startMetro,
  syncPod,
} from "./runtime.ts";
import fs from "node:fs";

const USAGE = `kitron — desktop runtime for React Native + TypeScript + Node.js

Usage:
  kitron create <dir> [--name N] [--identifier ID] [--backend rust]
  kitron update [dir] [--sdk <checkout>]
  kitron doctor
  kitron dev     [dir] [--port 8081] [--platform macos|windows]
  kitron build   [dir]
  kitron package [dir]
  kitron run     [dir]

The app root contains kitron.config.ts (default: cwd).`;

interface DevArgs {
  dir?: string;
  port: number;
  scheme?: string;
}

function parseFlag(argv: string[], flag: string): string | undefined {
  const index = argv.indexOf(flag);
  return index >= 0 ? argv[index + 1] : undefined;
}

/** `kitron dev [dir]` — Rust build + Metro + Debug build + launch with env. */
async function dev(argv: string[]): Promise<number> {
  const appRoot = resolveAppRoot(argv.find((arg) => !arg.startsWith("-")));
  const port = Number(parseFlag(argv, "--port") ?? 8081);
  const platform =
    parseFlag(argv, "--platform") ??
    (process.platform === "win32" ? "windows" : "macos");
  if (platform !== "macos" && platform !== "windows") {
    log.fail(`unknown platform "${platform}" — use --platform macos|windows`);
  }
  // A Win32 GUI process cannot be cross-run from macOS — refuse cleanly (the
  // machine-step checklist lives in the app's windows/kitron/README.md).
  if (platform === "windows" && process.platform !== "win32") {
    log.fail(
      "kitron dev --platform windows runs on a Windows machine (MSVC toolchain) — see windows/kitron/README.md",
    );
  }
  const config = resolveBackend(appRoot, await loadAppConfig(appRoot));

  if (platform === "windows") {
    // Metro + the Windows host exe; stays attached until the app quits.
    await devWindows(appRoot, config, port);
    return 0;
  }

  syncPod(appRoot);
  ensurePods(appRoot);
  void (await startMetro(appRoot, port));
  const appPath = await buildXcode(appRoot, "Debug");
  syncNodeHost(appRoot);
  const env = kitronLaunchEnv(appRoot, config, true);
  launchApp(appPath, env);
  // Keep the CLI attached to the app + Metro until they exit.
  await new Promise(() => {});
  return 0;
}

/** `kitron build [dir]` — Release .app with backend + Node runtime embedded. */
async function build(argv: string[]): Promise<number> {
  const appRoot = resolveAppRoot(argv.find((arg) => !arg.startsWith("-")));
  const config = resolveBackend(appRoot, await loadAppConfig(appRoot));

  syncPod(appRoot);
  ensurePods(appRoot);
  await bundleJs(appRoot);
  const appPath = await buildXcode(appRoot, "Release");
  embedRuntimeAssets(appPath, appRoot, config);
  log.info(`built ${appPath}`);
  return 0;
}

/** Metro production bundle for the macOS platform. */
async function bundleJs(appRoot: string): Promise<void> {
  const outputDir = path.join(appRoot, "macos", "build", "bundle");
  fs.mkdirSync(outputDir, { recursive: true });
  const bundle = run(
    "npx",
    [
      "react-native",
      "bundle",
      "--platform",
      "macos",
      "--dev",
      "false",
      "--entry-file",
      "index.ts",
      "--bundle-output",
      path.join(outputDir, "main.jsbundle"),
      "--assets-dest",
      outputDir,
    ],
    { cwd: appRoot },
  );
  if (bundle.status !== 0) {
    throw new Error("react-native bundle failed");
  }
}

/** Copies the production JS bundle + embedded Node backend into the .app. */
function embedRuntimeAssets(appPath: string, appRoot: string, config: KitronConfig): void {
  const resources = path.join(appPath, "Contents", "Resources");
  const kitronDir = path.join(resources, "kitron");
  fs.rmSync(kitronDir, { recursive: true, force: true });
  fs.mkdirSync(kitronDir, { recursive: true });
  fs.mkdirSync(path.join(kitronDir, "node-host"), { recursive: true });

  const bundleDir = path.join(appRoot, "macos", "build", "bundle");
  fs.copyFileSync(
    path.join(bundleDir, "main.jsbundle"),
    path.join(resources, "main.jsbundle"),
  );
  // Assets (images, fonts) emitted by Metro next to the bundle.
  for (const entry of fs.readdirSync(bundleDir, { withFileTypes: true })) {
    if (entry.name === "main.jsbundle") {
      continue;
    }
    const from = path.join(bundleDir, entry.name);
    const to = path.join(resources, entry.name);
    if (entry.isDirectory()) {
      copyDir(from, to);
    } else {
      fs.copyFileSync(from, to);
    }
  }

  // Embedded Node.js backend (TypeScript runs natively on Node 24+); the
  // config entry stays relative to the packaged resources root.
  copyDir(path.join(appRoot, "backend"), path.join(kitronDir, "backend"));
  // Only the Node host bootstrap is needed at runtime — the Rust build
  // artifacts and Windows runtime sources stay in the dev tree.
  const nodeHostDir = path.join(kitronDir, "node-host");
  fs.mkdirSync(nodeHostDir, { recursive: true });
  const hostMjs = path.join(appRoot, ".kitron", "host.mjs");
  if (fs.existsSync(hostMjs)) {
    fs.copyFileSync(hostMjs, path.join(nodeHostDir, "host.mjs"));
  }
  copyDir(
    path.join(appRoot, "node_modules", "kitron"),
    path.join(kitronDir, "node_modules", "kitron"),
  );
  copyDir(
    path.join(appRoot, "node_modules", "@kitron"),
    path.join(kitronDir, "node_modules", "@kitron"),
  );

  // Serialize the app config for the packaged bootstrap.
  fs.writeFileSync(
    path.join(kitronDir, "app-config.json"),
    JSON.stringify(
      {
        app: config.app,
        window: config.window ?? {},
        node: {
          ...config.node,
          entry: config.node?.entry ?? "./backend/index.ts",
        },
        permissions: config.permissions ?? [],
        extra: config.extra ?? {},
      },
      null,
      2,
    ),
  );

  // The system Node binary as the packaged runtime (MVP §41 packaging gate).
  const nodeDir = path.join(kitronDir, "node", "bin");
  fs.mkdirSync(nodeDir, { recursive: true });
  const nodeTarget = path.join(nodeDir, "node");
  fs.copyFileSync(process.execPath, nodeTarget);
  stripBinary(nodeTarget);

  // Prune vendored SDK copies to their runtime artifacts only (drop TS sources,
  // tests and maps) — the embedded Node host only needs the compiled `dist/`.
  pruneVendored(path.join(kitronDir, "node_modules"));

  // Strip debug symbols from the packaged app binary and frameworks.
  const macosDir = path.join(appPath, "Contents", "MacOS");
  for (const entry of fs.readdirSync(macosDir)) {
    stripBinary(path.join(macosDir, entry));
  }
  const frameworksDir = path.join(appPath, "Contents", "Frameworks");
  if (fs.existsSync(frameworksDir)) {
    for (const entry of fs.readdirSync(frameworksDir, { withFileTypes: true })) {
      const full = path.join(frameworksDir, entry.name);
      if (entry.isFile()) stripBinary(full);
      else if (entry.isDirectory()) {
        for (const sub of fs.readdirSync(full)) {
          stripBinary(path.join(full, sub));
        }
      }
    }
  }
  // Stripping invalidates the ad-hoc signature; re-sign so the app launches.
  try {
    execFileSync("codesign", ["--force", "--deep", "--sign", "-", appPath], { stdio: "ignore" });
  } catch {
    // ignore
  }
}

function stripBinary(filePath: string): void {
  try {
    if (!fs.statSync(filePath).isFile()) return;
    // Only attempt on Mach-O / ELF-ish binaries; `strip` errors on text files.
    execFileSync("strip", ["-x", filePath], { stdio: "ignore" });
  } catch {
    // Non-binary or already-stripped files are fine.
  }
}

function pruneVendored(nodeModulesDir: string): void {
  if (!fs.existsSync(nodeModulesDir)) return;
  const walk = (dir: string) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        if (entry.name === "src" || entry.name === "test" || entry.name === "tests" || entry.name === "__tests__") {
          fs.rmSync(full, { recursive: true, force: true });
          continue;
        }
        walk(full);
      } else if (/\.(ts|map|md|d\.ts)$/i.test(entry.name)) {
        try { fs.rmSync(full, { force: true }); } catch { /* ignore */ }
      }
    }
  };
  walk(nodeModulesDir);
}

/** `kitron package [dir]` — wrap the Release .app into a macOS .dmg (§39). */
async function packageCommand(argv: string[]): Promise<number> {
  const appRoot = resolveAppRoot(argv.find((arg) => !arg.startsWith("-")));
  const appPath = findBuiltApp(appRoot, "Release");
  const appName = path.basename(appPath, ".app");
  const outDir = path.join(appRoot, "macos", "build", "package");
  const stage = path.join(outDir, "stage");
  fs.rmSync(stage, { recursive: true, force: true });
  fs.mkdirSync(stage, { recursive: true });
  fs.cpSync(appPath, path.join(stage, appName), { recursive: true, force: true });
  fs.symlinkSync("/Applications", path.join(stage, "Applications"));

  const dmg = path.join(outDir, `${appName}.dmg`);
  fs.rmSync(dmg, { force: true });
  log.info(`creating ${dmg}…`);
  const result = run("hdiutil", [
    "create",
    "-volname",
    appName,
    "-srcfolder",
    stage,
    "-ov",
    "-format",
    "UDZO",
    dmg,
  ]);
  fs.rmSync(stage, { recursive: true, force: true });
  if (result.status !== 0) {
    throw new Error("hdiutil failed");
  }
  log.info(`packaged ${dmg}`);
  return 0;
}

/** `kitron run [dir]` — launch the most recent Release build with env. */
async function runCommand(argv: string[]): Promise<number> {
  const appRoot = resolveAppRoot(argv.find((arg) => !arg.startsWith("-")));
  const config = resolveBackend(appRoot, await loadAppConfig(appRoot));
  const appPath = findBuiltApp(appRoot, "Release");
  launchApp(appPath, kitronLaunchEnv(appRoot, config, false));
  await new Promise(() => {});
  return 0;
}

/**
 * `kitron update [dir] [--sdk <checkout>]` — re-vendor the runtime (SDK
 * packages, CLI, Rust workspace, pod sources, templates) into an existing
 * app. The natural follow-up to editing the SDK in its checkout: the app
 * builds against the runtime it ships with, so that shipping runtime must be
 * refreshed in place — `create` cannot be re-run over a working app.
 */
async function updateCommand(argv: string[]): Promise<number> {
  const appRoot = resolveAppRoot(argv.find((arg) => !arg.startsWith("-")));
  const sdkFlag = parseFlag(argv, "--sdk");

  if (!fs.existsSync(path.join(appRoot, "kitron.config.ts"))) {
    log.fail(`${appRoot} is not a Kitron app (no kitron.config.ts)`);
  }

  const sourceRoot = resolveUpdateSourceRoot(sdkFlag);
  if (!sourceRoot) {
    console.error(
      "kitron update needs the SDK sources. The CLI running vendored inside\n" +
        "an app cannot self-update: install the kitron CLI globally\n" +
        "(or run it from a checkout) and retry, or pass --sdk <checkout>.",
    );
    return 1;
  }

  log.info(`updating ${appRoot} from ${sourceRoot}`);
  vendorSdkPackages(appRoot, sourceRoot);
  vendorRuntime(appRoot, sourceRoot, loadTypescript());
  syncNodeHost(appRoot);
  vendorWindowsTemplate(appRoot);

  // Stamp for diagnostics (and for tests to observe the refresh).
  fs.mkdirSync(path.join(appRoot, ".kitron"), { recursive: true });
  fs.writeFileSync(
    path.join(appRoot, ".kitron", "sdk-vendor.json"),
    JSON.stringify({ vendoredAt: new Date().toISOString(), sdk: sourceRoot }, null, 2) + "\n",
  );

  log.info("vendored @kitron/* packages, the CLI and the runtime into the app.");
  console.log(
    [
      "",
      "Next steps:",
      "  • pod sources and the static lib refresh on the next `kitron dev` / `kitron build`",
      "  • run `pod install` only when the podspec or bridge headers changed",
    ].join("\n"),
  );
  return 0;
}

export async function main(argv: string[]): Promise<number> {
  const [command, ...rest] = argv;
  switch (command) {
    case "create":
      return create(rest);
    case "update":
      return updateCommand(rest);
    case "doctor":
      return doctor();
    case "dev":
      return dev(rest);
    case "build":
      return build(rest);
    case "package":
      return packageCommand(rest);
    case "run":
      return runCommand(rest);
    case "--help":
    case "-h":
    case undefined:
      console.log(USAGE);
      return command === undefined ? 1 : 0;
    default:
      console.error(USAGE);
      return 1;
  }
}

// Silence unused warnings for genuinely optional CLI helpers.
void discoverProject;
