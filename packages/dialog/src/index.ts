/**
 * `@kitron/dialog` — native dialogs (spec §12).
 *
 * macOS uses `NSAlert` / `NSOpenPanel` / `NSSavePanel` through the native
 * runtime; the calls are gated by the `dialog` permission declared in
 * `kitron.config.ts` (spec §23). The promise settles once the user dismisses
 * the panel. Without a `windowId` the panel is app-modal; with one it is
 * presented as a sheet attached to that window.
 */

import {
  KitronError,
  getNativeModule,
  parseEnvelope,
} from "@kitron/core";

function requireNative() {
  const native = getNativeModule();
  if (!native) {
    throw new KitronError({
      code: "KITRON_UNAVAILABLE",
      message: "dialog APIs require the Kitron native runtime",
    });
  }
  return native;
}

export type DialogStyle = "info" | "warning" | "critical";

export interface MessageOptions {
  title?: string;
  message?: string;
  detail?: string;
  /** Button labels, in order. Defaults to a single "OK". */
  buttons?: string[];
  style?: DialogStyle;
  /**
   * Present the alert as a **sheet** attached to this window (sliding from
   * its title bar) instead of an app-modal box. An unknown id resolves to
   * `WINDOW_NOT_FOUND`.
   */
  windowId?: number;
}

export interface MessageResult {
  /** Label of the button the user clicked. */
  button: string;
  /** Zero-based index of the clicked button. */
  index: number;
  /** True when the first button was clicked. */
  confirmed: boolean;
}

export interface OpenFileOptions {
  title?: string;
  /** Allow selecting more than one file. */
  multiple?: boolean;
  /** Directory the picker starts in. */
  directory?: string;
  /** Allowed file extensions, without the dot (e.g. `["md", "txt"]`). */
  extensions?: string[];
  /** Present the picker as a sheet attached to this window. */
  windowId?: number;
}

export interface OpenFileResult {
  canceled: boolean;
  /** Absolute paths of the selected files. */
  paths: string[];
}

export interface SaveFileOptions {
  title?: string;
  /** File name pre-filled in the panel. */
  defaultName?: string;
  /** Directory the panel starts in. */
  directory?: string;
  /** Present the panel as a sheet attached to this window. */
  windowId?: number;
}

export interface SaveFileResult {
  canceled: boolean;
  /** Absolute path of the chosen location (null when canceled). */
  path: string | null;
}

export const dialog = {
  /** Shows a native message box. */
  async message(options: MessageOptions = {}): Promise<MessageResult> {
    return parseEnvelope(
      await requireNative().dialogMessage(JSON.stringify(options ?? {})),
    ) as MessageResult;
  },

  /** Opens the native file picker. */
  async openFile(options: OpenFileOptions = {}): Promise<OpenFileResult> {
    return parseEnvelope(
      await requireNative().dialogOpenFile(JSON.stringify(options ?? {})),
    ) as OpenFileResult;
  },

  /** Opens the native save panel. */
  async saveFile(options: SaveFileOptions = {}): Promise<SaveFileResult> {
    return parseEnvelope(
      await requireNative().dialogSaveFile(JSON.stringify(options ?? {})),
    ) as SaveFileResult;
  },
};
