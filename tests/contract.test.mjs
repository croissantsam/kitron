import { describe, it } from "node:test";
import assert from "node:assert/strict";

// Contract: TS IPC envelope <-> Rust kitron-core wire format.
// Mirrors runtime/rust/kitron-core/src/lib.rs tests.

describe("ipc envelope contract", () => {
  it("KitronError carries code/message/platform", async () => {
    const { KitronError } = await import("@kitron/core");
    const e = new KitronError({ code: "WINDOW_CREATION_FAILED", message: "nope" });
    assert.equal(e.code, "WINDOW_CREATION_FAILED");
    assert.ok(["macos", "windows", "linux", "unknown"].includes(e.platform));
  });

  it("window.create vertical slice via in-memory transport", async () => {
    const core = await import("@kitron/core");
    const t = new core.InMemoryTransport();
    t.on("window", "create", async (_m, params) => ({ id: "win-1", echo: params }));
    core.setBridgeTransport(t);
    const { window } = await import("@kitron/window");
    const w = await window.create({ width: 1200, height: 800, title: "My App" });
    assert.equal(w.id, "win-1");
    core.setBridgeTransport(null);
  });

  it("window.list returns all windows", async () => {
    const core = await import("@kitron/core");
    const t = new core.InMemoryTransport();
    t.on("window", "list", async () => [{ id: "a" }, { id: "b" }]);
    core.setBridgeTransport(t);
    const { window } = await import("@kitron/window");
    assert.deepEqual(await window.list(), [{ id: "a" }, { id: "b" }]);
    core.setBridgeTransport(null);
  });

  it("bridge without transport throws KITRON_RUNTIME_UNAVAILABLE", async () => {
    const core = await import("@kitron/core");
    core.setBridgeTransport(null);
    await assert.rejects(() => core.invoke("window", "create", {}), (e) => e.code === "KITRON_RUNTIME_UNAVAILABLE");
  });

  it("backend.invoke envelope", async () => {
    const core = await import("@kitron/core");
    const t = new core.InMemoryTransport();
    t.on("backend", "invoke", async (_m, params) => ({ ok: true, params }));
    core.setBridgeTransport(t);
    const { backend } = await import("@kitron/node");
    const r = await backend.invoke("users.get", { id: 123 });
    assert.deepEqual(r.params, { procedure: "users.get", params: { id: 123 } });
    core.setBridgeTransport(null);
  });

  it("dialog cancel maps to null", async () => {
    const core = await import("@kitron/core");
    const t = new core.InMemoryTransport();
    t.on("dialog", "openFile", async () => {
      throw new core.KitronError({ code: "DIALOG_CANCELLED", message: "cancelled" });
    });
    core.setBridgeTransport(t);
    const { dialog } = await import("@kitron/dialog");
    assert.equal(await dialog.openFile({ multiple: true }), null);
    core.setBridgeTransport(null);
  });

  it("tray create routes click events to the right instance", async () => {
    const core = await import("@kitron/core");
    const t = new core.InMemoryTransport();
    t.on("tray", "create", async () => ({ id: "tray-1" }));
    core.setBridgeTransport(t);
    const { Tray } = await import("@kitron/tray");
    const tray = await Tray.create({ icon: "./icon.png" });
    let clicked = 0;
    tray.onClick(() => clicked++);
    t.emit("tray.clicked", { id: "tray-1" });
    t.emit("tray.clicked", { id: "other" });
    assert.equal(clicked, 1);
    core.setBridgeTransport(null);
  });

  it("menu.onAction receives native actions", async () => {
    const core = await import("@kitron/core");
    const t = new core.InMemoryTransport();
    t.on("menu", "create", async () => ({ id: "menu-1" }));
    core.setBridgeTransport(t);
    const { menu } = await import("@kitron/menu");
    const seen = [];
    menu.onAction((e) => seen.push(e));
    const { id } = await menu.create([{ label: "File", items: [{ label: "New", action: "new-file" }] }]);
    assert.equal(id, "menu-1");
    t.emit("menu.action", { menuId: "menu-1", action: "new-file" });
    assert.deepEqual(seen, [{ menuId: "menu-1", action: "new-file" }]);
    core.setBridgeTransport(null);
  });

  it("shortcut duplicate registration surfaces typed error", async () => {
    const core = await import("@kitron/core");
    const t = new core.InMemoryTransport();
    t.on("shortcut", "register", async () => {
      throw new core.KitronError({ code: "SHORTCUT_REGISTER_FAILED", message: "already registered" });
    });
    core.setBridgeTransport(t);
    const { shortcut } = await import("@kitron/shortcut");
    await assert.rejects(() => shortcut.register("Ctrl+K"), (e) => e.code === "SHORTCUT_REGISTER_FAILED");
    core.setBridgeTransport(null);
  });

  it("updater.verify rejects bad signatures with typed error", async () => {
    const core = await import("@kitron/core");
    const t = new core.InMemoryTransport();
    t.on("updater", "verify", async () => {
      throw new core.KitronError({ code: "UPDATE_SIGNATURE_INVALID", message: "bad sig" });
    });
    core.setBridgeTransport(t);
    const { updater } = await import("@kitron/updater");
    await assert.rejects(
      () => updater.verifySignature("payload", "sig", "key"),
      (e) => e.code === "UPDATE_SIGNATURE_INVALID",
    );
    core.setBridgeTransport(null);
  });

  it("app deepLink/fileOpen emit through native events", async () => {
    const core = await import("@kitron/core");
    const t = new core.InMemoryTransport();
    t.on("app", "deepLink", async (_m, params) => {
      t.emit("app.deepLink", params);
      return null;
    });
    core.setBridgeTransport(t);
    const { app } = await import("@kitron/core");
    let got;
    app.on("deepLink", (p) => (got = p));
    await app.emitDeepLink("myapp://open/1");
    assert.deepEqual(got, { url: "myapp://open/1" });
    core.setBridgeTransport(null);
  });
});
