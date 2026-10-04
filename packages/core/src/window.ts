import { randomUUID } from "node:crypto";
import type { BrowserWindowOptions, WindowEventName } from "@kitron/types";
import { DEFAULT_WINDOW_OPTIONS } from "@kitron/types";
import { getPeer } from "./transport.js";
import { WebContents } from "./webcontents.js";

/**
 * BrowserWindow — §16.
 *
 * The instance lives in the Node backend; every method is an IPC call
 * forwarded to the Rust host which owns the native window + WebView.
 */
export class BrowserWindow {
  readonly id: string;
  private readonly options: BrowserWindowOptions;

  /** `webContents` equivalent — Tranche B. */
  readonly webContents: WebContents;

  constructor(options: BrowserWindowOptions = {}) {
    this.id = randomUUID();
    this.options = { ...DEFAULT_WINDOW_OPTIONS, ...options };
    this.webContents = new WebContents(this.id);
    // Fire-and-forget creation is wrong: surface errors via ready promise.
    this.created = getPeer()
      .invoke("window.create", { id: this.id, options: this.options })
      .then(() => undefined);
  }

  private created: Promise<void>;

  /** Resolves once the Rust host acknowledged creation. */
  get ready(): Promise<void> {
    return this.created;
  }

  private call<T = void>(action: string, payload?: unknown): Promise<T> {
    return this.created.then(() =>
      getPeer().invoke<T>(`window.${action}`, { id: this.id, ...(payload as object) }),
    );
  }

  loadURL(url: string): Promise<void> {
    return this.call("loadURL", { url });
  }

  loadFile(path: string): Promise<void> {
    return this.call("loadFile", { path });
  }

  show(): Promise<void> {
    return this.call("show");
  }

  hide(): Promise<void> {
    return this.call("hide");
  }

  close(): Promise<void> {
    return this.call("close");
  }

  focus(): Promise<void> {
    return this.call("focus");
  }

  minimize(): Promise<void> {
    return this.call("minimize");
  }

  maximize(): Promise<void> {
    return this.call("maximize");
  }

  restore(): Promise<void> {
    return this.call("restore");
  }

  reload(): Promise<void> {
    return this.call("reload");
  }

  openDevTools(): Promise<void> {
    return this.call("openDevTools");
  }

  closeDevTools(): Promise<void> {
    return this.call("closeDevTools");
  }

  setTitle(title: string): Promise<void> {
    return this.call("setTitle", { title });
  }

  setSize(width: number, height: number): Promise<void> {
    return this.call("setSize", { width, height });
  }

  setPosition(x: number, y: number): Promise<void> {
    return this.call("setPosition", { x, y });
  }

  setAlwaysOnTop(flag: boolean): Promise<void> {
    return this.call("setAlwaysOnTop", { flag });
  }

  setFullscreen(flag: boolean): Promise<void> {
    return this.call("setFullscreen", { flag });
  }

  isFullscreen(): Promise<boolean> {
    return this.call<boolean>("isFullscreen");
  }

  setMinimumSize(width: number, height: number): Promise<void> {
    return this.call("setMinimumSize", { width, height });
  }

  setMaximumSize(width: number, height: number): Promise<void> {
    return this.call("setMaximumSize", { width, height });
  }

  isMaximized(): Promise<boolean> {
    return this.call<boolean>("isMaximized");
  }

  on(event: WindowEventName, listener: (payload: unknown) => void): () => void {
    return getPeer().subscribe(`window.${this.id}.${event}`, listener);
  }
}
