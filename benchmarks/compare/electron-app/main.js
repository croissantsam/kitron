const { app, BrowserWindow, ipcMain } = require("electron");
const path = require("node:path");

ipcMain.handle("ping", () => "pong");

async function main() {
  await app.whenReady();
  console.log("READY");
  const win = new BrowserWindow({
    width: 1200,
    height: 800,
    title: "Bench",
    webPreferences: { preload: path.join(__dirname, "preload.js") },
  });
  win.webContents.on("did-finish-load", () => console.log("LOADED"));
  await win.loadFile(path.join(__dirname, "index.html"));
}

main();
