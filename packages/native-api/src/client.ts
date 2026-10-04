import { IPCPeer } from "@kitron/ipc";
import type { KitronRendererAPI } from "./types.js";
import { isChannelAllowed } from "./preload.js";

/**
 * Framework-independent renderer client — §25.
 * Works with TanStack Start (default), React, Vue, Svelte or Vanilla:
 * `await kitron.invoke("ping")`.
 */
export function createKitronClient(send: (frame: string) => void): KitronRendererAPI & {
  peer: IPCPeer;
  receive: (data: string) => void;
} {
  const peer = new IPCPeer(send);

  const guard = (channel: string) => {
    if (!isChannelAllowed(channel)) {
      throw Object.assign(new Error(`Channel "${channel}" is not allowed`), {
        code: "PERMISSION_DENIED",
      });
    }
  };

  return {
    peer,
    receive: (data: string) => peer.receive(data),
    invoke: async <T = unknown>(channel: string, payload?: unknown, opts?: { timeout?: number }) => {
      guard(channel);
      return peer.invoke<T>(channel, payload, opts);
    },
    emit: (channel: string, payload?: unknown) => {
      guard(channel);
      peer.emit(channel, payload);
    },
    subscribe: <T = unknown>(channel: string, listener: (payload: T) => void) => {
      guard(channel);
      return peer.subscribe(channel, listener);
    },
    native: {
      notification: {
        show: (options) => {
          guard("native.notification.show");
          return peer.invoke("native.notification.show", options);
        },
      },
      window: {
        setAlwaysOnTop: (flag) => peer.invoke("native.window.setAlwaysOnTop", { flag }),
        setTitle: (title) => peer.invoke("native.window.setTitle", { title }),
      },
      dialog: {
        open: (options) => peer.invoke("native.dialog.open", options ?? {}),
        save: (options) => peer.invoke("native.dialog.save", options ?? {}),
      },
      clipboard: {
        readText: () => peer.invoke("native.clipboard.readText"),
        writeText: (text: string) => peer.invoke("native.clipboard.writeText", { text }),
      },
    },
  };
}
