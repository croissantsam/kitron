import { describe, expect, it } from "vitest";
import { mkdtempSync, writeFileSync, existsSync, rmSync } from "node:fs";
import { tmpdir, homedir } from "node:os";
import { join } from "node:path";
import { shell } from "@kitron/core";
import { nativeTheme, powerMonitor } from "@kitron/core";
import { requestSingleInstanceLock } from "@kitron/core";
import { parseProtocolURLs, setAsDefaultProtocolClient } from "@kitron/core";
import { createMockBackend } from "@kitron/runtime";
import { BrowserWindow, configureTransport } from "@kitron/core";

describe("shell (Tranche A, Node-first)", () => {
  it("trashItem moves a file away", async () => {
    const dir = mkdtempSync(join(tmpdir(), "kitron-shell-"));
    const home = mkdtempSync(join(tmpdir(), "kitron-home-"));
    const prevHome = process.env["HOME"];
    process.env["HOME"] = home;
    try {
      const file = join(dir, "note.txt");
      writeFileSync(file, "hello");
      const ok = await shell.trashItem(file);
      expect(ok).toBe(true);
      expect(existsSync(file)).toBe(false);
    } finally {
      process.env["HOME"] = prevHome;
      rmSync(dir, { recursive: true, force: true });
      rmSync(home, { recursive: true, force: true });
    }
  });

  it("openPath returns an error string for missing handler target", async () => {
    const err = await shell.openPath("/nonexistent/kitron-target-xyz");
    expect(typeof err).toBe("string");
  });
});

describe("nativeTheme / powerMonitor", () => {
  it("reads theme info without crashing", async () => {
    const info = await nativeTheme.getInfo();
    expect(["system", "light", "dark"]).toContain(info.source);
    expect(typeof info.shouldUseDarkColors).toBe("boolean");
  });

  it("isOnBatteryPower resolves a boolean", async () => {
    await expect(powerMonitor.isOnBatteryPower()).resolves.toEqual(
      expect.any(Boolean),
    );
  });
});

describe("single instance", () => {
  it("first lock wins, second loses", () => {
    const appId = `test-${Date.now()}-${Math.floor(Math.random() * 1e6)}`;
    expect(requestSingleInstanceLock({ appId })).toBe(true);
    // Same process re-locking the same appId sees the live owner (itself).
    expect(requestSingleInstanceLock({ appId })).toBe(false);
  });
});

describe("protocols", () => {
  it("parses custom-scheme argv", () => {
    expect(parseProtocolURLs(["node", "app", "kitron://open?id=1"])).toEqual([
      "kitron://open?id=1",
    ]);
    expect(parseProtocolURLs(["node", "app", "https://x.io"])).toEqual([]);
  });

  it("registers a dev .desktop entry on linux (skipped elsewhere)", () => {
    if (process.platform !== "linux") return;
    expect(setAsDefaultProtocolClient("kitron-test")).toBe(true);
    expect(
      existsSync(join(homedir(), ".local/share/applications/kitron-test.desktop")),
    ).toBe(true);
  });
});

describe("webContents over mock host (Tranche B)", () => {
  const { nodePeer } = createMockBackend();
  configureTransport(nodePeer);

  it("evalJS fails typed without a JS engine", async () => {
    const win = new BrowserWindow({ title: "w" });
    await win.ready;
    await expect(win.webContents.executeJavaScript("1+1")).rejects.toMatchObject({
      code: "PLATFORM_UNSUPPORTED",
    });
  });

  it("getZoomFactor defaults to 1", async () => {
    const win = new BrowserWindow({ title: "w" });
    await win.ready;
    await expect(win.webContents.getZoomFactor()).resolves.toBe(1);
  });

  it("renderer allowlist covers the new channels", async () => {
    const { isChannelAllowed } = await import("@kitron/native-api");
    for (const ch of [
      "shell.openExternal",
      "theme.getInfo",
      "power.isOnBattery",
      "webcontents.evalJS",
      "app.requestSingleInstanceLock",
    ]) {
      expect(isChannelAllowed(ch)).toBe(true);
    }
  });
});
