/** Kitron CLI dispatch + the dev/build/run commands (spec §37/§41). */

import path from "node:path";
import process from "node:process";

import type { KitronConfig } from "@kitron/core";

import { loadAppConfig, log, resolveAppRoot, copyDir, run } from "./lib.ts";
import { doctor } from "./doctor.ts";
import { create, syncNodeHost } from "./create.ts";
import {
  buildXcode,
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
  kitron create <dir> [--name N] [--identifier ID]
  kitron doctor
  kitron dev     [dir] [--port 8081]
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
  const config = await loadAppConfig(appRoot);

  syncPod(appRoot);
  ensurePods(appRoot);
  void (await startMetro(appRoot, port));
  const appPath = await buildXcode(appRoot, "Debug");
  syncNodeHost(appRoot);
  const config2 = config;
  const env = kitronLaunchEnv(appRoot, config2, true);
  launchApp(appPath, env);
  // Keep the CLI attached to the app + Metro until they exit.
  await new Promise(() => {});
  return 0;
}

/** `kitron build [dir]` — Release .app with backend + Node runtime embedded. */
async function build(argv: string[]): Promise<number> {
  const appRoot = resolveAppRoot(argv.find((arg) => !arg.startsWith("-")));
  const config = await loadAppConfig(appRoot);

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
  copyDir(path.join(appRoot, ".kitron"), path.join(kitronDir, "node-host"));
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
  fs.copyFileSync(process.execPath, path.join(nodeDir, "node"));
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
  const config = await loadAppConfig(appRoot);
  const appPath = findBuiltApp(appRoot, "Release");
  launchApp(appPath, kitronLaunchEnv(appRoot, config, false));
  await new Promise(() => {});
  return 0;
}

export async function main(argv: string[]): Promise<number> {
  const [command, ...rest] = argv;
  switch (command) {
    case "create":
      return create(rest);
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
