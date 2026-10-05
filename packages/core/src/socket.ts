/**
 * @kitron/core — Unix socket transport.
 * Connects to a runtime started with `--socket PATH` (used by `kitron dev`
 * for the UI runner). Same NDJSON envelope as the stdio transport.
 */
import net from "node:net";
import readline from "node:readline";
import { KitronError } from "./errors.js";
import type { IpcResponse, IpcTransport } from "./ipc.js";

export interface SocketTransportOptions {
  path: string;
  timeoutMs?: number;
}

export class SocketTransport implements IpcTransport {
  private sock: net.Socket;
  private pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: unknown) => void }>();
  private nextId = 1;
  private eventListeners = new Set<(name: string, payload: unknown) => void>();
  private closed = false;
  readonly ready: Promise<void>;

  constructor(opts: SocketTransportOptions) {
    this.sock = net.createConnection(opts.path);
    this.ready = new Promise((resolve, reject) => {
      const timer = setTimeout(() => reject(new Error(`socket connect timeout: ${opts.path}`)), opts.timeoutMs ?? 10_000);
      this.sock.once("connect", () => {
        clearTimeout(timer);
        resolve();
      });
      this.sock.once("error", (err) => {
        clearTimeout(timer);
        reject(err);
      });
    });
    const rl = readline.createInterface({ input: this.sock });
    rl.on("line", (line) => this.onLine(line.trim()));
    const fail = () => {
      this.closed = true;
      for (const [, p] of this.pending) {
        p.reject(new KitronError({ code: "KITRON_IPC_TRANSPORT_CLOSED", message: "runtime socket closed" }));
      }
      this.pending.clear();
    };
    this.sock.on("close", fail);
    this.sock.on("error", () => {
      // 'close' follows; pending handled there unless never connected.
      if (!this.closed) fail();
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
    if (this.closed || !this.sock.writable) {
      return Promise.reject(
        new KitronError({ code: "KITRON_IPC_TRANSPORT_CLOSED", message: "runtime socket not connected" }),
      );
    }
    const id = this.nextId++;
    const frame = JSON.stringify({ id, domain, method, params: params ?? {} }) + "\n";
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.sock.write(frame, (err) => {
        if (err) {
          this.pending.delete(id);
          reject(new KitronError({ code: "KITRON_IPC_TRANSPORT_CLOSED", message: String(err) }));
        }
      });
    });
  }

  close(): void {
    try {
      this.sock.destroy();
    } catch {
      // ignore
    }
  }
}
