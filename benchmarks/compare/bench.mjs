// Kitron vs Electron benchmark — same app, same machine, same scenario.
// Measures: time-to-loaded, idle RSS of the spawned process trees,
// native shell size, renderer IPC round-trip. Prints a markdown table.
// Run: node benchmarks/compare/bench.mjs  (from the repo root)
import { spawn, execFileSync } from "node:child_process";
import { setTimeout as sleep } from "node:timers/promises";
import { performance } from "node:perf_hooks";
import { existsSync, statSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const electronApp = join(root, "benchmarks", "compare", "electron-app");
const electronBin =
  process.platform === "darwin"
    ? join(electronApp, "node_modules", "electron", "dist", "Electron.app", "Contents", "MacOS", "Electron")
    : "electron";
const hostBin = join(root, "native", "rust", "target", "debug", "kitron-host");
const kitronPage = join(root, "benchmarks", "compare", "kitron-page.html");

function treeProcs() {
  // pid -> { rssKB, ppid }
  const out = {};
  try {
    const txt = execFileSync("ps", ["-A", "-o", "pid=,rss=,ppid=,comm="], { encoding: "utf8" });
    for (const line of txt.split("\n")) {
      const m = line.trim().match(/^(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/);
      if (m) out[Number(m[1])] = { rssKB: Number(m[2]), ppid: Number(m[3]), comm: m[4] };
    }
  } catch { /* ignore */ }
  return out;
}

function treeRSS(rootPid) {
  const procs = treeProcs();
  const kids = new Map();
  for (const [pid, p] of Object.entries(procs)) {
    if (!kids.has(p.ppid)) kids.set(p.ppid, []);
    kids.get(p.ppid).push(Number(pid));
  }
  let total = 0;
  const stack = [rootPid];
  const seen = new Set();
  while (stack.length > 0) {
    const pid = stack.pop();
    if (seen.has(pid) || !procs[pid]) continue;
    seen.add(pid);
    total += procs[pid].rssKB;
    for (const k of kids.get(pid) ?? []) stack.push(k);
  }
  return { totalKB: total, n: seen.size };
}

function dirSizeKB(path) {
  try {
    const out = execFileSync("du", ["-sk", path], { encoding: "utf8" });
    return Number(out.split("\t")[0]);
  } catch { return 0; }
}

async function benchElectron() {
  const { createRequire } = await import("node:module");
  const requireHere = createRequire(join(electronApp, "package.json"));
  const { _electron } = requireHere("playwright-core");
  const t0 = Date.now();
  const eapp = await _electron.launch({ executablePath: electronBin, args: [electronApp] });
  const window = await eapp.firstWindow();
  await window.waitForLoadState("load");
  const tLoaded = Date.now() - t0;
  await sleep(3000);
  const pid = eapp.process().pid;
  const mem = treeRSS(pid);
  // Renderer IPC round-trip, 200 sequential invokes.
  const N = 200;
  const t1 = performance.now();
  for (let i = 0; i < N; i++) {
    await window.evaluate(() => window.bench.invoke("ping"));
  }
  const ipcMs = (performance.now() - t1) / N;
  await eapp.close();
  return { tLoaded, mem, ipcMs };
}

async function benchKitron() {
  if (!existsSync(hostBin)) throw new Error("kitron-host not built (cargo build --features native-webview)");
  const t0 = Date.now();
  const child = spawn(hostBin, ["--url", `file://${kitronPage}?n=200`, "--title", "Bench"], {
    stdio: ["pipe", "pipe", "ignore"],
  });
  let buf = "";
  const pingTimes = [];
  let createdResolve;
  const created = new Promise((r) => { createdResolve = r; });
  let loadedAt = 0;
  child.stdout.on("data", (d) => {
    buf += String(d);
    let idx;
    while ((idx = buf.indexOf("\n")) >= 0) {
      const line = buf.slice(0, idx).trim();
      buf = buf.slice(idx + 1);
      if (!line) continue;
      let frame;
      try { frame = JSON.parse(line); } catch { continue; }
      if (frame.channel === "ping" && frame.id) {
        if (!loadedAt) {
          loadedAt = Date.now() - t0;
          createdResolve();
        }
        pingTimes.push(performance.now());
        child.stdin.write(JSON.stringify({ id: frame.id, success: true, payload: "pong" }) + "\n");
      } else if (frame.id && frame.channel) {
        // Answer any other renderer forward generically.
        child.stdin.write(JSON.stringify({ id: frame.id, success: true, payload: null }) + "\n");
      }
      // (Binary's own responses carry no channel — ignored.)
    }
  });
  await Promise.race([created, sleep(20000)]);
  const tLoaded = loadedAt || 20000;
  await sleep(3000);
  const mem = treeRSS(child.pid);
  // Wait until 201 pings observed (1 display + 200 bench), then compute.
  const deadline = Date.now() + 30000;
  while (pingTimes.length < 201 && Date.now() < deadline) await sleep(200);
  let ipcMs = 0;
  if (pingTimes.length >= 2) {
    const deltas = [];
    for (let i = 1; i < pingTimes.length; i++) deltas.push(pingTimes[i] - pingTimes[i - 1]);
    deltas.sort((a, b) => a - b);
    ipcMs = deltas[Math.floor(deltas.length / 2)];
  }
  child.kill();
  return { tLoaded, mem, ipcMs, pings: pingTimes.length };
}

const fmtMB = (kb) => `${(kb / 1024).toFixed(0)} MB`;
const electronDist = join(electronApp, "node_modules", "electron", "dist");

console.log("Warming up + measuring (2 iterations per side)…");
const eRuns = [];
for (let i = 0; i < 3; i++) eRuns.push(await benchElectron());
const kRuns = [];
for (let i = 0; i < 3; i++) kRuns.push(await benchKitron());

const mean = (a) => a.reduce((x, y) => x + y, 0) / a.length;
const e = {
  t: mean(eRuns.map((r) => r.tLoaded)),
  rss: mean(eRuns.map((r) => r.mem.totalKB)),
  ipc: mean(eRuns.map((r) => r.ipcMs)),
};
const k = {
  t: mean(kRuns.map((r) => r.tLoaded)),
  rss: mean(kRuns.map((r) => r.mem.totalKB)),
  ipc: mean(kRuns.map((r) => r.ipcMs)),
};
const eSize = dirSizeKB(electronDist);
const kSize = statSync(hostBin).size / 1024;

console.log("\n## Kitron vs Electron (measured, same machine)");
console.log(`\nMachine: ${process.platform}-${process.arch}, Electron ${(() => { try { return execFileSync(electronBin, ["--version"], { encoding: "utf8" }).trim(); } catch { return "?"; } })()}, Kitron host (wry 0.6/WKWebView). Same page content (h1 + ping), local file://, 1200x800, idle 3s before RSS, IPC = mean of 200 sequential renderer invokes.`);
console.log("\n| Metric | Electron | Kitron |");
console.log("|---|---|---|");
console.log(`| Time to loaded | ${e.t.toFixed(0)} ms | ${k.t.toFixed(0)} ms |`);
console.log(`| Idle RSS (spawned tree) | ${fmtMB(e.rss)} (${mean(eRuns.map((r) => r.mem.n)).toFixed(0)} procs) | ${fmtMB(k.rss)} (${mean(kRuns.map((r) => r.mem.n)).toFixed(0)} procs) |`);
console.log(`| Native shell size | ${fmtMB(eSize)} (Chromium bundled) | ${fmtMB(kSize)} (system WebView, no Chromium) |`);
console.log(`| Renderer IPC round-trip | ${e.ipc.toFixed(3)} ms | ${k.ipc.toFixed(3)} ms |`);
console.log("\nCaveats: Kitron RSS excludes shared OS WebKit processes; Electron includes its own helpers. Neither side includes a Node backend here; Kitron prod will embed Node (~90 MB), Electron already bundles it above. Re-run: `node benchmarks/compare/bench.mjs`.");
