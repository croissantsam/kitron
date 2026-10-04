import { runDev } from "@kitron/runtime";
import { flag } from "../args.js";

export async function cmdDev(positional: string[], flags: Record<string, string | boolean>): Promise<void> {
  const portRaw = flag(flags, "port", "p");
  await runDev({
    cwd: (flag(flags, "cwd") as string) ?? process.cwd(),
    port: portRaw == null ? undefined : Number(portRaw),
    openDevtools: Boolean(flag(flags, "devtools") ?? false),
  });
  // Keep alive; runDev installs SIGINT handling.
  await new Promise(() => undefined);
  void positional;
}
