/**
 * Preload script — injected into every WebView before page scripts (§20).
 *
 * Guarantees:
 * - no `window.require / process / fs / node` leaks,
 * - a strict channel whitelist,
 * - `window.kitron = { invoke, emit, subscribe, native }` only.
 *
 * The native side must provide `__kitronBridge.postMessage(text)` and call
 * `__kitronDispatch(text)` for inbound frames (wired by the Rust host).
 */

/** Channels the renderer is allowed to invoke/subscribe. */
export const RENDERER_ALLOWLIST: RegExp[] = [
  /^ping$/,
  /^app\.(getName|getVersion|getPath|getInfo|isReady|ready|requestSingleInstanceLock|setProtocolClient|getProtocolURLs)$/,
  /^window\.[a-zA-Z]+$/,
  /^webcontents\.[a-zA-Z]+$/,
  /^shell\.(openExternal|openPath|showItemInFolder|trashItem|beep)$/,
  /^theme\.(getInfo|setSource)$/,
  /^power\.(isOnBattery|getIdleState|getIdleTime)$/,
  /^safe\.(encrypt|decrypt|available)$/,
  /^updater\.(check|download|install|setFeed|checkFeed)$/,
  /^native\.(notification|dialog|menu|tray|clipboard|screen|desktop|shortcut|power|window)\.[a-zA-Z]+$/,
  /^plugin\.[a-z0-9-]+\.[a-zA-Z0-9_-]+$/,
  /^log$/,
  /^(second-instance|open-url|theme\.updated)$/,
];

export function isChannelAllowed(channel: string): boolean {
  return RENDERER_ALLOWLIST.some((re) => re.test(channel));
}

/** Build the preload JavaScript source. Inlined by the Rust host / vite plugin. */
export function buildPreloadSource(): string {
  const allowlist = RENDERER_ALLOWLIST.map((r) => r.source);
  return `(() => {
  "use strict";
  const ALLOW = [${allowlist.map((s) => `/${s}/`).join(",")}];
  const allowed = (ch) => ALLOW.some((re) => re.test(ch));
  const pending = new Map();
  let seq = 0;
  const post = (text) => {
    const bridge = window.__kitronBridge;
    if (!bridge || typeof bridge.postMessage !== "function") {
      throw new Error("Kitron bridge unavailable");
    }
    bridge.postMessage(text);
  };
  window.__kitronDispatch = (text) => {
    let msg;
    try { msg = JSON.parse(text); } catch { return; }
    if (msg && typeof msg.id === "string" && typeof msg.success === "boolean") {
      const entry = pending.get(msg.id);
      if (!entry) return;
      pending.delete(msg.id);
      clearTimeout(entry.timer);
      if (msg.success) entry.resolve(msg.payload);
      else entry.reject(Object.assign(new Error(msg.error && msg.error.message || "IPC failed"), { code: msg.error && msg.error.code }));
      return;
    }
    if (msg && typeof msg.channel === "string" && !("id" in msg)) {
      const set = listeners.get(msg.channel);
      if (set) for (const fn of [...set]) { try { fn(msg.payload); } catch {} }
    }
  };
  const listeners = new Map();
  function invoke(channel, payload, opts) {
    if (!allowed(channel)) return Promise.reject(Object.assign(new Error('Channel "' + channel + '" is not allowed'), { code: "PERMISSION_DENIED" }));
    const id = "r-" + (++seq) + "-" + Date.now();
    const timeout = (opts && opts.timeout) || 30000;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => { pending.delete(id); reject(Object.assign(new Error('Invoke "' + channel + '" timed out'), { code: "IPC_TIMEOUT" })); }, timeout);
      pending.set(id, { resolve, reject, timer });
      try { post(JSON.stringify({ id, channel, payload: payload ?? null })); }
      catch (e) { pending.delete(id); clearTimeout(timer); reject(e); }
    });
  }
  function emit(channel, payload) {
    if (!allowed(channel)) throw Object.assign(new Error('Channel "' + channel + '" is not allowed'), { code: "PERMISSION_DENIED" });
    post(JSON.stringify({ channel, payload: payload ?? null }));
  }
  function subscribe(channel, listener) {
    if (!allowed(channel)) throw Object.assign(new Error('Channel "' + channel + '" is not allowed'), { code: "PERMISSION_DENIED" });
    let set = listeners.get(channel);
    if (!set) { set = new Set(); listeners.set(channel, set); }
    set.add(listener);
    return () => { set.delete(listener); if (set.size === 0) listeners.delete(channel); };
  }
  const native = {
    notification: { show: (o) => invoke("native.notification.show", o) },
    window: {
      setAlwaysOnTop: (flag) => invoke("native.window.setAlwaysOnTop", { flag }),
      setTitle: (title) => invoke("native.window.setTitle", { title }),
    },
    dialog: {
      open: (o) => invoke("native.dialog.open", o ?? {}),
      save: (o) => invoke("native.dialog.save", o ?? {}),
    },
    clipboard: {
      readText: () => invoke("native.clipboard.readText"),
      writeText: (text) => invoke("native.clipboard.writeText", { text }),
    },
  };
  Object.defineProperty(window, "kitron", { value: Object.freeze({ invoke, emit, subscribe, native }), writable: false, configurable: false });
  // Explicitly seal the dangerous globals shut.
  for (const key of ["require", "node", "process", "fs"]) {
    try { if (key in window) Object.defineProperty(window, key, { value: undefined, writable: false, configurable: false }); } catch {}
  }
})();`;
}
