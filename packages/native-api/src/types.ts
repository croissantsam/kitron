/**
 * Renderer-side `window.kitron` surface — §20.
 *
 * The renderer NEVER gets `require`, `process`, `fs` or raw Node access.
 * Only `invoke` (request/response) and `subscribe`/`emit` on whitelisted
 * channels, plus namespaced `native` helpers.
 */
export interface KitronRendererAPI {
  invoke<T = unknown>(channel: string, payload?: unknown, opts?: { timeout?: number }): Promise<T>;
  emit(channel: string, payload?: unknown): void;
  subscribe<T = unknown>(channel: string, listener: (payload: T) => void): () => void;
  native: {
    notification: {
      show(options: { title: string; body: string; silent?: boolean; tag?: string }): Promise<void>;
    };
    window: {
      setAlwaysOnTop(flag: boolean): Promise<void>;
      setTitle(title: string): Promise<void>;
    };
    dialog: {
      open(options?: Record<string, unknown>): Promise<{ canceled: boolean; filePaths: string[] }>;
      save(options?: Record<string, unknown>): Promise<{ canceled: boolean; filePaths: string[] }>;
    };
    clipboard: {
      readText(): Promise<string>;
      writeText(text: string): Promise<void>;
    };
  };
}

declare global {
  interface Window {
    kitron?: KitronRendererAPI;
  }
}

export {};
