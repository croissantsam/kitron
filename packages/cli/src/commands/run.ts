import { existsSync } from "node:fs";
import { join } from "node:path";
import { defaultHostBinary } from "@kitron/runtime";
import { flag } from "../args.js";

/** `kitron run` — launch the built bundle layout (dev convenience). */
export async function cmdRun(_positional: string[], flags: Record<string, string | boolean>): Promise<void> {
  const cwd = ((flag(flags, "cwd") as string) ?? process.cwd()) as string;
  const bin = defaultHostBinary(cwd);
  if (!existsSync(bin)) {
    console.log("[Kitron] Native host not built. Run `cargo build -p kitron-host` first.");
    console.log(`[Kitron] (looked for ${join("native/rust/target/debug/kitron-host")})`);
    console.log(`[Kitron] manifest: ${join(cwd, "release/app/kitron-manifest.json")}`);
    return;
  }
  const { spawn } = await import("node:child_process");
  const child = spawn(bin, ["--url", `file://${join(cwd, "release/web/index.html")}`], { stdio: "inherit" });
  await new Promise((resolve) => child.on("exit", resolve));
}
