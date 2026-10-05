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
  kitron dev [--headless] [--permissions shell,process]
  kitron build [--platform macos|windows]
  kitron package [--platform macos] [--format app|dmg|pkg] [--sign <identity>] [--notarize]
  kitron package [--platform windows] [--format zip]
  kitron run [--headless] [--permissions shell,process]
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
    dependencies: await kitronDeps(dest),
  };
  await fs.writeFile(path.join(dest, "package.json"), JSON.stringify(pkg, null, 2));
  await fs.writeFile(path.join(dest, "kitron.config.ts"), TEMPLATE_CONFIG(name));
  await fs.writeFile(path.join(dest, "tsconfig.json"), TEMPLATE_TSCONFIG);
  await fs.writeFile(path.join(dest, "src", "App.tsx"), TEMPLATE_APP);
  await fs.writeFile(path.join(dest, "backend", "index.ts"), TEMPLATE_BACKEND);
  await fs.writeFile(path.join(dest, "README.md"), `# ${name}\n\nCreated with \`kitron create\`.\n\n\`\`\`bash\nkitron dev\n\`\`\`\n`);
  console.log(`created ${dest}\n  cd ${name}\n  kitron dev`);
}

/**
 * @kitron/* deps for a new app. Against a local Kitron checkout (the usual
 * `npm link` dev setup) we emit `file:` deps so `npm install` works before
 * anything is published; from an installed CLI we use version ranges.
 */
async function kitronDeps(appDir: string): Promise<Record<string, string>> {
  const repoRoot = path.resolve(__dirname, "..", "..");
  const names = ["kitron", "@kitron/core", "@kitron/window", "@kitron/react", "@kitron/node"];
  let local = false;
  try {
    local = (await fs.stat(path.join(repoRoot, "packages"))).isDirectory();
  } catch {
    local = false;
  }
  const deps: Record<string, string> = { react: "^19.3.0" };
  for (const n of names) {
    if (local) {
      const sub = n === "kitron" ? "packages/kitron" : `packages/${n.slice("@kitron/".length)}`;
      deps[n] = `file:${path.relative(appDir, path.join(repoRoot, sub)) || "."}`;
    } else {
      deps[n] = "0.1.0";
    }
  }
  return deps;
}

const TEMPLATE_CONFIG = (name: string) => `import { defineConfig } from "@kitron/core/config";

export default defineConfig({
  app: { name: ${JSON.stringify(name)}, identifier: "com.example.${name.toLowerCase().replace(/[^a-z0-9]/g, "")}", version: "0.1.0" },
  window: { width: 1200, height: 800 },
  node: { entry: "./backend/index.ts" },
  // Sensitive capabilities are denied unless listed here (§23).
  permissions: [],
});
`;

const TEMPLATE_APP = `import { useState } from "react";
import { Button, Text, View, render } from "@kitron/react";
import { window } from "kitron";

// Mainline React, rendered by Kitron's own reconciler host —
// no react-native fork, no webview, no Chromium (see docs/UI.md).
const main = await window.create({ width: 1200, height: 800, title: "My App" });

function App() {
  const [count, setCount] = useState(0);
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center", background: "#ffffff" }}>
      <Text style={{ fontSize: 28, color: "#111111" }}>Hello Kitron</Text>
      <Text style={{ fontSize: 14, color: "#666666" }}>Clicked {count} times</Text>
      <Button title="Click me" onPress={() => setCount((c) => c + 1)} />
    </View>
  );
}

await render(<App />, main.id);
`;

const TEMPLATE_TSCONFIG = `{
  "compilerOptions": {
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "jsx": "react-jsx",
    "strict": true,
    "skipLibCheck": true,
    "noEmit": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "forceConsistentCasingInFileNames": true
  },
  "include": ["src", "backend", "kitron.config.ts"]
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
  const permissions = flag(args, "--permissions");
  console.log(`[kitron] dev ${headless ? "(headless — no OS window)" : "(native window)"}`);
  const bin = await ensureRuntimeBinary();
  const sock = socketPath(process.cwd());

  // 1. window host (+ Node sidecar) with a UI socket for app code.
  const rtArgs = ["run", "--app-dir", process.cwd(), "--socket", sock, ...(headless ? ["--headless"] : [])];
  if (permissions) rtArgs.push("--permissions", permissions);
  const rt = spawn(bin, rtArgs, { stdio: "inherit" });
  const killRt = () => {
    try { rt.kill(); } catch { /* ignore */ }
  };
  try {
    await waitForSocket(sock);
  } catch (e) {
    killRt();
    throw e;
  }

  // 2. bundle + run the UI entry (src/App.tsx). Nothing else executes it.
  const ui = await startUiEntry(process.cwd(), sock);
  const code = await new Promise<number | null>((resolve) => {
    rt.on("exit", (c) => resolve(c));
    ui.on("exit", (c) => resolve(c));
  });
  try { ui.kill(); } catch { /* ignore */ }
  killRt();
  try { await fs.unlink(sock); } catch { /* ignore */ }
  if (code !== 0 && code !== null) process.exitCode = code;
}

/** Deterministic socket path per app directory. */
function socketPath(appDir: string): string {
  const tmp = process.env["TMPDIR"] ?? "/tmp";
  const safe = path.basename(appDir).toLowerCase().replace(/[^a-z0-9]/g, "-");
  return path.join(tmp, `kitron-${safe}.sock`);
}

async function waitForSocket(sock: string): Promise<void> {
  const { default: net } = await import("node:net");
  const deadline = Date.now() + 15_000;
  for (;;) {
    const ok = await new Promise<boolean>((resolve) => {
      const s = net.createConnection(sock);
      s.once("connect", () => {
        s.destroy();
        resolve(true);
      });
      s.once("error", () => resolve(false));
    });
    if (ok) return;
    if (Date.now() > deadline) throw new Error(`runtime socket never appeared: ${sock}`);
    await new Promise((r) => setTimeout(r, 100));
  }
}

/**
 * Bundle src/App.tsx with esbuild and run it with the bridge pre-connected.
 * Returns the UI child process (its exit ends `dev`, like Metro bundling).
 */
async function startUiEntry(appDir: string, sock: string): Promise<ReturnType<typeof spawn>> {
  const { default: esbuild } = await import("esbuild");
  const entry = path.join(appDir, "src", "App.tsx");
  try {
    await fs.stat(entry);
  } catch {
    throw new Error(`missing UI entry: ${entry} (kitron dev runs src/App.tsx)`);
  }
  const outDir = path.join(process.env["TMPDIR"] ?? "/tmp", `kitron-ui-${process.pid}`);
  await fs.mkdir(outDir, { recursive: true });
  const bundle = path.join(outDir, "app.mjs");
  // Single bundle: connect the bridge FIRST, then run App (top-level await
  // in App.tsx executes on import). One module graph = one bridge singleton.
  const entrySource = [
    `import { SocketTransport, setBridgeTransport } from "@kitron/core";`,
    `const __kitronTransport = new SocketTransport({ path: process.env["KITRON_SOCKET"] });`,
    `await __kitronTransport.ready;`,
    `setBridgeTransport(__kitronTransport);`,
    `await import(${JSON.stringify(entry)});`,
    ``,
  ].join("\n");
  await esbuild.build({
    stdin: { contents: entrySource, loader: "ts", resolveDir: appDir },
    bundle: true,
    platform: "node",
    format: "esm",
    outfile: bundle,
    absWorkingDir: appDir,
    logLevel: "warning",
    // One React only: the reconciler must share internals with the app's
    // react copy, never the Kitron workspace's (duplicate React breaks hooks).
    alias: {
      react: path.join(appDir, "node_modules", "react"),
    },
  });
  console.log("[kitron] UI entry bundled, connecting to runtime…");
  return spawn("node", [bundle], {
    cwd: appDir,
    stdio: "inherit",
    env: { ...process.env, KITRON_SOCKET: sock },
  });
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
    console.log("[kitron] hint: `kitron package --platform macos --format dmg --sign <identity>` for distribution");
  }
}

// ---- package ----

async function cmdPackage(args: string[]): Promise<void> {
  const platform = flag(args, "--platform") ?? currentPlatformName();
  const format = flag(args, "--format") ?? (platform === "macos" ? "app" : "zip");
  const sign = flag(args, "--sign");
  const notarize = args.includes("--notarize");
  const appName = path.basename(process.cwd());
  const dist = path.resolve(process.cwd(), "dist");
  await fs.mkdir(dist, { recursive: true });
  if (platform === "macos" && ["app", "dmg", "pkg"].includes(format)) {
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
    if (sign) await codesign(appDir, sign);
    else console.log("[kitron] unsigned build (pass --sign <identity> for distribution)");
    if (format === "dmg") await makeDmg(appDir, path.join(dist, `${appName}.dmg`), notarize);
    if (format === "pkg") await makePkg(appDir, path.join(dist, `${appName}.pkg`), notarize);
  } else if (platform === "windows") {
    // Windows packaging: zip the release binary + manifest. MSI via WiX
    // remains Phase 4 (needs Windows SDK); the zip is installable as-is.
    const binSrc = path.resolve(__dirname, "../../runtime/rust/target/release/kitron-runtime.exe");
    const zipPath = path.join(dist, `${appName}-windows.zip`);
    try {
      await fs.stat(binSrc);
    } catch {
      console.log("[kitron] no release binary found — run `kitron build --platform windows` on Windows first");
      await fs.writeFile(zipPath + ".placeholder", "kitron package placeholder\n");
      return;
    }
    // PowerShell Compress-Archive is always present on modern Windows.
    await runCmd("powershell", ["-Command", `Compress-Archive -Force -Path '${binSrc}' -DestinationPath '${zipPath}'`]);
    console.log(`[kitron] wrote ${zipPath}`);
  } else {
    console.log(`[kitron] package ${platform}/${format}: unsupported — macos supports app|dmg|pkg, windows supports zip`);
    process.exitCode = 1;
  }
}

async function runCmd(cmd: string, args: string[]): Promise<void> {
  await new Promise<void>((resolve, reject) => {
    const p = spawn(cmd, args, { stdio: "inherit" });
    p.on("exit", (c) => (c === 0 ? resolve() : reject(new Error(`${cmd} exited ${c}`))));
    p.on("error", reject);
  });
}

/** codesign --deep --force --options runtime (hardened runtime for notarization). */
async function codesign(appDir: string, identity: string): Promise<void> {
  console.log(`[kitron] signing ${appDir} as ${identity}`);
  await runCmd("codesign", ["--deep", "--force", "--options", "runtime", "--sign", identity, appDir]);
  await runCmd("codesign", ["--verify", "--deep", "--strict", appDir]);
  console.log("[kitron] signature verified");
}

async function makeDmg(appDir: string, out: string, notarize: boolean): Promise<void> {
  try { await fs.unlink(out); } catch { /* fresh */ }
  await runCmd("hdiutil", ["create", "-volname", path.basename(appDir, ".app"), "-srcfolder", appDir, "-ov", "-format", "UDZO", out]);
  console.log(`[kitron] wrote ${out}`);
  if (notarize) await notarizeFile(out);
}

async function makePkg(appDir: string, out: string, notarize: boolean): Promise<void> {
  await runCmd("productbuild", ["--component", appDir, "/Applications", out]);
  console.log(`[kitron] wrote ${out}`);
  if (notarize) await notarizeFile(out);
}

/** notarytool submit (keychain profile) + staple. Requires Apple credentials. */
async function notarizeFile(file: string): Promise<void> {
  const profile = process.env["KITRON_NOTARY_PROFILE"] ?? "kitron-notary";
  console.log(`[kitron] submitting ${file} for notarization (profile ${profile})…`);
  await runCmd("xcrun", ["notarytool", "submit", file, "--keychain-profile", profile, "--wait"]);
  await runCmd("xcrun", ["stapler", "staple", file]);
  console.log("[kitron] notarized + stapled");
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
  if (process.platform === "darwin") {
    rows.push(["codesign", await exists("codesign")]);
    rows.push(["hdiutil", await exists("hdiutil")]);
    rows.push(["notarytool", await which("xcrun notarytool --version")]);
  }
  rows.push(["crash reports", await crashCount()]);
  for (const [k, v] of rows) console.log(`  ${k.padEnd(22)} ${v}`);
  console.log("\n miraculous: no Chromium found (as intended).");
}

async function crashCount(): Promise<string> {
  try {
    const dir = path.join(require_os_tmpdir(), "kitron-crashes");
    const entries = await fs.readdir(dir);
    const n = entries.filter((e) => e.endsWith(".json")).length;
    return n === 0 ? "none" : `${n} report(s) in ${dir} (see \`crash.list\`)`;
  } catch {
    return "none";
  }
}

function require_os_tmpdir(): string {
  // tmpdir without importing node:os at top level (keeps cli.ts lean).
  return process.env["TMPDIR"] ?? (process.platform === "win32" ? "C:\\Windows\\Temp" : "/tmp");
}

async function exists(cmd: string): Promise<string> {
  try {
    const { stdout } = await execFileAsync(process.platform === "win32" ? "where" : "command", process.platform === "win32" ? [cmd] : ["-v", cmd], { timeout: 10_000 });
    const p = String(stdout).trim().split("\n")[0];
    return p ? `present (${p})` : "MISSING";
  } catch {
    return "MISSING";
  }
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
