import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { watch } from "node:fs";
import type { SecondInstanceInfo } from "@kitron/types";
import { getPeer } from "./transport.js";

function lockDir(appId: string): string {
  const safe = appId.replace(/[^a-zA-Z0-9_.-]+/g, "-");
  return join(tmpdir(), `kitron-${safe}.lock`);
}

export interface SingleInstanceOptions {
  appId?: string;
}

/**
 * `requestSingleInstanceLock` equivalent — Tranche A, Node-first.
 * First instance owns a lock dir and watches the handoff file; a second
 * instance drops its argv there and exits. The first one re-emits
 * `second-instance` (also forwarded on the IPC bus for the renderer).
 */
export function requestSingleInstanceLock(opts: SingleInstanceOptions = {}): boolean {
  const appId =
    opts.appId ?? process.env["KITRON_APP_ID"] ?? "com.kitron.app";
  const dir = lockDir(appId);
  const handoff = join(dir, "argv.json");

  try {
    mkdirSync(dir, { recursive: true });
    // Atomic-ish claim: write our pid; if present and alive → second instance.
    const pidFile = join(dir, "pid");
    if (existsSync(pidFile)) {
      const pid = Number(readFileSync(pidFile, "utf8"));
      if (Number.isFinite(pid) && pid > 0) {
        try {
          process.kill(pid, 0);
          // Owner alive → hand off and report second instance.
          writeFileSync(
            handoff,
            JSON.stringify({
              argv: process.argv.slice(1),
              workingDirectory: process.cwd(),
              at: Date.now(),
            } satisfies SecondInstanceInfo & { at: number }),
          );
          return false;
        } catch {
          // Stale lock — take over.
        }
      }
    }
    writeFileSync(pidFile, String(process.pid));
    const onExit = () => {
      try {
        rmSync(dir, { recursive: true, force: true });
      } catch {
        // best effort.
      }
    };
    process.once("exit", onExit);

    // Watch for second-instance handoffs.
    try {
      const w = watch(dir, (event, filename) => {
        if (filename !== "argv.json") return;
        void event;
        try {
          const info = JSON.parse(readFileSync(handoff, "utf8")) as SecondInstanceInfo;
          try {
            getPeer().emit("second-instance", info);
          } catch {
              // transport may not exist yet — ignore.
          }
          secondInstanceListeners.forEach((fn) => {
            try {
              fn(info);
            } catch {
              // listener errors must not break dispatch.
            }
          });
        } catch {
          // handoff not yet complete — ignore.
        }
      });
      w.unref?.();
    } catch {
      // watching unavailable — lock still valid.
    }
    return true;
  } catch {
    // If we cannot lock at all, behave as single (fail open).
    return true;
  }
}

const secondInstanceListeners = new Set<(info: SecondInstanceInfo) => void>();

export function onSecondInstance(listener: (info: SecondInstanceInfo) => void): () => void {
  secondInstanceListeners.add(listener);
  return () => {
    secondInstanceListeners.delete(listener);
  };
}
