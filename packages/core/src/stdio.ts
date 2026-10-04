/**
 * @kitron/core — stdio transport (MVP).
 * Spawns `kitron-runtime run` and speaks newline-delimited JSON.
 * Production replaces this with JSI / Unix-socket without changing dispatch.
 */
import { spawn, type ChildProcess } from "node:child_process";
import readline from "node:readline";
import { KitronError } from "./errors.js";
import type { IpcTransport } from "./ipc.js";
import type { IpcResponse } from "./ipc.js";

export interface StdioTransportOptions {
  binary: string;
  args?: string[];
  cwd?: string;
}

export class StdioTransport implements IpcTransport {
  private proc: ChildProcess;
  private pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: unknown) => void }>();
  private nextId = 1;
  private eventListeners = new Set<(name: string, payload: unknown) => void>();
  private closed = false;

  constructor(opts: StdioTransportOptions) {
    this.proc = spawn(opts.binary, opts.args ?? ["run", "--headless"], {
      cwd: opts.cwd,
      stdio: ["pipe", "pipe", "inherit"],
    });
    const rl = readline.createInterface({ input: this.proc.stdout! });
    rl.on("line", (line) => this.onLine(line.trim()));
    this.proc.on("exit", () => {
      this.closed = true;
      for (const [, p] of this.pending) {
        p.reject(new KitronError({ code: "KITRON_IPC_TRANSPORT_CLOSED", message: "runtime exited" }));
      }
      this.pending.clear();
    });
  }

  private onLine(line: string): void {
    if (!line) return;
    let msg: unknown;
    try {
      msg = JSON.parse(line);
    } catch {
      return;
    }
    if (typeof msg !== "object" || msg === null) return;
    const o = msg as Record<string, unknown>;
    if (o["type"] === "event" && typeof o["name"] === "string") {
      for (const l of [...this.eventListeners]) l(o["name"] as string, o["payload"]);
      return;
    }
    if (typeof o["id"] === "number" && typeof o["ok"] === "boolean") {
      const id = o["id"] as number;
      const p = this.pending.get(id);
      if (!p) return;
      this.pending.delete(id);
      const resp = msg as IpcResponse;
      if (resp.ok) p.resolve((resp as { result: unknown }).result);
      else p.reject(KitronError.fromJSON((resp as { error: unknown }).error));
    }
  }

  onEvent(listener: (name: string, payload: unknown) => void): () => void {
    this.eventListeners.add(listener);
    return () => {
      this.eventListeners.delete(listener);
    };
  }

  invoke(domain: string, method: string, params?: unknown): Promise<unknown> {
    if (this.closed || !this.proc.stdin?.writable) {
      return Promise.reject(
        new KitronError({ code: "KITRON_IPC_TRANSPORT_CLOSED", message: "runtime not running" }),
      );
    }
    const id = this.nextId++;
    const frame = JSON.stringify({ id, domain, method, params: params ?? {} }) + "\n";
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.proc.stdin!.write(frame, (err) => {
        if (err) {
          this.pending.delete(id);
          reject(new KitronError({ code: "KITRON_IPC_TRANSPORT_CLOSED", message: String(err) }));
        }
      });
    });
  }

  close(): void {
    try {
      this.proc.kill();
    } catch {
      // ignore
    }
  }
}
