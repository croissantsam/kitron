import { createInMemoryPair } from "../packages/ipc/dist/index.js";

/** Reproducible IPC benchmarks (§38–39). Same app, same machine, same scenario. */
async function main() {
  const [a, b] = createInMemoryPair();
  b.handle("echo", (p) => p);

  const N = 2000;
  let t0 = performance.now();
  for (let i = 0; i < N; i++) await a.invoke("echo", { i });
  const dt = performance.now() - t0;
  console.log(`ipc latency: ${(dt / N).toFixed(3)} ms/op (${N} ops in ${dt.toFixed(0)} ms)`);
}

main();
