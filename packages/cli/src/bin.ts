#!/usr/bin/env node
import { parseArgs, flag } from "./index.js";
import { createApp, TEMPLATES, type TemplateName } from "./commands/create.js";
import { cmdDev } from "./commands/dev.js";
import { cmdBuild } from "./commands/build.js";
import { cmdPackage } from "./commands/package.js";
import { cmdRun } from "./commands/run.js";
import { cmdDoctor } from "./commands/doctor.js";

const VERSION = "0.1.0";

function help(): void {
  console.log(`kitron ${VERSION} — Desktop apps with TypeScript, Node.js and native WebViews.
Usage: kitron <command> [options]

Commands:
  create <dir> [--template react|vue|svelte|vanilla|tanstack-start]
  dev [--port <n>] [--devtools]
  build
  package [--target app|dmg|exe|msi|AppImage|deb|rpm]
  run
  doctor
`);
}

async function main(): Promise<void> {
  const { command, positional, flags } = parseArgs(process.argv.slice(2));
  if (flag(flags, "help", "h") || !command) {
    help();
    return;
  }
  if (flag(flags, "version", "V")) {
    console.log(VERSION);
    return;
  }
  switch (command) {
    case "create": {
      const dir = positional[0];
      if (!dir) {
        console.error("Usage: kitron create <dir> [--template <name>]");
        process.exitCode = 1;
        return;
      }
      const t = (flag(flags, "template", "t") as string) ?? "tanstack-start";
      if (!(TEMPLATES as string[]).includes(t)) {
        console.error(`Unknown template "${t}". Choose: ${TEMPLATES.join(", ")}`);
        process.exitCode = 1;
        return;
      }
      const name = dir.split("/").pop() || "my-app";
      await createApp(dir, t as TemplateName, name);
      console.log(`Created ${dir} (${t}). Next: cd ${dir} && pnpm install && pnpm dev`);
      break;
    }
    case "dev":
      await cmdDev(positional, flags);
      break;
    case "build":
      await cmdBuild(positional, flags);
      break;
    case "package":
      await cmdPackage(positional, flags);
      break;
    case "run":
      await cmdRun(positional, flags);
      break;
    case "doctor":
      await cmdDoctor();
      break;
    default:
      console.error(`Unknown command "${command}".`);
      help();
      process.exitCode = 1;
  }
}

main().catch((err) => {
  console.error(`[Kitron] ${(err as Error).message}`);
  process.exitCode = 1;
});
