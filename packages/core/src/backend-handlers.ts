import type { IPCPeer } from "@kitron/ipc";
import { nativeTheme, powerMonitor } from "./theme.js";
import { shell } from "./shell.js";
import { crashReporter } from "./crash.js";
import { safeStorage } from "./safe.js";
import { updater } from "./updater.js";
import { emitProtocolURLs, parseProtocolURLs, setAsDefaultProtocolClient } from "./protocol.js";
import { onSecondInstance, requestSingleInstanceLock } from "./single-instance.js";
import { getPeer } from "./transport.js";

/**
 * Channels served directly by the Node.js backend (Rule 2) — Tranche A.
 * Auto-registered for CLI-spawned backends; importable for custom hosts.
 */
export function registerNodeHandlers(peer: IPCPeer = getPeer()): void {
  const once = (channel: string, fn: (payload: never) => unknown) => {
    if (!peer.registry.has(channel)) peer.handle(channel, fn);
  };

  once("shell.openExternal", (p: unknown) =>
    shell.openExternal(String((p as { url?: string })?.url ?? "")),
  );
  once("shell.openPath", (p: unknown) =>
    shell.openPath(String((p as { path?: string })?.path ?? "")),
  );
  once("shell.showItemInFolder", (p: unknown) => {
    void shell.showItemInFolder(String((p as { path?: string })?.path ?? ""));
    return null;
  });
  once("shell.trashItem", (p: unknown) =>
    shell.trashItem(String((p as { path?: string })?.path ?? "")),
  );
  once("shell.beep", () => {
    shell.beep();
    return null;
  });

  once("theme.getInfo", () => nativeTheme.getInfo());
  once("theme.setSource", (p: unknown) => {
    nativeTheme.themeSource = (p as { source?: "system" | "light" | "dark" })?.source ?? "system";
    return nativeTheme.getInfo();
  });
  once("power.isOnBattery", () => powerMonitor.isOnBatteryPower());
  once("power.getIdleState", () => powerMonitor.getSystemIdleState());
  once("power.getIdleTime", () => powerMonitor.getSystemIdleTime());

  once("safe.encrypt", (p: unknown) =>
    safeStorage
      .encryptString(String((p as { text?: string })?.text ?? ""))
      .then((b) => b.toString("base64")),
  );
  once("safe.decrypt", (p: unknown) =>
    safeStorage.decryptString(Buffer.from(String((p as { data?: string })?.data ?? ""), "base64")),
  );
  once("safe.available", () => safeStorage.isEncryptionAvailable());

  once("updater.setFeed", (p: unknown) => {
    updater.setFeedURL(
      String((p as { url?: string })?.url ?? ""),
      (p as { version?: string })?.version,
    );
    return null;
  });
  once("updater.checkFeed", () => updater.checkForUpdates());

  once("app.requestSingleInstanceLock", (p: unknown) =>
    requestSingleInstanceLock({ appId: (p as { appId?: string })?.appId }),
  );
  once("app.setProtocolClient", (p: unknown) =>
    setAsDefaultProtocolClient(String((p as { protocol?: string })?.protocol ?? "kitron")),
  );
  once("app.getProtocolURLs", () => parseProtocolURLs());

  // Re-emit backend-local events onto the bus for the renderer.
  try {
    crashReporter.start();
  } catch {
    // reporting must never break startup.
  }
  onSecondInstance((info) => {
    try {
      peer.emit("second-instance", info);
    } catch {
      // bus optional.
    }
  });
  for (const url of emitProtocolURLs()) {
    void url;
  }
}
