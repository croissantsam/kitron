/**
 * @kitron/window (§10) — minimal multi-window API.
 *
 * Backed by Rust `kitron-runtime::windows` (winit → AppKit on macOS,
 * Win32 on Windows). All calls are async; never block the UI thread (§32).
 */
import { invoke, onNativeEvent } from "@kitron/core";

export interface WindowCreateOptions {
  width?: number;
  height?: number;
  minWidth?: number;
  minHeight?: number;
  title?: string;
  /** logical pixels */
  x?: number;
  y?: number;
  center?: boolean;
  resizable?: boolean;
  decorations?: boolean;
  visible?: boolean;
}

export interface KitronWindowInfo {
  id: string;
  title: string;
  width: number;
  height: number;
  x: number;
  y: number;
  minimized: boolean;
  maximized: boolean;
  visible: boolean;
  focused: boolean;
}

export type WindowEvent = "closed" | "focused" | "blurred" | "moved" | "resized";

export class KitronWindow {
  readonly id: string;

  constructor(id: string) {
    this.id = id;
  }

  static async create(opts: WindowCreateOptions = {}): Promise<KitronWindow> {
    const result = await invoke<{ id: string }>("window", "create", {
      width: opts.width ?? 1200,
      height: opts.height ?? 800,
      title: opts.title ?? "Kitron App",
      x: opts.x,
      y: opts.y,
      center: opts.center ?? true,
      minWidth: opts.minWidth,
      minHeight: opts.minHeight,
      resizable: opts.resizable ?? true,
      decorations: opts.decorations ?? true,
      visible: opts.visible ?? true,
    });
    return new KitronWindow(result.id);
  }

  async get(): Promise<KitronWindowInfo> {
    return invoke<KitronWindowInfo>("window", "get", { id: this.id });
  }
  async close(): Promise<void> {
    await invoke("window", "close", { id: this.id });
  }
  async minimize(): Promise<void> {
    await invoke("window", "minimize", { id: this.id });
  }
  async maximize(): Promise<void> {
    await invoke("window", "maximize", { id: this.id });
  }
  async restore(): Promise<void> {
    await invoke("window", "restore", { id: this.id });
  }
  async focus(): Promise<void> {
    await invoke("window", "focus", { id: this.id });
  }
  async show(): Promise<void> {
    await invoke("window", "show", { id: this.id });
  }
  async hide(): Promise<void> {
    await invoke("window", "hide", { id: this.id });
  }
  async setSize(width: number, height: number): Promise<void> {
    await invoke("window", "setSize", { id: this.id, width, height });
  }
  async setPosition(x: number, y: number): Promise<void> {
    await invoke("window", "setPosition", { id: this.id, x, y });
  }
  async setTitle(title: string): Promise<void> {
    await invoke("window", "setTitle", { id: this.id, title });
  }

  on(event: WindowEvent, listener: (info: KitronWindowInfo) => void): () => void {
    return onNativeEvent(`window.${event}`, (payload) => {
      const p = payload as { id?: string } | undefined;
      if (p && typeof p.id === "string" && p.id !== this.id) return;
      listener(payload as KitronWindowInfo);
    });
  }
}

/** Functional facade matching the spec examples (§10, §41). */
export const window = {
  create: (opts?: WindowCreateOptions) => KitronWindow.create(opts),
  get: (id: string) => new KitronWindow(id).get(),
  list: () => invoke<KitronWindowInfo[]>("window", "list"),
  close: (id: string) => new KitronWindow(id).close(),
  minimize: (id: string) => new KitronWindow(id).minimize(),
  maximize: (id: string) => new KitronWindow(id).maximize(),
  restore: (id: string) => new KitronWindow(id).restore(),
  focus: (id: string) => new KitronWindow(id).focus(),
  show: (id: string) => new KitronWindow(id).show(),
  hide: (id: string) => new KitronWindow(id).hide(),
  setSize: (id: string, w: number, h: number) => new KitronWindow(id).setSize(w, h),
  setPosition: (id: string, x: number, y: number) => new KitronWindow(id).setPosition(x, y),
};

export { KitronWindow as Window };
