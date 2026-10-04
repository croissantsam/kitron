import { existsSync } from "node:fs";
import { join } from "node:path";
import { spawn, type ChildProcess } from "node:child_process";
import { IPCPeer, IpcHub, parseFrameLine } from "@kitron/ipc";
import { configureTransport, setPermissionChecker } from "@kitron/core";
import { loadConfig } from "./config.js";
import { findFreePort } from "./ports.js";
import { attachMockNativeHost } from "./mock-native.js";
import { createPermissionChecker } from "./permissions.js";
import { RustHostManager } from "./rust-host.js";
import { NodeBackendManager } from "./node-manager.js";

export interface DevOptions {
  cwd?: string;
  port?: number;
  host?: string;
  openDevtools?: boolean;
}

/**
 * `kitron dev` pipeline — §27:
 * Vite + TanStack Start, Node.js backend, Rust host / mock fallback.
 *
 * All three processes meet on an {@link IpcHub} (§10):
 * host channels (`app.*`, `window.*`, `native.*`, …) go to the real
 * `kitron-host` binary when compiled, else to the in-process mock;
 * everything else (user `ipc.handle` channels, plugins) goes to the
 * Node.js backend child over stdio.
 */
export async function runDev(opts: DevOptions = {}): Promise<{ stop: () => void; url: string }> {
  const cwd = opts.cwd ?? process.cwd();
  const config = await loadConfig(cwd);
  const host = opts.host ?? config.dev.host ?? "127.0.0.1";
  const port = await findFreePort(opts.port ?? config.dev.port ?? undefined, host);
  const url = `http://${host}:${port}`;
  const hub = new IpcHub();

  // 1. Host link: real binary preferred, mock fallback.
  const rust = new RustHostManager({
    cwd,
    url,
    title: config.build.productName,
    devtools: opts.openDevtools ?? config.dev.devtools,
  });
  rust.onLine = (line) => {
    if (parseFrameLine(line)) hub.routeFrom("host", line);
    else console.log(line);
  };
  const realHost = rust.start();
  if (realHost) {
    hub.addLink({ name: "host", deliver: (f) => rust.send(f) }, { host: true });
  } else {
    const mockPeer = new IPCPeer((frame) => hub.routeFrom("mock", frame));
    attachMockNativeHost(mockPeer, {
      appName: config.build.productName,
      appVersion: config.build.version,
    });
    hub.addLink({ name: "mock", deliver: (f) => mockPeer.receive(f) }, { host: true });
  }

  // 2. Orchestrator-side peer (CLI-internal core calls + hub completeness).
  const nodePeer = new IPCPeer((frame) => hub.routeFrom("node", frame));
  hub.addLink({ name: "node", deliver: (f) => nodePeer.receive(f) });
  configureTransport(nodePeer);
  setPermissionChecker(createPermissionChecker(config.permissions));

  console.log(`[Kitron][Node] Starting dev session → ${url}`);

  // 3. Vite dev server (project's own vite, if present).
  const vite = startVite(cwd, host, port);

  // 4. Node backend child (default IPC route for user channels).
  const backendEntry = join(cwd, config.build.backendEntry);
  let backend: NodeBackendManager | null = null;
  if (existsSync(backendEntry)) {
    backend = new NodeBackendManager({
      entry: backendEntry,
      cwd,
      env: { KITRON_DEV_URL: url, KITRON_APP_ID: config.build.appId },
    });
    backend.onLine = (line) => {
      if (parseFrameLine(line)) hub.routeFrom("backend", line);
      else console.log(line);
    };
    backend.onStderr = (line) => console.error(line);
    hub.addLink({ name: "backend", deliver: (f) => backend!.send(f) }, { isDefault: true });
    backend.start();
  } else {
    console.log("[Kitron][Node] No backend entry found — renderer-only mode.");
  }

  console.log(`[Kitron][Renderer] Application started → ${url}`);
  console.log("[Kitron] Hot reload works through Vite. Press Ctrl+C to stop.");

  const stop = () => {
    vite?.kill("SIGTERM");
    backend?.stop();
    rust.stop();
  };
  process.on("SIGINT", () => {
    stop();
    process.exit(0);
  });

  return { stop, url };
}

function startVite(cwd: string, host: string, port: number): ChildProcess | null {
  const viteBin = join(cwd, "node_modules", ".bin", process.platform === "win32" ? "vite.cmd" : "vite");
  if (!existsSync(viteBin) && !existsSync(join(cwd, "vite.config.ts")) && !existsSync(join(cwd, "vite.config.js"))) {
    console.log("[Kitron][Node] No Vite project detected — skipping Vite (serve static or set port manually).");
    return null;
  }
  try {
    const child = spawn(viteBin, ["--host", host, "--port", String(port), "--strictPort"], {
      cwd,
      env: { ...process.env },
      stdio: "inherit",
    });
    child.on("error", () => {
      console.log("[Kitron][Node] Could not start Vite — is it installed?");
    });
    return child;
  } catch {
    return null;
  }
}
