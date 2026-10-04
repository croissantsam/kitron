import { describe, expect, it } from "vitest";
import { KitronError } from "@kitron/types";
import { createMockBackend } from "@kitron/runtime";
import {
  BrowserWindow,
  app,
  configureTransport,
  native,
  setPermissionChecker,
} from "@kitron/core";
import { createPermissionChecker } from "@kitron/runtime";

describe("core API over mock host", () => {
  const { nodePeer } = createMockBackend({ appName: "Test App", appVersion: "9.9.9" });
  configureTransport(nodePeer);
  setPermissionChecker(createPermissionChecker({ notifications: true }));

  it("ping → pong (§50)", async () => {
    await expect(nodePeer.invoke("ping")).resolves.toBe("pong");
  });

  it("app API", async () => {
    await expect(app.getName()).resolves.toBe("Test App");
    await expect(app.getVersion()).resolves.toBe("9.9.9");
    await expect(app.isReady()).resolves.toBe(true);
    await expect(app.getPath("documents")).resolves.toContain("Documents");
  });

  it("BrowserWindow lifecycle", async () => {
    const win = new BrowserWindow({ width: 800, height: 600, title: "T" });
    await win.ready;
    await win.loadURL("http://127.0.0.1:5173");
    await win.setTitle("T2");
    await win.show();
    await win.hide();
    await expect(win.isMaximized()).resolves.toBe(false);
    await win.close();
  });

  it("native notification", async () => {
    await expect(
      native.notification.show({ title: "Kitron", body: "Hello" }),
    ).resolves.toBeNull();
  });

  it("unknown window fails typed", async () => {
    const win = new BrowserWindow({ title: "ghost" });
    await win.ready;
    // Corrupt id path: direct invoke on missing window
    await expect(nodePeer.invoke("window.show", { id: "missing" })).rejects.toMatchObject({
      code: "WINDOW_FAILED",
    });
  });
});

describe("permissions (§21)", () => {
  it("denies disabled capabilities", async () => {
    const { nodePeer } = createMockBackend();
    configureTransport(nodePeer);
    setPermissionChecker(createPermissionChecker({ notifications: false }));
    await expect(
      native.notification.show({ title: "x", body: "y" }),
    ).rejects.toMatchObject({ code: "PERMISSION_DENIED" });
    expect(() => {
      throw new KitronError("PERMISSION_DENIED", "denied");
    }).toThrowError(KitronError);
  });
});
