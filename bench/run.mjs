/**
 * bench/run.mjs — Kitron vs Electron, measured on this machine.
 *
 * Metrics (hello-world apps, 800x600 window + one text):
 *   1. shipped bytes (release runtime / Electron framework + app)
 *   2. cold-start latency to interactive content (median of N runs)
 *   3. idle memory RSS (median of samples; Electron summed over processes)
 *
 * Usage: node bench/run.mjs [--runs N]
 */
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import fs from "node:fs/promises";
import path from "node:path";

const execFileAsync = promisify(execFile);
const RUNS = Number(process.argv.find((a) => a.startsWith("--runs="))?.split("=")[1] ?? 5);
const ROOT = path.resolve(new URL(".", import.meta.url).pathname, "..");
const KITRON_BIN = process.env.KITRON_BIN ?? path.join(ROOT, "runtime/rust/target/release/kitron-runtime");
const ELECTRON_DIR = path.join(ROOT, "bench/electron-app");

function median(xs) {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[s.length >> 1] : (s[(s.length >> 1) - 1] + s[s.length >> 1]) / 2;
}

async function runOnce(script, args) {
  const child = spawn("node", [script, ...args], { cwd: ROOT, stdio: ["ignore", "pipe", "pipe"] });
  const line = await new Promise((resolve, reject) => {
    let buf = "";
    child.stdout.on("data", (c) => {
      buf += c.toString();
      const idx = buf.indexOf("\n");
      if (idx >= 0) resolve(buf.slice(0, idx));
    });
    child.on("error", reject);
  });
  return { wrapper: child, result: JSON.parse(line) };
}

/** Sum RSS (KiB) of pid + all descendants via the ppid tree. */
async function treeRss(rootPid) {
  try {
    const { stdout } = await execFileAsync("ps", ["-ax", "-o", "pid=,ppid=,rss=,command="]);
    const kids = new Map();
    const rss = new Map();
    for (const line of stdout.split("\n")) {
      const m = line.trim().match(/^(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/);
      if (!m) continue;
      const [pid, ppid, mem] = [Number(m[1]), Number(m[2]), Number(m[3])];
      rss.set(pid, mem);
      if (!kids.has(ppid)) kids.set(ppid, []);
      kids.get(ppid).push(pid);
    }
    let total = 0;
    const stack = [rootPid];
    while (stack.length) {
      const cur = stack.pop();
      total += rss.get(cur) ?? 0;
      for (const k of kids.get(cur) ?? []) stack.push(k);
    }
    return total;
  } catch {
    return 0;
  }
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Median idle tree-RSS over ~3.5s, then kill app + wrapper. */
async function sampleIdle(wrapper, appPid) {
  await sleep(1500); // let it settle
  const samples = [];
  for (let i = 0; i < 3; i++) {
    samples.push(await treeRss(appPid));
    await sleep(700);
  }
  try {
    process.kill(appPid, "SIGKILL");
  } catch {
    // already gone
  }
  wrapper.kill("SIGKILL");
  return median(samples);
}

async function measureKitron() {
  try {
    await fs.stat(KITRON_BIN);
  } catch {
    throw new Error(`kitron binary missing: ${KITRON_BIN} (run cargo build --release first)`);
  }
  const starts = [];
  let mem = 0;
  for (let i = 0; i < RUNS; i++) {
    const { wrapper, result } = await runOnce("bench/measure-kitron.mjs", [KITRON_BIN]);
    if (result.error) throw new Error(`kitron: ${result.error}`);
    starts.push(result.startupReadyMs);
    if (i === 0) mem = await sampleIdle(wrapper, result.pid);
    else wrapper.kill("SIGKILL");
    await sleep(500);
  }
  return { startupMs: median(starts), allStarts: starts, idleRssKiB: mem };
}

async function measureElectron() {
  const starts = [];
  let mem = 0;
  let fallback = false;
  for (let i = 0; i < RUNS; i++) {
    const { wrapper, result } = await runOnce("bench/measure-electron.mjs", [ELECTRON_DIR]);
    if (result.error) throw new Error(`electron: ${result.error}`);
    starts.push(result.startupReadyMs);
    fallback = fallback || result.fallback;
    if (i === 0) mem = await sampleIdle(wrapper, result.pid);
    else wrapper.kill("SIGKILL");
    await sleep(800);
  }
  return { startupMs: median(starts), allStarts: starts, idleRssKiB: mem, fallback };
}

async function dirSize(p) {
  const { stdout } = await execFileAsync("du", ["-sk", p]);
  return Number(stdout.split("\t")[0]);
}

const [kitron, electron] = await Promise.all([
  measureKitron().catch((e) => ({ error: e.message })),
  // run sequentially: both open windows; avoid contention
  (async () => null)(),
]).then(async ([k]) => [k, await measureElectron().catch((e) => ({ error: e.message }))]);

const kitronBin = await dirSize(KITRON_BIN).catch(() => -1);
// Electron framework as shipped (the part every app re-bundles).
const electronApp = await dirSize(path.join(ELECTRON_DIR, "node_modules/electron/dist/Electron.app")).catch(() => -1);

const report = {
  machine: {
    os: `${process.platform} ${process.arch}`,
    cpus: (await import("node:os")).default.cpus()[0]?.model ?? "unknown",
    totalMemMiB: Math.round((await import("node:os")).default.totalmem() / 1024 / 1024),
  },
  runs: RUNS,
  kitron: kitron.error ? { error: kitron.error } : {
    startupReadyMsMedian: kitron.startupMs,
    startupAllMs: kitron.allStarts,
    idleRssMiB: +(kitron.idleRssKiB / 1024).toFixed(1),
    runtimeBinaryKiB: kitronBin,
  },
  electron: electron.error ? { error: electron.error } : {
    startupReadyMsMedian: electron.startupMs,
    startupAllMs: electron.allStarts,
    idleRssMiB: +(electron.idleRssKiB / 1024).toFixed(1),
    frameworkKiB: electronApp,
    paintFallback: electron.fallback,
  },
};

console.log(JSON.stringify(report, null, 2));
await fs.writeFile(path.join(ROOT, "bench/results.json"), JSON.stringify(report, null, 2) + "\n");
