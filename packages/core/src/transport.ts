import { IPCPeer } from "@kitron/ipc";
import { KitronError } from "@kitron/types";
import { registerNodeHandlers } from "./backend-handlers.js";

let peer: IPCPeer | null = null;
let permissionChecker: ((capability: string) => void) | null = null;

/** Wire the underlying transport (called by @kitron/runtime or tests). */
export function configureTransport(next: IPCPeer | null): void {
  peer = next;
}

function isNode(): boolean {
  return typeof process !== "undefined" && !!process.versions?.node;
}

/**
 * Lazily create the peer. When the backend runs as a CLI-spawned child
 * (`KITRON_IPC=stdio`), the transport self-configures over stdio so user
 * entries (`src/main.kitron.ts`) work unmodified.
 */
function ensurePeer(): IPCPeer {
  if (!peer && isNode() && process.env["KITRON_IPC"] === "stdio") {
    const auto = new IPCPeer((frame) => {
      try {
        process.stdout.write(frame);
      } catch {
        // stdout gone — drop the frame.
      }
    });
    let buffer = "";
    process.stdin.on("data", (d: Buffer) => {
      buffer += String(d);
      let idx: number;
      while ((idx = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, idx + 1);
        buffer = buffer.slice(idx + 1);
        if (line.trim().length > 0) auto.receive(line);
      }
    });
    peer = auto;
    // Backend-implemented channels (shell/theme/power/…) serve locally.
    try {
      registerNodeHandlers(auto);
    } catch {
      // registration must never break transport setup.
    }
  }
  if (!peer) {
    throw new KitronError(
      "IPC_FAILED",
      "Kitron transport is not configured. In an app backend this is wired automatically; in tests use configureTransport() with an in-memory peer.",
    );
  }
  return peer;
}

export function getPeer(): IPCPeer {
  return ensurePeer();
}

export function isTransportConfigured(): boolean {
  if (peer) return true;
  return isNode() && process.env["KITRON_IPC"] === "stdio";
}

/** Runtime installs a permission gate (`kitron.config.ts` → §21). */
export function setPermissionChecker(fn: ((capability: string) => void) | null): void {
  permissionChecker = fn;
}

export function requirePermission(capability: string): void {
  permissionChecker?.(capability);
}
