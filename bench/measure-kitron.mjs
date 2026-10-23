/**
 * Measures one Kitron cold start: spawn the packaged Release app with a clean
 * environment (no Metro, no KITRON_* vars — exactly what a user launches) and
 * watch stdout for the runtime chain. READY = "app.ready received", which the
 * app logs once React has mounted its UI (the same marker the macOS e2e
 * asserts). Prints one JSON line, then keeps the child alive for memory
 * sampling (the parent kills it).
 *
 * Usage: node bench/measure-kitron.mjs <app-path>
 */
import { spawn } from "node:child_process";
import fs from "node:fs";
import path from "node:path";

const appPath = process.argv[2];
if (!appPath || !fs.existsSync(appPath)) {
  console.log(
    JSON.stringify({
      error: `no .app at ${appPath} — run kitron build first (bench/run.mjs --prepare does it)`,
    }),
  );
  process.exit(1);
}
const productName = path.basename(appPath).replace(/\.app$/, "");
const bin = path.join(appPath, "Contents", "MacOS", productName);
if (!fs.existsSync(bin)) {
  console.log(
    JSON.stringify({ error: `${bin} missing — the build produced the bundle but not the executable` }),
  );
  process.exit(1);
}
const t0 = Date.now();

const child = spawn(bin, [], {
  stdio: ["ignore", "pipe", "pipe"],
  cwd: "/tmp",
  // Standalone launch: embedded Node, bundled JS, no SDK helpers reachable.
  env: { HOME: process.env.HOME, PATH: "/usr/bin:/bin:/usr/sbin:/sbin" },
});
let done = false;
// The packaged app logs everything through NSLog → stderr (0 bytes on
// stdout — verified). Both pipes feed the same scanner.
const scan = (chunk) => {
  for (const line of String(chunk).split("\n")) {
    if (!line.trim()) continue;
    if (/app\.ready received/.test(line) && !done) {
      done = true;
      console.log(JSON.stringify({ pid: child.pid, startupReadyMs: Date.now() - t0 }));
    }
  }
};
child.stdout.on("data", scan);
child.stderr.on("data", scan);
child.on("error", (e) => {
  console.log(JSON.stringify({ error: String(e) }));
  process.exit(1);
});
setTimeout(() => {
  if (!done) {
    console.log(JSON.stringify({ error: "timeout waiting for app.ready", elapsedMs: Date.now() - t0 }));
    process.exit(1);
  }
}, 60000);
setInterval(() => {}, 1000);
