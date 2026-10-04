/** BrowserWindow types — §16. */

export interface BrowserWindowOptions {
  width?: number;
  height?: number;
  minWidth?: number;
  minHeight?: number;
  maxWidth?: number;
  maxHeight?: number;
  x?: number;
  y?: number;
  title?: string;
  /** URL loaded on creation (dev mode). */
  url?: string;
  /** Local file loaded on creation (production). */
  file?: string;
  center?: boolean;
  resizable?: boolean;
  minimizable?: boolean;
  maximizable?: boolean;
  closable?: boolean;
  alwaysOnTop?: boolean;
  fullscreen?: boolean;
  show?: boolean;
  frame?: boolean;
  transparent?: boolean;
  devtools?: boolean;
}

export interface WindowBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface WindowState extends WindowBounds {
  title: string;
  visible: boolean;
  focused: boolean;
  minimized: boolean;
  maximized: boolean;
  fullscreen: boolean;
  alwaysOnTop: boolean;
}

export type WindowEventName =
  | "closed"
  | "focused"
  | "blurred"
  | "moved"
  | "resized"
  | "minimized"
  | "maximized"
  | "restored";

export const DEFAULT_WINDOW_OPTIONS: Required<
  Pick<
    BrowserWindowOptions,
    | "width"
    | "height"
    | "center"
    | "resizable"
    | "minimizable"
    | "maximizable"
    | "closable"
    | "alwaysOnTop"
    | "fullscreen"
    | "show"
    | "frame"
    | "transparent"
    | "devtools"
  >
> = {
  width: 1200,
  height: 800,
  center: true,
  resizable: true,
  minimizable: true,
  maximizable: true,
  closable: true,
  alwaysOnTop: false,
  fullscreen: false,
  show: true,
  frame: true,
  transparent: false,
  devtools: false,
};
