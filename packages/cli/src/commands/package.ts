import { runPackage } from "@kitron/runtime";
import { flag } from "../args.js";

export async function cmdPackage(_positional: string[], flags: Record<string, string | boolean>): Promise<void> {
  const target = flag(flags, "target", "t") as string | undefined;
  await runPackage({
    cwd: (flag(flags, "cwd") as string) ?? process.cwd(),
    target: (target as never) ?? undefined,
  });
}
