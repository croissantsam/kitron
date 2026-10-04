import { describe, expect, it } from "vitest";
import { createServer } from "node:http";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { AddressInfo } from "node:net";
import { crashReporter, safeStorage, updater, powerMonitor } from "@kitron/core";
import { BrowserWindow, configureTransport } from "@kitron/core";
import { createMockBackend } from "@kitron/runtime";
import { IPCPeer, IpcHub } from "@kitron/ipc";

describe("updater feed", () => {
  it("detects a newer version from a local feed", async () => {
    const server = createServer((req, res) => {
      res.setHeader("content-type", "application/json");
      res.end(JSON.stringify({ version: "9.9.9", notes: "big", url: "http://x/y" }));
      void req;
    });
    await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
    const port = (server.address() as AddressInfo).port;
    try {
      updater.setFeedURL(`http://127.0.0.1:${port}/feed.json`, "0.1.0");
      const feed = await updater.checkForUpdates();
      expect(feed.version).toBe("9.9.9");
      const info = await updater.check();
      expect(info.available).toBe(true);
    } finally {
      server.close();
    }
  });

  it("reports no update when current", async () => {
    const server = createServer((req, res) => {
      res.setHeader("content-type", "application/json");
      res.end(JSON.stringify({ version: "0.1.0" }));
      void req;
    });
    await new Promise<void>((r) => server.listen(0, "127.0.0.1", r));
    const port = (server.address() as AddressInfo).port;
    try {
      updater.setFeedURL(`http://127.0.0.1:${port}/feed.json`, "0.1.0");
      await expect(updater.checkForUpdates()).rejects.toThrowError(/no update/);
      await expect(updater.check()).resolves.toMatchObject({ available: false });
    } finally {
      server.close();
    }
  });
});

describe("crashReporter", () => {
  it("writes and reads back a report", () => {
    const dir = mkdtempSync(join(tmpdir(), "kitron-crash-"));
    const prev = process.env["KITRON_CRASH_DIR"];
    process.env["KITRON_CRASH_DIR"] = dir;
    try {
      crashReporter.start();
      const r = crashReporter.addReport("node-uncaught", "boom-test");
      expect(r.message).toBe("boom-test");
      const last = crashReporter.getLastCrashReport();
      expect(last?.id).toBe(r.id);
    } finally {
      if (prev === undefined) delete process.env["KITRON_CRASH_DIR"];
      else process.env["KITRON_CRASH_DIR"] = prev;
      rmSync(dir, { recursive: true, force: true });
    }
  });
});

describe("safeStorage", () => {
  it("round-trips a secret when a store exists", async () => {
    const available = await safeStorage.isEncryptionAvailable();
    if (!available) return; // no OS store in this environment — honest skip.
    const ct = await safeStorage.encryptString("s3cret-token");
    expect(Buffer.isBuffer(ct)).toBe(true);
    await expect(safeStorage.decryptString(ct)).resolves.toBe("s3cret-token");
  });
});

describe("power idle", () => {
  it("getSystemIdleTime resolves a number", async () => {
    const t = await powerMonitor.getSystemIdleTime();
    expect(typeof t).toBe("number");
  });

  it("getSystemIdleState maps idle time", async () => {
    const s = await powerMonitor.getSystemIdleState(1_000_000);
    expect(["active", "idle", "unknown"]).toContain(s);
  });
});

describe("hub routes webcontents to the host", () => {
  it("webcontents.* goes to host, shell.* to backend", async () => {
    const hub = new IpcHub();
    const hostPeer = new IPCPeer((f) => hub.routeFrom("host", f));
    hostPeer.handle("webcontents.getZoomFactor", () => 2);
    hub.addLink({ name: "host", deliver: (f) => hostPeer.receive(f) }, { host: true });
    const backendPeer = new IPCPeer((f) => hub.routeFrom("backend", f));
    backendPeer.handle("shell.beep", () => null);
    hub.addLink({ name: "backend", deliver: (f) => backendPeer.receive(f) }, { isDefault: true });
    const caller = new IPCPeer((f) => hub.routeFrom("caller", f));
    hub.addLink({ name: "caller", deliver: (f) => caller.receive(f) });
    await expect(caller.invoke("webcontents.getZoomFactor", { id: "w" })).resolves.toBe(2);
    await expect(caller.invoke("shell.beep")).resolves.toBeNull();
  });
});

describe("window fullscreen + capture over mock", () => {
  const { nodePeer } = createMockBackend();
  configureTransport(nodePeer);

  it("tracks fullscreen state", async () => {
    const win = new BrowserWindow({ title: "fs" });
    await win.ready;
    await win.setFullscreen(true);
    await expect(win.isFullscreen()).resolves.toBe(true);
  });

  it("capturePage fails typed on the mock", async () => {
    const win = new BrowserWindow({ title: "cap" });
    await win.ready;
    await expect(win.webContents.capturePage()).rejects.toMatchObject({
      code: "PLATFORM_UNSUPPORTED",
    });
  });

  it("printToPDF fails typed on the mock", async () => {
    const win = new BrowserWindow({ title: "pdf" });
    await win.ready;
    await expect(win.webContents.printToPDF()).rejects.toMatchObject({
      code: "PLATFORM_UNSUPPORTED",
    });
  });
});
