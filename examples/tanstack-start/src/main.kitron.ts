import { app, BrowserWindow, native } from "@kitron/core";

await app.ready();

const win = new BrowserWindow({
  width: 1200,
  height: 800,
  title: "Kitron Vanilla",
});

await win.loadURL(process.env["KITRON_DEV_URL"] ?? "http://127.0.0.1:5173");

await native.notification.show({
  title: "Kitron",
  body: "Application started",
});
