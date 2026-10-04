import { runBuild } from "@kitron/runtime";
import { flag } from "../args.js";

export async function cmdBuild(_positional: string[], flags: Record<string, string | boolean>): Promise<void> {
  await runBuild({ cwd: (flag(flags, "cwd") as string) ?? process.cwd() });
}
