const { app, BrowserWindow } = require("electron");

const t0 = Number(process.env.KITRON_BENCH_T0 ?? Date.now());
console.log(JSON.stringify({ event: "BOOT", ms: Date.now() - t0 }));

async function main() {
  await app.whenReady();
  console.log(JSON.stringify({ event: "APP_READY", ms: Date.now() - t0 }));
  const win = new BrowserWindow({ width: 800, height: 600, show: false, webPreferences: { nodeIntegration: true, contextIsolation: false } });
  console.log(JSON.stringify({ event: "WINDOW_CREATED", ms: Date.now() - t0 }));
  win.webContents.on("console-message", (e) => {
    const message = (e && (e.message || e.toString())) || "";
    // Forward renderer console output (incl. our SCROLL JSON) to stdout.
    if (message.trim()) process.stdout.write(message + "\n");
  });
  await win.loadFile("index.html");
  console.log(JSON.stringify({ event: "READY", ms: Date.now() - t0 }));
}

main().catch((e) => {
  console.error(JSON.stringify({ event: "ERROR", message: String(e && e.stack || e) }));
  process.exit(1);
});
