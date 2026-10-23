/**
 * Measures one Kitron cold start: spawn the packaged Release app with a clean
 * environment and watch stdout/stderr for the runtime chain. Also captures a
 * FPS measurement logged by the app after ~4s.
 * Prints one JSON line, then keeps the child alive for memory sampling.
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
  env: { HOME: process.env.HOME, PATH: "/usr/bin:/bin:/usr/sbin:/sbin" },
});
let done = false;
let startupReadyMs = null;
let scrollMs = null;
const scan = (chunk) => {
  for (const line of String(chunk).split("\n")) {
    if (!line.trim()) continue;
    if (/app\.ready received/.test(line) && startupReadyMs === null) {
      startupReadyMs = Date.now() - t0;
    }
    const m = line.match(/SCROLL_RESULT\D*(\d+)/);
    if (m && scrollMs === null) {
      scrollMs = Number(m[1]);
    }
    if (startupReadyMs !== null && scrollMs !== null && !done) {
      done = true;
      console.log(JSON.stringify({ pid: child.pid, startupReadyMs, scrollMs }));
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
    if (startupReadyMs !== null) {
      done = true;
      console.log(JSON.stringify({ pid: child.pid, startupReadyMs, scrollMs }));
    } else {
      console.log(JSON.stringify({ error: "timeout waiting for app.ready", elapsedMs: Date.now() - t0 }));
      process.exit(1);
    }
  }
}, 20000);
setInterval(() => {}, 1000);
