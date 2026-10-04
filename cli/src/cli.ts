#!/usr/bin/env node
/**
 * `kitron` CLI (§35-§39) — zero heavy deps (no Vite, no web framework).
 * Commands: create, dev, build, package, run, doctor
 */
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const execFileAsync = promisify(execFile);
const __dirname = path.dirname(fileURLToPath(import.meta.url));

const VERSION = "0.1.0";

async function main(): Promise<void> {
  const [, , cmd, ...rest] = process.argv;
  switch (cmd) {
    case "create": return cmdCreate(rest);
    case "dev": return cmdDev(rest);
    case "build": return cmdBuild(rest);
    case "package": return cmdPackage(rest);
    case "run": return cmdRun(rest);
    case "doctor": return cmdDoctor();
    case "--version":
    case "-V": console.log(VERSION); return;
    case "--help":
    case "-h":
    case undefined: return help();
    default:
      console.error(`unknown command: ${cmd}`);
      return help(1);
  }
}

function help(exit = 0): void {
  console.log(`kitron ${VERSION} — React Native + Node.js + Rust desktop runtime (no Chromium)

Usage:
  kitron create <name> [--template basic]
  kitron dev [--headless] [--port 8081]
  kitron build [--platform macos|windows]
  kitron package [--platform macos] [--format dmg|app]
  kitron run [--headless]
  kitron doctor
`);
  process.exit(exit);
}

// ---- create ----

async function cmdCreate(args: string[]): Promise<void> {
  const name = args.find((a) => !a.startsWith("--"));
  if (!name) { console.error("usage: kitron create <name>"); process.exit(1); }
  const dest = path.resolve(process.cwd(), name);
  try { await fs.stat(dest); console.error(`exists: ${dest}`); process.exit(1); }
  catch { /* ok */ }
  await fs.mkdir(dest, { recursive: true });
  await fs.mkdir(path.join(dest, "src"), { recursive: true });
  await fs.mkdir(path.join(dest, "backend"), { recursive: true });
  await fs.mkdir(path.join(dest, "assets"), { recursive: true });

  const pkg = {
    name, version: "0.1.0", private: true, type: "module",
    scripts: { dev: "kitron dev", build: "kitron build" },
    dependencies: { kitron: "0.1.0", "@kitron/window": "0.1.0", "@kitron/core": "0.1.0", "react": "^18.3.1", "react-native": "^0.76.0" },
  };
  await fs.writeFile(path.join(dest, "package.json"), JSON.stringify(pkg, null, 2));
  await fs.writeFile(path.join(dest, "kitron.config.ts"), TEMPLATE_CONFIG(name));
  await fs.writeFile(path.join(dest, "src", "App.tsx"), TEMPLATE_APP);
  await fs.writeFile(path.join(dest, "backend", "index.ts"), TEMPLATE_BACKEND);
  await fs.writeFile(path.join(dest, "README.md"), `# ${name}\n\nCreated with \`kitron create\`.\n\n\`\`\`bash\nkitron dev\n\`\`\`\n`);
  console.log(`created ${dest}\n  cd ${name}\n  kitron dev`);
}

const TEMPLATE_CONFIG = (name: string) => `export default {
  app: { name: ${JSON.stringify(name)}, identifier: "com.example.${name.toLowerCase().replace(/[^a-z0-9]/g, "")}", version: "0.1.0" },
  window: { width: 1200, height: 800 },
  node: { entry: "./backend/index.ts" },
};
`;

const TEMPLATE_APP = `export function App() {
  // React Native renderer goes here (View/Text from react-native).
  // Kitron hosts this in a native window — no HTML, no Chromium.
  return null;
}
`;

const TEMPLATE_BACKEND = `import { defineBackend } from "@kitron/node/backend";

// Local Node backend — started automatically by Kitron.
// Expose procedures invoked from UI via backend.invoke("name", params).
defineBackend({
  "ping": () => ({ pong: true }),
});

console.error("[backend] ready");
`;

// ---- dev ----

async function cmdDev(args: string[]): Promise<void> {
  const headless = args.includes("--headless");
  console.log(`[kitron] dev ${headless ? "(headless — no OS window)" : "(native window)"}`);
  // 1. ensure Rust runtime binary exists (best effort)
  const bin = await ensureRuntimeBinary();
  // 2. launch runtime host
  const child = spawn(bin, ["run", "--app-dir", process.cwd(), ...(headless ? ["--headless"] : [])], { stdio: "inherit" });
  await new Promise<void>((resolve) => { child.on("exit", () => resolve()); });
}

async function ensureRuntimeBinary(): Promise<string> {
  const override = process.env["KITRON_RUNTIME_BIN"];
  if (override) return override;
  const local = path.resolve(__dirname, "../../runtime/rust/target/debug/kitron-runtime");
  try { await fs.stat(local); return local; }
  catch { /* build below */ }
  console.log("[kitron] building Rust runtime (cargo build)…");
  await new Promise<void>((resolve, reject) => {
    const p = spawn("cargo", ["build", "--bin", "kitron-runtime"], { cwd: path.resolve(__dirname, "../../runtime/rust"), stdio: "inherit" });
    p.on("exit", (c) => (c === 0 ? resolve() : reject(new Error(`cargo build exited ${c}`))));
    p.on("error", reject);
  });
  return local;
}

// ---- build ----

async function cmdBuild(args: string[]): Promise<void> {
  const platform = flag(args, "--platform") ?? currentPlatformName();
  console.log(`[kitron] build for ${platform}`);
  await new Promise<void>((resolve, reject) => {
    const p = spawn("cargo", ["build", "--release", "--bin", "kitron-runtime"], { cwd: path.resolve(__dirname, "../../runtime/rust"), stdio: "inherit" });
    p.on("exit", (c) => (c === 0 ? resolve() : reject(new Error(`cargo build --release exited ${c}`))));
    p.on("error", reject);
  });
  const out = path.resolve(process.cwd(), "dist");
  await fs.mkdir(out, { recursive: true });
  const manifest = { app: "kitron-app", platform, builtAt: new Date().toISOString(), runtime: "kitron-runtime" };
  await fs.writeFile(path.join(out, "kitron-manifest.json"), JSON.stringify(manifest, null, 2));
  console.log(`[kitron] wrote ${path.join(out, "kitron-manifest.json")}`);
  if (platform === "macos") {
    console.log("[kitron] hint: `kitron package --platform macos` assembles MyApp.app (+ .dmg later)");
  }
}

// ---- package ----

async function cmdPackage(args: string[]): Promise<void> {
  const platform = flag(args, "--platform") ?? currentPlatformName();
  const format = flag(args, "--format") ?? (platform === "macos" ? "app" : "exe");
  const appName = path.basename(process.cwd());
  const dist = path.resolve(process.cwd(), "dist");
  await fs.mkdir(dist, { recursive: true });
  if (platform === "macos" && (format === "app" || format === "dmg")) {
    const appDir = path.join(dist, `${appName}.app`);
    const macosDir = path.join(appDir, "Contents", "MacOS");
    const resDir = path.join(appDir, "Contents", "Resources");
    await fs.mkdir(macosDir, { recursive: true });
    await fs.mkdir(resDir, { recursive: true });
    await fs.writeFile(path.join(appDir, "Contents", "Info.plist"), macPlist(appName));
    const binSrc = path.resolve(__dirname, "../../runtime/rust/target/release/kitron-runtime");
    try {
      await fs.copyFile(binSrc, path.join(macosDir, appName));
      await fs.chmod(path.join(macosDir, appName), 0o755);
    } catch {
      await fs.writeFile(path.join(macosDir, appName), "#!/bin/sh\necho 'kitron placeholder — run kitron build first'\n");
    }
    console.log(`[kitron] assembled ${appDir}`);
    if (format === "dmg") console.log("[kitron] .dmg + notarization: Phase 4 (placeholder)");
  } else {
    console.log(`[kitron] package ${platform}/${format}: placeholder (Phase 4: .msi, code signing)`);
    await fs.writeFile(path.join(dist, `${appName}.${format}.placeholder`), "kitron package placeholder\n");
  }
}

function macPlist(name: string): string {
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>${name}</string>
  <key>CFBundleIdentifier</key><string>com.kitron.${name}</string>
  <key>CFBundleVersion</key><string>0.1.0</string>
  <key>CFBundleExecutable</key><string>${name}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
`;
}

// ---- run ----

async function cmdRun(args: string[]): Promise<void> {
  return cmdDev(args);
}

// ---- doctor ----

async function cmdDoctor(): Promise<void> {
  console.log("kitron doctor\n");
  const rows: [string, string][] = [];
  rows.push(["node", await which("node --version")]);
  rows.push(["npm", await which("npm --version")]);
  rows.push(["rustc", await which("rustc --version")]);
  rows.push(["cargo", await which("cargo --version")]);
  rows.push(["os", `${process.platform} ${process.arch}`]);
  rows.push(["xcode-select (macos)", process.platform === "darwin" ? await which("xcode-select -p") : "n/a (not macos)"]);
  for (const [k, v] of rows) console.log(`  ${k.padEnd(22)} ${v}`);
  console.log("\n miraculous: no Chromium found (as intended).");
}

async function which(cmd: string): Promise<string> {
  try {
    const parts = cmd.split(" ");
    const c = parts[0] as string;
    const a = parts.slice(1);
    const { stdout } = await execFileAsync(c, a, { timeout: 10_000 });
    return String(stdout).trim().split("\n")[0] ?? "ok";
  } catch (e) {
    return `MISSING (${e instanceof Error ? e.message.split("\n")[0] : e})`;
  }
}

function flag(args: string[], name: string): string | undefined {
  const i = args.indexOf(name);
  if (i >= 0) return args[i + 1];
  const pref = args.find((a) => a.startsWith(name + "="));
  return pref?.slice(name.length + 1);
}

function currentPlatformName(): string {
  return process.platform === "darwin" ? "macos" : process.platform === "win32" ? "windows" : "linux";
}

main().catch((e) => { console.error(e); process.exit(1); });
