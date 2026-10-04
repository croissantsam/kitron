const { app, BrowserWindow } = require("electron");

const t0 = Number(process.env.KITRON_BENCH_T0 ?? Date.now());
console.log(JSON.stringify({ event: "BOOT", ms: Date.now() - t0 }));

async function main() {
  await app.whenReady();
  console.log(JSON.stringify({ event: "APP_READY", ms: Date.now() - t0 }));
  const win = new BrowserWindow({ width: 800, height: 600, show: false });
  console.log(JSON.stringify({ event: "WINDOW_CREATED", ms: Date.now() - t0 }));
  await win.loadFile("index.html");
  console.log(JSON.stringify({ event: "READY", ms: Date.now() - t0 }));
}

main().catch((e) => {
  console.error(JSON.stringify({ event: "ERROR", message: String(e && e.stack || e) }));
  process.exit(1);
});
