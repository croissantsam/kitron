/**
 * Phase-2 SDK namespaces (spec §42).
 *
 * `clipboard`, `dialog`, `shell` and `menu` are implemented natively on macOS
 * (`NSPasteboard`, `NSAlert`/`NSOpenPanel`/`NSSavePanel`, `NSWorkspace`,
 * `NSMenu`). The remaining ones (tray, notification, process, shortcut,
 * updater) are reserved by the runtime but land after the MVP gate (spec §41)
 * — they throw a typed `KITRON_PHASE2` error so apps can feature-detect.
 */

export { dialog } from "@kitron/dialog";
export type {
  DialogStyle,
  MessageOptions,
  MessageResult,
  OpenFileOptions,
  OpenFileResult,
  SaveFileOptions,
  SaveFileResult,
} from "@kitron/dialog";
export { clipboard } from "@kitron/clipboard";
export { shell } from "@kitron/shell";
export { menu } from "@kitron/menu";
export type { MenuDefinition, MenuItemDefinition } from "@kitron/menu";

import { KitronError } from "@kitron/core";

function phase2(feature: string): never {
  throw new KitronError({
    code: "KITRON_PHASE2",
    message:
      `${feature} ships after the MVP gate (spec §41). ` +
      "Track progress on the Kitron roadmap; feature-detect with try/catch.",
  });
}

export const tray = {
  create: (_options: unknown) => phase2("tray.create"),
};

export const notification = {
  show: (_options: unknown) => phase2("notification.show"),
};

export const process = {
  env: (_name: string) => phase2("process.env"),
  spawn: (_options: unknown) => phase2("process.spawn"),
};

export const shortcut = {
  register: (_accelerator: string, _handler: () => void) =>
    phase2("shortcut.register"),
  unregister: (_accelerator: string) => phase2("shortcut.unregister"),
};

export const updater = {
  check: () => phase2("updater.check"),
  download: () => phase2("updater.download"),
};
