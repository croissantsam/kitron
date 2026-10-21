/** `kitron doctor` — toolchain sanity checks (spec §37). */

import fs from "node:fs";
import path from "node:path";

import { log, POD_TEMPLATE_ROOT, RUST_WORKSPACE, run } from "./lib.ts";

interface Check {
  name: string;
  ok: boolean;
  detail: string;
}

function version(cmd: string, args: string[]): string | null {
  const result = run(cmd, args, { capture: true });
  return result.status === 0 ? result.stdout.trim().split("\n")[0] ?? null : null;
}

function nodeMajor(): number | null {
  const value = version("node", ["--version"]);
  return value ? Number(value.replace(/^v/, "").split(".")[0]) : null;
}

/** Rust std targets needed for the universal (arm64 + x86_64) staticlib. */
function rustTargets(): string[] {
  const result = run("rustup", ["target", "list", "--installed"], { capture: true });
  if (result.status !== 0) {
    return [];
  }
  return result.stdout
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

export async function doctor(): Promise<number> {
  const checks: Check[] = [];

  const node = nodeMajor();
  checks.push({
    name: "node",
    ok: node !== null && node >= 24,
    detail: version("node", ["--version"]) ?? "missing (Node 24+ required)",
  });

  const clang = version("clang++", ["--version"]);
  checks.push({ name: "clang++", ok: clang !== null, detail: clang ?? "missing" });

  const rustc = version("rustc", ["--version"]);
  const cargo = version("cargo", ["--version"]);
  checks.push({ name: "rustc", ok: rustc !== null, detail: rustc ?? "missing" });
  checks.push({ name: "cargo", ok: cargo !== null, detail: cargo ?? "missing" });

  const xcodebuild = version("xcodebuild", ["-version"]);
  checks.push({
    name: "xcodebuild",
    ok: xcodebuild !== null,
    detail: xcodebuild ?? "install Xcode with command line tools",
  });

  const lipo = run("xcrun", ["-find", "lipo"], { capture: true });
  checks.push({
    name: "lipo",
    ok: lipo.status === 0,
    detail: lipo.status === 0 ? lipo.stdout.trim() : "missing (needed for universal builds)",
  });

  const pod = version("pod", ["--version"]);
  checks.push({ name: "cocoapods", ok: pod !== null, detail: pod ?? "missing" });

  const pnpm = version("pnpm", ["--version"]);
  checks.push({ name: "pnpm", ok: pnpm !== null, detail: pnpm ?? "npm works too" });

  // Rust targets for the universal staticlib (Release builds are universal).
  const targets = rustTargets();
  const darwinTargets = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
  const missing = darwinTargets.filter((target) => !targets.includes(target));
  checks.push({
    name: "rust targets",
    ok: missing.length === 0,
    detail:
      missing.length === 0
        ? darwinTargets.join(", ")
        : `missing ${missing.join(", ")} — run: rustup target add ${missing.join(" ")}`,
  });

  // SDK integrity: runtime pieces the CLI copies into created apps.
  checks.push({
    name: "rust workspace",
    ok: fs.existsSync(path.join(RUST_WORKSPACE, "Cargo.toml")),
    detail: RUST_WORKSPACE,
  });
  checks.push({
    name: "kitron pod",
    ok: fs.existsSync(path.join(POD_TEMPLATE_ROOT, "Kitron.podspec")),
    detail: POD_TEMPLATE_ROOT,
  });
  checks.push({
    name: "node host",
    ok: fs.existsSync(path.join(RUST_WORKSPACE, "..", "node", "host.mjs")),
    detail: path.join(RUST_WORKSPACE, "..", "node", "host.mjs"),
  });

  let failed = 0;
  for (const check of checks) {
    const mark = check.ok ? "\x1b[32m✔\x1b[0m" : "\x1b[31m✖\x1b[0m";
    console.log(`${mark} ${check.name.padEnd(14)} ${check.detail}`);
    if (!check.ok) {
      failed += 1;
    }
  }

  if (failed > 0) {
    console.log(`\n${failed} check(s) failed — fix those before using kitron dev/build.`);
    return 1;
  }
  console.log("\nAll checks passed ✅");
  void log;
  return 0;
}
