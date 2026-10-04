import type { IPCHandler } from "@kitron/types";
import { KitronError } from "@kitron/types";

/** Registry of local request handlers (`ipc.handle`). */
export class HandlerRegistry {
  private handlers = new Map<string, IPCHandler<never, unknown>>();

  handle<TPayload = unknown, TResult = unknown>(
    channel: string,
    handler: IPCHandler<TPayload, TResult>,
  ): () => void {
    if (!channel || typeof channel !== "string") {
      throw new KitronError("INVALID_CHANNEL", "Channel must be a non-empty string");
    }
    if (this.handlers.has(channel)) {
      throw new KitronError(
        "INVALID_CHANNEL",
        `Handler already registered for channel "${channel}"`,
      );
    }
    this.handlers.set(channel, handler as IPCHandler<never, unknown>);
    return () => {
      this.handlers.delete(channel);
    };
  }

  remove(channel: string): void {
    this.handlers.delete(channel);
  }

  has(channel: string): boolean {
    return this.handlers.has(channel);
  }

  channels(): string[] {
    return [...this.handlers.keys()];
  }

  async invokeLocal(channel: string, payload: unknown): Promise<unknown> {
    const handler = this.handlers.get(channel);
    if (!handler) {
      throw new KitronError(
        "HANDLER_NOT_FOUND",
        `No handler registered for channel "${channel}"`,
      );
    }
    return handler(payload as never);
  }
}
