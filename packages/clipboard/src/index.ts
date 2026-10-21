/**
 * `@kitron/clipboard` — system clipboard (spec §13).
 *
 * macOS uses `NSPasteboard` through the native runtime; the call is gated by
 * the `clipboard` permission declared in `kitron.config.ts` (spec §23).
 */

import {
  KitronError,
  getNativeModule,
  parseEnvelope,
  throwIfFailed,
} from "@kitron/core";

function requireNative() {
  const native = getNativeModule();
  if (!native) {
    throw new KitronError({
      code: "KITRON_UNAVAILABLE",
      message: "clipboard APIs require the Kitron native runtime",
    });
  }
  return native;
}

export const clipboard = {
  /** Reads the clipboard as text (empty string when it holds no text). */
  async read(): Promise<string> {
    const result = parseEnvelope(await requireNative().clipboardRead()) as {
      text?: string;
    };
    return result?.text ?? "";
  },

  /** Writes text to the clipboard, replacing its current content. */
  async write(text: string): Promise<void> {
    if (typeof text !== "string") {
      throw new KitronError({
        code: "KITRON_ARGS_INVALID",
        message: "clipboard.write requires a string",
      });
    }
    throwIfFailed(await requireNative().clipboardWrite(text));
  },
};
