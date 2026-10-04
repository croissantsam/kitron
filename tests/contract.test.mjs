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

  it("PERMISSION_DENIED rehydrates as KitronError", async () => {
    const { KitronError } = await import("@kitron/core");
    const e = KitronError.fromJSON({ code: "PERMISSION_DENIED", message: "shell requires a grant", platform: "macos" });
    assert.ok(e instanceof KitronError);
    assert.equal(e.code, "PERMISSION_DENIED");
  });

  it("crash list/get/clear roundtrip", async () => {
    const core = await import("@kitron/core");
    const t = new core.InMemoryTransport();
    let store = [{ id: "c1", time: "t", app_version: "0.1.0", platform: "macos", message: "boom" }];
    t.on("crash", "list", async () => store);
    t.on("crash", "get", async (_m, params) => {
      const found = store.find((r) => r.id === params.id);
      if (!found) throw new core.KitronError({ code: "CRASH_NOT_FOUND", message: "nope" });
      return found;
    });
    t.on("crash", "clear", async () => ({ cleared: store.splice(0).length }));
    core.setBridgeTransport(t);
    const { crash } = await import("@kitron/crash");
    assert.equal((await crash.list()).length, 1);
    assert.equal((await crash.get("c1")).message, "boom");
    await assert.rejects(() => crash.get("nope"), (e) => e.code === "CRASH_NOT_FOUND");
    assert.deepEqual(await crash.clear(), { cleared: 1 });
    core.setBridgeTransport(null);
  });

  it("updater staged flow surfaces typed errors", async () => {    const core = await import("@kitron/core");
    const t = new core.InMemoryTransport();
    t.on("updater", "download", async () => ({ path: "/tmp/u.bin", version: "1.2.0" }));
    t.on("updater", "install", async () => {
      throw new core.KitronError({ code: "UPDATE_NOT_STAGED", message: "nothing downloaded" });
    });
    core.setBridgeTransport(t);
    const { updater } = await import("@kitron/updater");
    assert.deepEqual(await updater.downloadUpdate("https://x.com/u.bin", "1.2.0"), { path: "/tmp/u.bin", version: "1.2.0" });
    await assert.rejects(
      () => updater.installUpdate({ signature: "s", publicKey: "k" }),
      (e) => e.code === "UPDATE_NOT_STAGED",
    );
    core.setBridgeTransport(null);
  });

  it("ui reconciler creates, patches, and routes press", async () => {
    const core = await import("@kitron/core");
    const ops = [];
    const t = new core.InMemoryTransport();
    for (const m of ["create", "append", "setProps", "setStyle", "remove", "setRoot", "clearRoot"]) {
      t.on("ui", m, async (_m, params) => { ops.push([m, params]); return m === "create" ? null : null; });
    }
    core.setBridgeTransport(t);
    const { View, Text, Button, render, unmount, __resetUiForTests } = await import("@kitron/ui");
    __resetUiForTests();
    let pressed = 0;
    const tree = () => View({ style: { flex: 1 } },
      Text({}, "hi"),
      Button({ title: "Go", onPress: () => pressed++ }),
    );
    await render("win1", tree());
    assert.ok(ops.some(([m]) => m === "create"));
    assert.ok(ops.some(([m, p]) => m === "setRoot" && p.root));
    const creates = ops.filter(([m]) => m === "create").length;
    // Re-render same shape: no new creates, only patches.
    ops.length = 0;
    await render("win1", tree());
    assert.equal(ops.filter(([m]) => m === "create").length, 0);
    assert.ok(ops.some(([m]) => m === "setProps"));
    // Native press routes to the button handler (node ui-1.1).
    t.emit("ui.press", { windowId: "win1", nodeId: "ui-1.1" });
    assert.equal(pressed, 1);
    await unmount("win1");
    core.setBridgeTransport(null);
    assert.ok(creates >= 3);
  });

  it("ui input change routes to onChange", async () => {
    const core = await import("@kitron/core");
    const t = new core.InMemoryTransport();
    t.on("ui", "create", async () => null);
    t.on("ui", "append", async () => null);
    t.on("ui", "setRoot", async () => null);
    t.on("ui", "setProps", async () => null);
    t.on("ui", "setStyle", async () => null);
    core.setBridgeTransport(t);
    const { View, TextInput, render, __resetUiForTests } = await import("@kitron/ui");
    __resetUiForTests();
    let seen = "";
    await render("win2", View({}, TextInput({ value: "", onChange: (v) => (seen = v) })));
    t.emit("ui.change", { windowId: "win2", nodeId: "ui-1.0", value: "hello" });
    assert.equal(seen, "hello");
    core.setBridgeTransport(null);
  });

  it("react renders state and routes press", async () => {
    const core = await import("@kitron/core");
    const props = {};
    const ops = [];
    const t = new core.InMemoryTransport();
    t.on("ui", "create", async () => null);
    t.on("ui", "append", async () => null);
    t.on("ui", "setRoot", async () => null);
    t.on("ui", "clearRoot", async () => null);
    t.on("ui", "setStyle", async () => null);
    t.on("ui", "setProps", async (_m, p) => { props[p.id] = p.props; ops.push(p.id); return null; });
    t.on("ui", "remove", async () => null);
    core.setBridgeTransport(t);
    const React = await import("react");
    const { render, unmount, __resetReactForTests, flushUi } = await import("@kitron/react");
    __resetReactForTests();
    function App() {
      const [count, setCount] = React.useState(0);
      return React.createElement("View", { style: { flex: 1 } },
        React.createElement("Text", null, "Hello ", count),
        React.createElement("Button", { title: "Go", onPress: () => setCount((c) => c + 1) }));
    }
    let setRoot = null;
    t.on("ui", "setRoot", async (_m, p) => { setRoot = p; return null; });
    await render(React.createElement(App), "winR");
    assert.equal(props["r-1"].text, "Hello 0");
    assert.ok(setRoot && setRoot.window === "winR" && typeof setRoot.root === "string");
    // Button is r-2 (bottom-up creation): press it.
    t.emit("ui.press", { windowId: "winR", nodeId: "r-2" });
    await new Promise((r) => setTimeout(r, 200));
    await flushUi();
    assert.equal(props["r-1"].text, "Hello 1");
    await unmount("winR");
    core.setBridgeTransport(null);
  });

  it("react input change reaches onChangeText", async () => {
    const core = await import("@kitron/core");
    const t = new core.InMemoryTransport();
    for (const m of ["create", "append", "setProps", "setStyle", "setRoot", "clearRoot", "remove"])
      t.on("ui", m, async () => null);
    core.setBridgeTransport(t);
    const React = await import("react");
    const { render, __resetReactForTests } = await import("@kitron/react");
    __resetReactForTests();
    let seen = "";
    function App() {
      return React.createElement("View", null,
        React.createElement("TextInput", { value: "", placeholder: "type", onChangeText: (v) => (seen = v) }));
    }
    await render(React.createElement(App), "winI");
    // r-1 = input (only child).
    t.emit("ui.change", { windowId: "winI", nodeId: "r-1", value: "hi" });
    assert.equal(seen, "hi");
    core.setBridgeTransport(null);
  });

  it("rn-compat: FlatList + Pressable + StyleSheet", async () => {
    const core = await import("@kitron/core");
    const creates = [];
    const t = new core.InMemoryTransport();
    for (const m of ["create", "append", "setProps", "setStyle", "setRoot", "clearRoot", "remove"])
      t.on("ui", m, async (_m, p) => { if (m === "create") creates.push(p); return null; });
    core.setBridgeTransport(t);
    const React = await import("react");
    const RN = await import("@kitron/react-native");
    const { __resetReactForTests, flushUi } = await import("@kitron/react");
    __resetReactForTests();
    const pressed = [];
    function App() {
      return React.createElement(RN.FlatList, {
        data: [{ id: "a", title: "Alpha" }, { id: "b", title: "Beta" }],
        keyExtractor: (item) => item.id,
        renderItem: ({ item }) => React.createElement(RN.Pressable, {
          onPress: () => pressed.push(item.id),
          children: React.createElement(RN.Text, { style: { fontSize: 16 } }, item.title),
        }),
      });
    }
    const { render } = await import("@kitron/react");
    await render(React.createElement(App), "winRN");
    // Two pressable rows created (pressable views participate in hit-test).
    const pressables = creates.filter((c) => c.props && c.props.pressable === true);
    assert.equal(pressables.length, 2);
    // StyleSheet.create is identity; flatten merges arrays.
    assert.deepEqual(RN.StyleSheet.flatten([{ flex: 1 }, { padding: 4 }]), { flex: 1, padding: 4 });
    // Press first row.
    t.emit("ui.press", { windowId: "winRN", nodeId: pressables[0].id });
    await new Promise((r) => setTimeout(r, 100));
    await flushUi();
    assert.deepEqual(pressed, ["a"]);
    core.setBridgeTransport(null);
  });

  it("rn-compat: ScrollView onScroll + Image uri + Alert + Animated", async () => {
    const core = await import("@kitron/core");
    const creates = [];
    const t = new core.InMemoryTransport();
    for (const m of ["create", "append", "setProps", "setStyle", "setRoot", "clearRoot", "remove"])
      t.on("ui", m, async (_m, p) => { if (m === "create") creates.push(p); return null; });
    t.on("dialog", "message", async (_m, p) => ({ button: 1 }));
    core.setBridgeTransport(t);
    const React = await import("react");
    const RN = await import("@kitron/react-native");
    const { __resetReactForTests } = await import("@kitron/react");
    __resetReactForTests();
    const scrolled = [];
    function App() {
      return React.createElement(RN.ScrollView, {
        onScroll: (e) => scrolled.push(e.contentOffset),
        children: React.createElement(RN.Image, { source: { uri: "file:///tmp/x.png" } }),
      });
    }
    const { render } = await import("@kitron/react");
    await render(React.createElement(App), "winS");
    const scrollView = creates.find((c) => c.kind === "view" && c.props && c.props.scrollable === true);
    assert.ok(scrollView, "scrollable view created");
    const img = creates.find((c) => c.kind === "image");
    assert.equal(img && img.props.uri, "file:///tmp/x.png");
    t.emit("ui.scroll", { windowId: "winS", nodeId: scrollView.id, x: 0, y: 120 });
    assert.deepEqual(scrolled, [{ x: 0, y: 120 }]);
    // Alert resolves the pressed button.
    let alerted = "";
    await RN.Alert.alert("Sure?", "Really", [{ text: "No" }, { text: "Yes", onPress: () => (alerted = "yes") }]);
    assert.equal(alerted, "yes");
    // Animated timing drives a value.
    const v = new RN.Animated.Value(0);
    const seen = [];
    v.addListener((x) => seen.push(x));
    await new Promise((resolve) => RN.Animated.timing(v, { toValue: 1, duration: 40 }).start(() => resolve()));
    assert.ok(v.getValue() === 1 && seen.length > 0);
    assert.equal(RN.Platform.OS, "kitron");
    assert.equal(typeof RN.Dimensions.get("window").width, "number");
    assert.equal(RN.processColor("#ff0000"), 0xff0000ff);
    core.setBridgeTransport(null);
  });

  it("rn-compat: native modules throw a guided error", async () => {
    const RN = await import("@kitron/react-native");
    assert.throws(() => RN.NativeModules.SomeNativeThing, (e) => e.code === "NATIVE_ADAPTER_MISSING");
    assert.throws(() => RN.TurboModuleRegistry.get("Foo"), (e) => e.code === "NATIVE_ADAPTER_MISSING");
    assert.throws(() => RN.requireNativeComponent("FooView"), (e) => e.code === "NATIVE_ADAPTER_MISSING");
  });
});
