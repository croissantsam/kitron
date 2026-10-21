/** `kitron create <dir> [--name N] [--identifier ID]` — scaffold an app. */

import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { createRequire } from "node:module";

import {
  APP_TEMPLATE_ROOT,
  NODE_HOST_TEMPLATE,
  POD_TEMPLATE_ROOT,
  SDK_ROOT,
  buildRustStaticLib,
  copyDir,
  log,
  run,
} from "./lib.ts";

interface CreateArgs {
  destination: string;
  name?: string;
  identifier?: string;
  skipInstall: boolean;
  skipMacosSetup: boolean;
}

function parseArgs(argv: string[]): CreateArgs {
  let destination = "";
  let name: string | undefined;
  let identifier: string | undefined;
  let skipInstall = process.env.KITRON_CREATE_SKIP_INSTALL === "1";
  let skipMacosSetup = process.env.KITRON_CREATE_SKIP_MACOS === "1";
  for (let index = 0; index < argv.length; index += 1) {
    const arg: string | undefined = argv[index];
    if (arg === undefined) {
      continue;
    }
    if (arg === "--name") {
      name = argv[++index];
    } else if (arg === "--identifier") {
      identifier = argv[++index];
    } else if (arg === "--skip-install") {
      skipInstall = true;
    } else if (arg === "--skip-macos-setup") {
      skipMacosSetup = true;
    } else if (!arg.startsWith("-")) {
      destination = arg;
    }
  }
  if (!destination) {
    throw new Error("usage: kitron create <dir> [--name N] [--identifier ID]");
  }
  return {
    destination: path.resolve(destination),
    name,
    identifier,
    skipInstall,
    skipMacosSetup,
  };
}

const APPLE_BUNDLE_ID = /^[a-zA-Z][a-zA-Z0-9-]*(\.[a-zA-Z0-9-]+)+$/;

function sanitizeProjectName(raw: string): string {
  return raw.replace(/[^a-zA-Z0-9]/g, "");
}

function slugifyIdentifier(raw: string): string {
  const cleaned = raw
    .toLowerCase()
    .replace(/[^a-z0-9.]+/g, "-")
    .replace(/^[-.]+|[-.]+$/g, "");
  return APPLE_BUNDLE_ID.test(cleaned) ? cleaned : `dev.kitron.app.${cleaned}`;
}

function rewriteTemplate(files: string[], replacements: Array<[RegExp, string]>) {
  for (const file of files) {
    let source = fs.readFileSync(file, "utf8");
    for (const [pattern, replacement] of replacements) {
      source = source.replace(pattern, replacement);
    }
    fs.writeFileSync(file, source);
  }
}

/** Builds the Rust staticlib and syncs it + the Kitron pod into `macos/`. */
export function syncKitronPod(appRoot: string): void {
  const staticLib = buildRustStaticLib();

  const podDir = path.join(appRoot, "macos", "Kitron");
  fs.mkdirSync(podDir, { recursive: true });
  fs.copyFileSync(staticLib, path.join(podDir, "libkitron_macos.a"));
  // Headers + bridge sources live within the pod dir relative to the podspec.
  copyDir(path.join(POD_TEMPLATE_ROOT, "Kitron"), path.join(podDir, "Kitron"));
  fs.copyFileSync(
    path.join(POD_TEMPLATE_ROOT, "Kitron.podspec"),
    path.join(podDir, "Kitron.podspec"),
  );
}

/** Adds `pod 'Kitron', :path => 'Kitron'` to the app's macOS Podfile. */
export function patchPodfile(appRoot: string): void {
  const podfile = path.join(appRoot, "macos", "Podfile");
  if (!fs.existsSync(podfile)) {
    throw new Error(`missing ${podfile} — run generate-macos first`);
  }
  let source = fs.readFileSync(podfile, "utf8");
  if (source.includes("pod 'Kitron'")) {
    return;
  }
  source = source.replace(
    /(\n\s*use_native_modules!)/,
    "\n  # Kitron: Rust core + JSI host object (link vendored staticlib)\n  pod 'Kitron', :path => 'Kitron'$1",
  );
  fs.writeFileSync(podfile, source);
}

/**
 * Points the Xcode "Bundle React Native code and images" phase at the app's
 * TypeScript entry (`index.ts`). Without it the phase looks for `index.js` and
 * a Release build from Xcode fails.
 */
export function patchXcodeProject(appRoot: string): void {
  const macosDir = path.join(appRoot, "macos");
  const projects = fs
    .readdirSync(macosDir)
    .filter((entry) => entry.endsWith(".xcodeproj"));
  for (const project of projects) {
    const pbxproj = path.join(macosDir, project, "project.pbxproj");
    if (!fs.existsSync(pbxproj)) {
      continue;
    }
    const source = fs.readFileSync(pbxproj, "utf8");
    if (source.includes("ENTRY_FILE")) {
      continue;
    }
    // Project-level build settings only (they carry ALWAYS_SEARCH_USER_PATHS).
    const patched = source.replace(
      /(buildSettings = \{\n(\s*)ALWAYS_SEARCH_USER_PATHS = NO;)/g,
      "$1\n$2ENTRY_FILE = index.ts;",
    );
    fs.writeFileSync(pbxproj, patched);
  }
}

/** Ensures the embedded Node host bootstrap exists at `.kitron/host.mjs`. */
export function syncNodeHost(appRoot: string): void {
  const hostDir = path.join(appRoot, ".kitron");
  fs.mkdirSync(hostDir, { recursive: true });
  fs.copyFileSync(NODE_HOST_TEMPLATE, path.join(hostDir, "host.mjs"));
}

/** Copies the SDK packages into the app's node_modules (flat, standalone). */
export function vendorSdkPackages(appRoot: string, pkgRoot: string): void {
  const nodeModules = path.join(appRoot, "node_modules");
  const map: Array<[string, string]> = [
    ["packages/core", "@kitron/core"],
    ["packages/window", "@kitron/window"],
    ["packages/node", "@kitron/node"],
    ["packages/clipboard", "@kitron/clipboard"],
    ["packages/dialog", "@kitron/dialog"],
    ["packages/menu", "@kitron/menu"],
    ["packages/tray", "@kitron/tray"],
    ["packages/notification", "@kitron/notification"],
    ["packages/process", "@kitron/process"],
    ["packages/shell", "@kitron/shell"],
    ["packages/shortcut", "@kitron/shortcut"],
    ["packages/updater", "@kitron/updater"],
    ["packages/kitron", "kitron"],
  ];
  const typescript = loadTypescript();
  for (const [from, to] of map) {
    const source = path.join(pkgRoot, from);
    const destination = path.join(nodeModules, to);
    fs.rmSync(destination, { recursive: true, force: true });
    fs.mkdirSync(destination, { recursive: true });
    const packageJson = JSON.parse(
      fs.readFileSync(path.join(source, "package.json"), "utf8"),
    ) as Record<string, unknown> & {
      dependencies?: Record<string, string>;
      main?: string;
      types?: string;
      exports?: Record<string, unknown>;
    };
    // Standalone copies don't use the pnpm workspace protocol.
    for (const key of Object.keys(packageJson.dependencies ?? {})) {
      if (packageJson.dependencies?.[key] === "workspace:*") {
        packageJson.dependencies[key] = "*";
      }
    }
    // Vendored packages ship compiled JS so the embedded Node host (plain ESM)
    // and Metro can both consume them directly — Node refuses TS stripping for
    // files under node_modules. TS sources stay vendored for the app's
    // type-checking (the `types` condition points back at them).
    transpilePackage(typescript, path.join(source, "src"), path.join(destination, "dist"));
    copyDir(path.join(source, "src"), path.join(destination, "src"));
    packageJson.main = "./dist/index.js";
    packageJson.types = "./src/index.ts";
    // Canonical ESM export map (types condition first).
    packageJson.exports = {
      ".": {
        types: "./src/index.ts",
        default: "./dist/index.js",
      },
    };
    fs.writeFileSync(
      path.join(destination, "package.json"),
      JSON.stringify(packageJson, null, 2) + "\n",
    );
    for (const file of ["LICENSE", "README.md"]) {
      const sourceFile = path.join(source, file);
      if (fs.existsSync(sourceFile)) {
        fs.copyFileSync(sourceFile, path.join(destination, file));
      }
    }
  }
}

function loadTypescript(): typeof import("typescript") {
  const require = createRequire(import.meta.url);
  // The CLI depends on typescript so it is guaranteed resolvable here.
  return require("typescript") as typeof import("typescript");
}

/** Emits ESM JS from a package's TS sources (no declarations: the app's
 * TypeScript resolves types through the vendored `src/*.ts` files, referenced
 * from the rewired package.json `types` field). Per-file `transpileModule`
 * behaves exactly like the compiled sources for our isolatedModules-safe SDK,
 * with relative `.js` specifiers passing through unchanged. */
function transpilePackage(
  ts: typeof import("typescript"),
  sourceDir: string,
  outDir: string,
): void {
  const compilerOptions: import("typescript").CompilerOptions = {
    ...ts.getDefaultCompilerOptions(),
    target: ts.ScriptTarget.ES2023,
    module: ts.ModuleKind.ESNext,
    esModuleInterop: true,
    skipLibCheck: true,
    moduleDetection: ts.ModuleDetectionKind.Force,
  };
  const sources = fs
    .readdirSync(sourceDir)
    .filter((name) => name.endsWith(".ts") && !name.endsWith(".d.ts"))
    .sort();
  if (sources.length === 0) {
    throw new Error(`no TypeScript sources under ${sourceDir}`);
  }
  for (const name of sources) {
    const fileName = path.join(sourceDir, name);
    const output = ts.transpileModule(fs.readFileSync(fileName, "utf8"), {
      fileName,
      reportDiagnostics: true,
      compilerOptions,
    });
    const diagnostics = output.diagnostics ?? [];
    if (diagnostics.length > 0) {
      const text = ts.formatDiagnostics(diagnostics, {
        getCurrentDirectory: () => sourceDir,
        getCanonicalFileName: (file) => file,
        getNewLine: () => "\n",
      });
      throw new Error(`SDK transpile failed for ${fileName}:\n${text}`);
    }
    const outName = name.slice(0, -3) + ".js";
    fs.mkdirSync(outDir, { recursive: true });
    fs.writeFileSync(path.join(outDir, outName), output.outputText);
  }
}

export async function create(argv: string[]): Promise<number> {
  const args = parseArgs(argv);
  const destination = args.destination;

  if (fs.existsSync(path.join(destination, "package.json"))) {
    throw new Error(`refusing to scaffold into non-empty app ${destination}`);
  }

  const projectName =
    sanitizeProjectName(args.name ?? path.basename(destination)) || "KitronApp";
  const identifier =
    args.identifier ?? slugifyIdentifier(projectName.toLowerCase());

  log.info(`scaffolding ${projectName} → ${destination}`);
  fs.mkdirSync(path.dirname(destination), { recursive: true });
  copyDir(APP_TEMPLATE_ROOT, destination);
  fs.renameSync(path.join(destination, "gitignore"), path.join(destination, ".gitignore"));

  // Template placeholders: name + identifier in package.json & config & README.
  rewriteTemplate(
    [
      path.join(destination, "package.json"),
      path.join(destination, "kitron.config.ts"),
      path.join(destination, "README.md"),
    ],
    [
      [/HelloKitron/g, projectName],
      [/dev\.croissantlabs\.hello-kitron/g, identifier],
    ],
  );

  // 1. Install JS dependencies.
  if (!args.skipInstall) {
    log.info("installing dependencies (pnpm)…");
    const install = run("pnpm", ["install"], { cwd: destination });
    if (install.status !== 0) {
      log.warn("pnpm failed — retrying with npm");
      const npmInstall = run("npm", ["install"], { cwd: destination });
      if (npmInstall.status !== 0) {
        throw new Error("dependency install failed");
      }
    }
  }

  // 2. Vendor the Kitron SDK packages for standalone resolution.
  vendorSdkPackages(destination, SDK_ROOT);
  syncNodeHost(destination);

  // 3. Generate the macOS Xcode project via react-native-macos's generator.
  const require = createRequire(import.meta.url);
  if (!args.skipMacosSetup) {
    const generatorPath = path.join(
      destination,
      "node_modules",
      "react-native-macos",
      "local-cli",
      "generate-macos.js",
    );
    if (!fs.existsSync(generatorPath)) {
      throw new Error(
        `react-native-macos not installed under ${destination}; re-run with --skip-macos-setup after installing deps`,
      );
    }
    const generateMacOS: (
      projectDir: string,
      name: string,
      options: { overwrite?: boolean },
    ) => void = require(generatorPath);
    log.info("generating macos/ project…");
    generateMacOS(destination, projectName, { overwrite: true });

    // 4. Kitron-native wiring.
    const appDelegate = path.join(
      destination,
      "macos",
      `${projectName}-macOS`,
      "AppDelegate.swift",
    );
    fs.copyFileSync(
      path.join(APP_TEMPLATE_ROOT, "kitron-macos", "AppDelegate.swift"),
      appDelegate,
    );

    patchPodfile(destination);
    patchXcodeProject(destination);
    syncKitronPod(destination);

    log.info("pod install (this can take a few minutes)…");
    const pod = run("pod", ["install"], {
      cwd: path.join(destination, "macos"),
      env: { RCT_NEW_ARCH_ENABLED: "1" },
    });
    if (pod.status !== 0) {
      log.warn("pod install failed — run it manually with RCT_NEW_ARCH_ENABLED=1");
      return 1;
    }
  }

  log.info(
    `done. next: cd ${path.basename(destination)} && kitron dev`,
  );
  return 0;
}
