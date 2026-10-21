/**
 * `@kitron/menu` — native application menus (spec §18).
 *
 * The menu is declared in TypeScript and installed natively; clicking an item
 * dispatches its `action` id back to the app as a `menu.action` event.
 *
 * ```ts
 * menu.setApplicationMenu([
 *   {
 *     label: "File",
 *     items: [
 *       { label: "New", action: "new-file", accelerator: "CmdOrCtrl+N" },
 *       { type: "separator" },
 *       { label: "Quit", action: "app.quit" },
 *     ],
 *   },
 * ]);
 *
 * menu.onAction((action) => {
 *   if (action === "new-file") { /* … *\/ }
 * });
 * ```
 */

import {
  KitronError,
  events,
  getNativeModule,
  throwIfFailed,
} from "@kitron/core";

export interface MenuItemDefinition {
  /** `"separator"` draws a divider. */
  type?: "separator";
  label?: string;
  /** Action id dispatched to JS when the item is clicked. */
  action?: string;
  /** Accelerator, e.g. `"CmdOrCtrl+N"`. */
  accelerator?: string;
  /** Nested items become a submenu. */
  items?: MenuItemDefinition[];
  enabled?: boolean;
  checked?: boolean;
}

export interface MenuDefinition {
  label: string;
  items: MenuItemDefinition[];
}

function requireNative() {
  const native = getNativeModule();
  if (!native) {
    throw new KitronError({
      code: "KITRON_UNAVAILABLE",
      message: "menu APIs require the Kitron native runtime",
    });
  }
  return native;
}

export const menu = {
  /**
   * Installs the application menu. Replaces the menu the runtime installed by
   * default.
   */
  async setApplicationMenu(definition: MenuDefinition[]): Promise<void> {
    if (!Array.isArray(definition)) {
      throw new KitronError({
        code: "KITRON_ARGS_INVALID",
        message: "menu.setApplicationMenu expects an array of menus",
      });
    }
    throwIfFailed(
      await requireNative().menuSetApplicationMenu(JSON.stringify(definition)),
    );
  },

  /**
   * Subscribes to menu clicks. The listener receives the `action` id of the
   * clicked item.
   */
  onAction(listener: (action: string) => void): () => void {
    return events.on("menu.action", (event) => {
      const payload = (event as { payload?: { action?: string } }).payload;
      if (typeof payload?.action === "string") {
        listener(payload.action);
      }
    });
  },
};
