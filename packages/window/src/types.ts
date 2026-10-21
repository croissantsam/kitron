/** Window shapes surfaced across the bridge (mirror of the Rust `WindowSnapshot`). */

export type WindowId = number;

/** Frame in top-left screen coordinates (designer-friendly; spec §38). */
export interface WindowFrame {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface WindowSnapshot {
  id: WindowId;
  title: string;
  frame: WindowFrame;
  focused: boolean;
  minimized: boolean;
  maximized: boolean;
  visible: boolean;
}

export interface WindowCreateOptions {
  title?: string;
  width?: number;
  height?: number;
  focus?: boolean;
  resizable?: boolean;
  minimizable?: boolean;
  closable?: boolean;
}

export type WindowEventName =
  | "window.created"
  | "window.closed"
  | "window.focus"
  | "window.blur"
  | "window.resize"
  | "window.move"
  | "window.minimize"
  | "window.restore";

export type WindowEvent =
  | { name: "window.created"; windowId: WindowId; payload: { id: WindowId; title: string; width: number; height: number } }
  | { name: WindowEventName; windowId: WindowId; payload: { id: WindowId } };

export interface SetSizeArgs {
  width: number;
  height: number;
}

export interface SetPositionArgs {
  x: number;
  y: number;
}

/** Guard for narrowing kitron event payloads. */
export function isWindowEvent(event: { name: string; windowId?: number | null }): boolean {
  return event.name.startsWith("window.");
}
