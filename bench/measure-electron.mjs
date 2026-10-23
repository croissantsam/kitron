import { spawn } from "node:child_process";
import path from "node:path";

const appDir = process.argv[2] ?? "./bench/electron-app";
const electronBin = path.resolve(appDir, "node_modules/.bin/electron");
const t0 = Date.now();

const child = spawn(electronBin, ["--no-sandbox", "."], {
  cwd: path.resolve(appDir),
  stdio: ["ignore", "pipe", "pipe"],
  env: { ...process.env, KITRON_BENCH_T0: String(t0), ELECTRON_DISABLE_SANDBOX: "1" },
});

let done = false;
let startupReadyMs = null;
let scrollMs = null;
child.stdout.on("data", (chunk) => {
  for (const line of String(chunk).split("\n")) {
    if (!line.trim()) continue;
    try {
      const msg = JSON.parse(line);
      if ((msg.event === "READY" || msg.event === "READY_FALLBACK") && startupReadyMs === null) {
        startupReadyMs = Date.now() - t0;
      }
      if (msg.event === "SCROLL" && scrollMs === null) {
        scrollMs = msg.ms;
      }
      if (startupReadyMs !== null && scrollMs !== null && !done) {
        done = true;
        console.log(JSON.stringify({
          pid: child.pid,
          startupReadyMs,
          scrollMs,
          fallback: msg.event === "READY_FALLBACK",
        }));
      }
    } catch {
      // non-JSON logs ignored
    }
  }
});
child.stderr.on("data", () => {});
child.on("error", (e) => {
  console.log(JSON.stringify({ error: String(e) }));
  process.exit(1);
});
setTimeout(() => {
  if (!done) {
    if (startupReadyMs !== null) {
      done = true;
      console.log(JSON.stringify({ pid: child.pid, startupReadyMs, scrollMs, fallback: false }));
    } else {
      console.log(JSON.stringify({ error: "timeout waiting for READY", elapsedMs: Date.now() - t0 }));
      process.exit(1);
    }
  }
}, 20000);

setInterval(() => {}, 1000);
