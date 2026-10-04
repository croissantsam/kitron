#!/usr/bin/env node
import { createRequire } from "node:module";
import { createApp, TEMPLATES, type TemplateName } from "@kitron/cli";

/**
 * `npx create-kitron my-app [--template react]` (also `npm init kitron`).
 * Same scaffolder as `kitron create`, but emits publish-ready npm ranges
 * for @kitron/* instead of `workspace:*` links.
 */
function usage(): void {
  console.log(`create-kitron  — scaffold a Kitron desktop app.
Usage: npx create-kitron <dir> [--template <name>]

Templates: ${TEMPLATES.join(", ")} (default: tanstack-start)

Also works as: npm init kitron <dir> [--template <name>]`);
}

async function main(): Promise<void> {
  const argv = process.argv.slice(2);
  const flags: Record<string, string> = {};
  const positional: string[] = [];
  for (let i = 0; i < argv.length; i++) {
    const tok = argv[i]!;
    if (tok.startsWith("--")) {
      const eq = tok.indexOf("=");
      if (eq >= 0) flags[tok.slice(2, eq)] = tok.slice(eq + 1);
      else if (i + 1 < argv.length && !argv[i + 1]!.startsWith("-")) {
        flags[tok.slice(2)] = argv[++i]!;
      } else flags[tok.slice(2)] = "true";
    } else if (!tok.startsWith("-")) {
      positional.push(tok);
    }
  }
  if (flags["help"] || flags["h"] || positional.length === 0) {
    usage();
    if (positional.length === 0) process.exitCode = 1;
    return;
  }
  const dir = positional[0]!;
  const t = flags["template"] ?? flags["t"] ?? "tanstack-start";
  if (!(TEMPLATES as string[]).includes(t)) {
    console.error(`Unknown template "${t}". Choose: ${TEMPLATES.join(", ")}`);
    process.exitCode = 1;
    return;
  }
  // Pin @kitron/* to this release line (pnpm rewrites workspace:* on publish).
  let kitronTag = "^0.1.0";
  try {
    const require = createRequire(import.meta.url);
    const cliPkg = require("@kitron/cli/package.json") as { version?: string };
    if (cliPkg.version) kitronTag = `^${cliPkg.version}`;
  } catch {
    // fall back to the default tag.
  }
  const name = dir.split("/").pop() || "my-app";
  await createApp(dir, t as TemplateName, name, { kitronTag });
  console.log(`Created ${dir} (${t}).`);
  console.log(`Next:\n  cd ${dir}\n  npm install   # or pnpm install / yarn install\n  npm run dev`);
}

main().catch((err) => {
  console.error(`[create-kitron] ${(err as Error).message}`);
  process.exitCode = 1;
});
