/**
 * Measures one Kitron cold start: spawn headless runtime → window.create →
 * render a hello tree → layout. Prints one JSON line, then exits (leaving
 * the child for the parent to sample + kill).
 *
 * Usage: node bench/measure-kitron.mjs <runtime-bin>
 */
import { spawn } from "node:child_process";

const bin = process.argv[2] ?? "./runtime/rust/target/debug/kitron-runtime";
const t0 = Date.now();

const child = spawn(bin, ["run", "--headless"], { stdio: ["pipe", "pipe", "inherit"] });
child.on("error", (e) => {
  console.log(JSON.stringify({ error: String(e) }));
  process.exit(1);
});

let nextId = 1;
const pending = new Map();
let buf = "";
child.stdout.on("data", (chunk) => {
  buf += chunk.toString();
  let idx;
  while ((idx = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, idx).trim();
    buf = buf.slice(idx + 1);
    if (!line) continue;
    try {
      const msg = JSON.parse(line);
      if (typeof msg.id === "number" && pending.has(msg.id)) {
        const { resolve } = pending.get(msg.id);
        pending.delete(msg.id);
        resolve(msg);
      }
    } catch {
      // event lines ignored
    }
  }
});

function rpc(domain, method, params) {
  const id = nextId++;
  return new Promise((resolve) => {
    pending.set(id, { resolve });
    child.stdin.write(JSON.stringify({ id, domain, method, params }) + "\n");
  });
}

const created = await rpc("window", "create", { width: 800, height: 600, title: "bench" });
const tCreated = Date.now() - t0;
const winId = created.result.id;

await rpc("ui", "create", { id: "b-root", kind: "view", props: {}, style: { flex: 1 } });
await rpc("ui", "create", { id: "b-text", kind: "text", props: { text: "Hello Kitron" }, style: { fontSize: 28 } });
await rpc("ui", "append", { parent: "b-root", child: "b-text" });
await rpc("ui", "setRoot", { window: winId, root: "b-root" });
const laid = await rpc("ui", "layout", { window: winId, width: 800, height: 600 });
const tReady = Date.now() - t0;

console.log(JSON.stringify({
  pid: child.pid,
  startupCreateMs: tCreated,
  startupReadyMs: tReady,
  layoutNodes: Array.isArray(laid.result) ? laid.result.length : -1,
}));

// Keep the child alive for memory sampling; parent kills it.
setInterval(() => {}, 1000);
