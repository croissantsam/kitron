import { homedir, tmpdir } from "node:os";
import { IPCPeer, createInMemoryPair } from "@kitron/ipc";
import { KitronError } from "@kitron/types";
import type { BrowserWindowOptions } from "@kitron/types";

/**
 * In-memory fallback for the Rust host.
 * Used by `kitron dev` when the native binary is not compiled yet, and by tests.
 * Implements the MVP surface: window registry, `ping → pong`, notifications.
 */
export function attachMockNativeHost(peer: IPCPeer, opts: { appName?: string; appVersion?: string } = {}): void {
  const appName = opts.appName ?? "Kitron App";
  const appVersion = opts.appVersion ?? "0.1.0";
  let ready = true;

  const windows = new Map<string, BrowserWindowOptions & { title: string }>();

  const safe = (channel: string, fn: (payload: never) => unknown) => {
    if (!peer.registry.has(channel)) peer.handle(channel, fn);
  };

  // Golden demo: invoke("ping") → "pong" (§50).
  safe("ping", () => "pong");

  safe("app.ready", async () => undefined);
  safe("app.isReady", () => ready);
  safe("app.getName", () => appName);
  safe("app.getVersion", () => appVersion);
  safe("app.getInfo", () => ({
    name: appName,
    version: appVersion,
    locale: Intl.DateTimeFormat().resolvedOptions().locale ?? "en-US",
    platform: process.platform,
    arch: process.arch,
    ready,
  }));
  safe("app.getPath", (payload: unknown) => {
    const name = (payload as { name?: string })?.name ?? "userData";
    const home = homedir();
    const tmp = tmpdir();
    const map: Record<string, string> = {
      home,
      temp: tmp,
      userData: `${home}/Library/Application Support/${appName}`,
      appData: `${home}/Library/Application Support`,
      documents: `${home}/Documents`,
      downloads: `${home}/Downloads`,
      desktop: `${home}/Desktop`,
      pictures: `${home}/Pictures`,
      music: `${home}/Music`,
      videos: `${home}/Movies`,
      logs: `${home}/Library/Logs/${appName}`,
      exe: process.execPath,
    };
    return map[name] ?? map["userData"]!;
  });
  safe("app.quit", () => {
    ready = false;
    peer.emit("app.quit-requested");
    return null;
  });
  safe("app.exit", () => null);

  safe("window.create", (payload: unknown) => {
    const p = payload as { id: string; options?: BrowserWindowOptions };
    if (!p?.id) throw new KitronError("WINDOW_FAILED", "window.create requires an id");
    windows.set(p.id, {
      ...(p.options ?? {}),
      title: p.options?.title ?? appName,
    });
    console.log(`[Kitron][Rust] WebView created (window ${p.id})`);
    return { id: p.id };
  });

  const withWindow = (fn: (id: string, payload: Record<string, unknown>) => unknown) => {
    return (payload: unknown) => {
      const p = (payload ?? {}) as Record<string, unknown>;
      const id = String(p["id"] ?? "");
      if (!windows.has(id)) throw new KitronError("WINDOW_FAILED", `Unknown window "${id}"`);
      return fn(id, p);
    };
  };

  for (const action of [
    "loadURL",
    "loadFile",
    "show",
    "hide",
    "close",
    "focus",
    "minimize",
    "maximize",
    "restore",
    "reload",
    "openDevTools",
    "closeDevTools",
  ]) {
    safe(`window.${action}`, withWindow(() => null));
  }
  safe("window.setTitle", withWindow((id, p) => {
    windows.get(id)!.title = String(p["title"] ?? "");
    return null;
  }));
  safe("window.setSize", withWindow(() => null));
  safe("window.setPosition", withWindow(() => null));
  safe("window.setAlwaysOnTop", withWindow(() => null));
  safe("window.isMaximized", withWindow(() => false));
  let fullscreen = false;
  safe("window.setFullscreen", withWindow((_id, p) => {
    fullscreen = Boolean((p as Record<string, unknown>)["flag"] ?? true);
    return null;
  }));
  safe("window.isFullscreen", withWindow(() => fullscreen));
  safe("window.setMinimumSize", withWindow(() => null));
  safe("window.setMaximumSize", withWindow(() => null));

  safe("native.window.setAlwaysOnTop", () => null);
  safe("native.window.setTitle", () => null);
  safe("native.notification.show", (payload: unknown) => {
    const p = (payload ?? {}) as { title?: string; body?: string };
    console.log(`[Kitron][Rust] Notification: ${p["title"] ?? ""} — ${p["body"] ?? ""}`);
    return null;
  });
  safe("native.dialog.open", () => ({ canceled: true, filePaths: [] }));
  safe("native.dialog.save", () => ({ canceled: true, filePaths: [] }));
  safe("native.dialog.message", () => ({ canceled: true, response: 0 }));
  safe("native.menu.set", () => null);
  safe("native.menu.popup", () => null);
  safe("native.tray.create", () => `tray-${Date.now()}`);
  safe("native.tray.destroy", () => null);
  safe("native.clipboard.readText", () => "");
  safe("native.clipboard.writeText", () => null);
  safe("native.clipboard.writeFormat", () => null);
  safe("native.clipboard.read", () => "");
  safe("native.clipboard.clear", () => null);
  safe("native.screen.getDisplays", () => []);
  safe("native.screen.getCursor", () => ({ x: 0, y: 0 }));
  safe("native.desktop.getSources", () => []);
  safe("native.shortcut.register", () => null);
  safe("native.shortcut.unregister", () => null);
  safe("native.power.saveBlockerStart", () => 0);
  safe("native.power.saveBlockerStop", () => null);
  safe("updater.check", () => ({ available: false }));
  safe("updater.download", () => null);
  safe("updater.install", () => null);

  // Tranche B: webContents without a JS engine — typed failure so callers
  // get a clear error instead of a hang. Real host evaluates in the WebView.
  const noJS = () => {
    throw new KitronError(
      "PLATFORM_UNSUPPORTED",
      "webContents evaluation requires the real native host (mock has no JS engine)",
    );
  };
  safe("webcontents.evalJS", noJS);
  safe("webcontents.getURL", noJS);
  safe("webcontents.getTitle", noJS);
  safe("webcontents.setZoomFactor", noJS);
  safe("webcontents.getZoomFactor", () => 1);
  safe("webcontents.capturePage", noJS);
  safe("webcontents.printToPDF", noJS);
  safe("webcontents.reload", () => null);
  safe("native.clipboard.readImage", noJS);
  safe("native.clipboard.writeImage", noJS);
}

/** A loopback pair: [nodePeer, hostPeer] with the mock host attached. */
export function createMockBackend(opts?: { appName?: string; appVersion?: string }): {
  nodePeer: IPCPeer;
  hostPeer: IPCPeer;
} {
  const [nodePeer, hostPeer] = createInMemoryPair();
  attachMockNativeHost(hostPeer, opts);
  return { nodePeer, hostPeer };
}
