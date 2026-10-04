import type {
  ClipboardAPI,
  DialogAPI,
  MenuAPI,
  NotificationAPI,
  PowerAPI,
  ScreenAPI,
  TrayAPI,
} from "@kitron/types";
import { getPeer } from "./transport.js";
import { requirePermission } from "./transport.js";

function call<T = void>(channel: string, payload?: unknown): Promise<T> {
  return getPeer().invoke<T>(channel, payload);
}

/** Native capabilities — §17. Every call crosses IPC to the Rust bridge. */
export const native = {
  window: {
    setAlwaysOnTop(flag: boolean): Promise<void> {
      return call("native.window.setAlwaysOnTop", { flag });
    },
    setTitle(title: string): Promise<void> {
      return call("native.window.setTitle", { title });
    },
  },

  notification: {
    async show(options: NotificationAPI.ShowOptions): Promise<void> {
      requirePermission("notifications");
      return call("native.notification.show", options);
    },
  },

  dialog: {
    async open(options: DialogAPI.OpenOptions = {}): Promise<DialogAPI.FileResult> {
      requirePermission("dialogs");
      return call("native.dialog.open", options);
    },
    async save(options: DialogAPI.SaveOptions = {}): Promise<DialogAPI.FileResult> {
      requirePermission("dialogs");
      return call("native.dialog.save", options);
    },
    async message(options: DialogAPI.MessageOptions): Promise<DialogAPI.MessageResult> {
      requirePermission("dialogs");
      return call("native.dialog.message", options);
    },
  },

  menu: {
    set(template: MenuAPI.MenuTemplate): Promise<void> {
      return call("native.menu.set", template);
    },
    popup(windowId?: string, x?: number, y?: number): Promise<void> {
      return call("native.menu.popup", { windowId, x, y });
    },
    onClick(listener: (id: string) => void): () => void {
      return getPeer().subscribe<{ id: string }>("native.menu.click", (p) =>
        listener(p.id),
      );
    },
  },

  tray: {
    async create(options: TrayAPI.TrayOptions): Promise<string> {
      requirePermission("tray");
      return call("native.tray.create", options);
    },
    destroy(id: string): Promise<void> {
      return call("native.tray.destroy", { id });
    },
    onClick(listener: (id: string) => void): () => void {
      return getPeer().subscribe("native.tray.click", listener);
    },
  },

  clipboard: {
    readText(): Promise<string> {
      requirePermission("clipboard");
      return call("native.clipboard.readText");
    },
    writeText(text: string): Promise<void> {
      requirePermission("clipboard");
      return call("native.clipboard.writeText", { text });
    },
    write(format: ClipboardAPI.Format, text: string): Promise<void> {
      requirePermission("clipboard");
      return call("native.clipboard.writeFormat", { format, text });
    },
    async read(format: ClipboardAPI.Format): Promise<string> {
      requirePermission("clipboard");
      if (format === "image") {
        const res = await call<ClipboardAPI.ImageResult>("native.clipboard.readImage");
        return res.pngBase64;
      }
      return call("native.clipboard.read", { format });
    },
    readImage(): Promise<ClipboardAPI.ImageResult> {
      requirePermission("clipboard");
      return call("native.clipboard.readImage");
    },
    writeImage(pngBase64: string): Promise<void> {
      requirePermission("clipboard");
      return call("native.clipboard.writeImage", { pngBase64 });
    },
    clear(): Promise<void> {
      requirePermission("clipboard");
      return call("native.clipboard.clear");
    },
  },

  screen: {
    getDisplays(): Promise<ScreenAPI.Display[]> {
      return call("native.screen.getDisplays");
    },
    getCursor(): Promise<ScreenAPI.CursorPoint> {
      return call("native.screen.getCursor");
    },
  },

  desktop: {
    getSources(
      types: Array<"screen" | "window"> = ["screen", "window"],
    ): Promise<ScreenAPI.DesktopSource[]> {
      return call("native.desktop.getSources", { types });
    },
  },

  shortcut: {
    async register(accelerator: string): Promise<void> {
      requirePermission("globalShortcuts");
      return call("native.shortcut.register", { accelerator });
    },
    unregister(accelerator: string): Promise<void> {
      return call("native.shortcut.unregister", { accelerator });
    },
    onTrigger(listener: (accelerator: string) => void): () => void {
      return getPeer().subscribe("native.shortcut.trigger", listener);
    },
  },

  power: {
    async saveBlockerStart(kind: PowerAPI.SaveBlockerKind): Promise<number> {
      requirePermission("power");
      return call("native.power.saveBlockerStart", { kind });
    },
    saveBlockerStop(id: number): Promise<void> {
      return call("native.power.saveBlockerStop", { id });
    },
  },
};
