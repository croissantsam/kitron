/**
 * `@kitron/tray` — system tray / status-bar items (spec §42).
 *
 * macOS uses `NSStatusBar`. Gated by the `tray` permission (spec §23).
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
      message: "tray APIs require the Kitron native runtime",
    });
  }
  return native;
}

export interface TrayMenuDefinition {
  label: string;
  items: { label?: string; type?: "separator"; action?: string; accelerator?: string; enabled?: boolean; checked?: boolean; items?: TrayMenuDefinition["items"] }[];
}

export interface TrayCreateOptions {
  title?: string;
  tooltip?: string;
  menu?: TrayMenuDefinition[];
}

export const tray = {
  /** Creates a status-bar item. Returns its id. */
  async create(options: TrayCreateOptions = {}): Promise<number> {
    const result = parseEnvelope(
      await requireNative().trayCreate(JSON.stringify(options ?? {})),
    ) as { id: number };
    return result.id;
  },

  /** Updates the item's title. */
  async setTitle(id: number, title: string): Promise<void> {
    await requireNative().traySetTitle(id, title);
  },

  /** Removes the item. */
  async remove(id: number): Promise<void> {
    await requireNative().trayRemove(id);
  },
};
