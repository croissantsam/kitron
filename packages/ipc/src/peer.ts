import { randomUUID } from "node:crypto";
import type {
  IPCEvent,
  IPCEventListener,
  IPCInvokeOptions,
  IPCRequest,
  IPCResponse,
  IPCUnsubscribe,
} from "@kitron/types";
import { KitronError } from "@kitron/types";
import { NDJSONDecoder, classifyFrame, encodeFrame } from "./codec.js";
import { HandlerRegistry } from "./router.js";

export const DEFAULT_INVOKE_TIMEOUT = 30_000;

export interface PeerSend {
  (frame: string): void;
}

export interface PeerOptions {
  /** Timeout applied when `invoke` has no explicit timeout. */
  defaultTimeout?: number;
  /** Called for frames that are neither request/response/event. */
  onUnknownFrame?: (value: unknown) => void;
  /** Called when a local handler throws. */
  onHandlerError?: (channel: string, error: unknown) => void;
  generateId?: () => string;
}

/**
 * Transport-agnostic IPC peer.
 *
 * - `invoke(channel, payload)` sends a request and resolves with the response.
 * - `handle(channel, fn)` registers a local handler.
 * - `emit(channel, payload)` broadcasts an event.
 * - `subscribe(channel, listener)` listens for events.
 *
 * Wire bytes with `receive(data)` / `send` — the caller owns the transport
 * (stdio, TCP, WebView bridge…).
 */
export class IPCPeer {
  readonly registry = new HandlerRegistry();
  private pending = new Map<
    string,
    { resolve: (v: unknown) => void; reject: (e: unknown) => void; timer: NodeJS.Timeout }
  >();
  private eventListeners = new Map<string, Set<IPCEventListener>>();
  private decoder = new NDJSONDecoder();
  private send: PeerSend;
  private opts: Required<Pick<PeerOptions, "defaultTimeout">> &
    Pick<PeerOptions, "onUnknownFrame" | "onHandlerError" | "generateId">;

  constructor(send: PeerSend, opts: PeerOptions = {}) {
    this.send = send;
    this.opts = {
      defaultTimeout: opts.defaultTimeout ?? DEFAULT_INVOKE_TIMEOUT,
      onUnknownFrame: opts.onUnknownFrame,
      onHandlerError: opts.onHandlerError,
      generateId: opts.generateId ?? (() => randomUUID()),
    };
  }

  setSender(send: PeerSend): void {
    this.send = send;
  }

  handle<TPayload = unknown, TResult = unknown>(
    channel: string,
    handler: (payload: TPayload) => Promise<TResult> | TResult,
  ): () => void {
    return this.registry.handle(channel, handler);
  }

  removeHandler(channel: string): void {
    this.registry.remove(channel);
  }

  invoke<T = unknown>(
    channel: string,
    payload?: unknown,
    options: IPCInvokeOptions = {},
  ): Promise<T> {
    const id = this.opts.generateId!();
    const timeout = options.timeout ?? this.opts.defaultTimeout;
    const request: IPCRequest = { id, channel, payload: payload ?? null };

    const promise = new Promise<T>((resolve, reject) => {
      const onAbort = () => {
        this.pending.delete(id);
        clearTimeout(timer);
        reject(new KitronError("IPC_FAILED", `Invoke "${channel}" aborted`));
      };

      const timer = setTimeout(() => {
        this.pending.delete(id);
        options.signal?.removeEventListener("abort", onAbort);
        reject(
          new KitronError("IPC_TIMEOUT", `Invoke "${channel}" timed out after ${timeout}ms`, {
            channel,
            id,
          }),
        );
      }, timeout);
      // Don't keep the process alive just for a pending invoke.
      timer.unref?.();

      if (options.signal?.aborted) {
        clearTimeout(timer);
        reject(new KitronError("IPC_FAILED", `Invoke "${channel}" aborted`));
        return;
      }
      options.signal?.addEventListener("abort", onAbort, { once: true });

      this.pending.set(id, {
        resolve: (v) => {
          clearTimeout(timer);
          options.signal?.removeEventListener("abort", onAbort);
          resolve(v as T);
        },
        reject: (e) => {
          clearTimeout(timer);
          options.signal?.removeEventListener("abort", onAbort);
          reject(e);
        },
        timer,
      });

      try {
        this.send(encodeFrame(request));
      } catch (err) {
        this.pending.delete(id);
        clearTimeout(timer);
        reject(KitronError.fromUnknown(err));
      }
    });

    return promise;
  }

  emit(channel: string, payload?: unknown): void {
    const event: IPCEvent = { channel, payload: payload ?? null };
    this.send(encodeFrame(event));
  }

  subscribe<T = unknown>(channel: string, listener: IPCEventListener<T>): IPCUnsubscribe {
    let set = this.eventListeners.get(channel);
    if (!set) {
      set = new Set();
      this.eventListeners.set(channel, set);
    }
    set.add(listener as IPCEventListener);
    return () => {
      set!.delete(listener as IPCEventListener);
      if (set!.size === 0) this.eventListeners.delete(channel);
    };
  }

  /** Feed raw wire data into the peer. */
  receive(data: string): void {
    const frames = this.decoder.push(data);
    for (const frame of frames) this.dispatch(frame);
  }

  private dispatch(frame: unknown): void {
    const classified = classifyFrame(frame);
    switch (classified.kind) {
      case "response": {
        this.onResponse(classified.value);
        break;
      }
      case "request": {
        void this.onRequest(classified.value);
        break;
      }
      case "event": {
        this.onEvent(classified.value);
        break;
      }
      default: {
        this.opts.onUnknownFrame?.(frame);
        break;
      }
    }
  }

  private onResponse(res: IPCResponse): void {
    const entry = this.pending.get(res.id);
    if (!entry) return;
    this.pending.delete(res.id);
    clearTimeout(entry.timer);
    if (res.success) {
      entry.resolve(res.payload);
    } else {
      entry.reject(
        new KitronError(
          res.error?.code ?? "IPC_FAILED",
          res.error?.message ?? "IPC call failed",
          res.error?.details,
        ),
      );
    }
  }

  private async onRequest(req: IPCRequest): Promise<void> {
    let response: IPCResponse;
    try {
      const result = await this.registry.invokeLocal(req.channel, req.payload);
      response = { id: req.id, success: true, payload: result ?? null };
    } catch (err) {
      this.opts.onHandlerError?.(req.channel, err);
      const ke = KitronError.fromUnknown(err);
      response = {
        id: req.id,
        success: false,
        error: { code: ke.code, message: ke.message, details: ke.details },
      };
    }
    try {
      this.send(encodeFrame(response));
    } catch {
      // Transport gone — nothing left to do.
    }
  }

  private onEvent(event: IPCEvent): void {
    const set = this.eventListeners.get(event.channel);
    if (!set) return;
    for (const listener of [...set]) {
      try {
        listener(event.payload);
      } catch {
        // Listener errors must not break dispatch.
      }
    }
  }

  pendingCount(): number {
    return this.pending.size;
  }
}

/**
 * Create two peers wired to each other in memory.
 * Useful for unit tests and for the Node ↔ Rust loopback in `kitron dev`
 * when the native host is unavailable.
 */
export function createInMemoryPair(opts: PeerOptions = {}): [IPCPeer, IPCPeer] {
  let a!: IPCPeer;
  let b!: IPCPeer;
  a = new IPCPeer((frame) => b.receive(frame), opts);
  b = new IPCPeer((frame) => a.receive(frame), opts);
  return [a, b];
}
