/**
 * Native API shapes — §17. Transported as IPC channels
 * (`native.<domain>.<action>`) to the Rust bridge.
 */

export namespace NotificationAPI {
  export interface ShowOptions {
    title: string;
    body: string;
    silent?: boolean;
    tag?: string;
    /** macOS sound name (e.g. "Glass"); ignored elsewhere. */
    sound?: string;
  }
}

export namespace DialogAPI {
  export interface OpenOptions {
    title?: string;
    defaultPath?: string;
    filters?: Array<{ name: string; extensions: string[] }>;
    multiple?: boolean;
    directory?: boolean;
  }
  export interface SaveOptions {
    title?: string;
    defaultPath?: string;
    filters?: Array<{ name: string; extensions: string[] }>;
  }
  export interface MessageOptions {
    title?: string;
    message: string;
    detail?: string;
    buttons?: string[];
    kind?: "info" | "warning" | "error" | "question";
  }
  export interface FileResult {
    canceled: boolean;
    filePaths: string[];
  }
  export interface MessageResult {
    canceled: boolean;
    response: number;
  }
}

export namespace MenuAPI {
  export interface MenuItem {
    id?: string;
    label?: string;
    role?: string;
    accelerator?: string;
    enabled?: boolean;
    visible?: boolean;
    checked?: boolean;
    type?: "normal" | "separator" | "submenu" | "checkbox" | "radio";
    submenu?: MenuItem[];
  }
  export interface MenuTemplate {
    appMenu?: MenuItem[];
  }
}

export namespace TrayAPI {
  export interface TrayOptions {
    title?: string;
    tooltip?: string;
    iconPath?: string;
    menu?: MenuAPI.MenuItem[];
  }
}

export namespace ClipboardAPI {
  export type Format = "text" | "html" | "rtf" | "image";
  /** PNG bytes encoded as base64 (no data: prefix). */
  export interface ImageResult {
    available: boolean;
    pngBase64: string;
  }
}

export namespace ScreenAPI {
  export interface Display {
    id: number;
    label: string;
    bounds: { x: number; y: number; width: number; height: number };
    workArea: { x: number; y: number; width: number; height: number };
    scaleFactor: number;
    primary: boolean;
  }
  export interface CursorPoint {
    x: number;
    y: number;
  }
  /** desktopCapturer source: screen or top-level window. */
  export interface DesktopSource {
    id: string;
    name: string;
    kind: "screen" | "window";
  }
}

export namespace ShortcutAPI {
  export interface Registered {
    accelerator: string;
  }
}

export namespace PowerAPI {
  export type SaveBlockerKind = "prevent-app-suspension" | "prevent-display-sleep";
}
