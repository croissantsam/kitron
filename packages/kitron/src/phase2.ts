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

export { tray } from "@kitron/tray";
export type { TrayCreateOptions, TrayMenuDefinition } from "@kitron/tray";
export { notification } from "@kitron/notification";
export type { NotificationOptions } from "@kitron/notification";
export { process } from "@kitron/process";
export type { SpawnOptions, SpawnResult } from "@kitron/process";
export { shortcut } from "@kitron/shortcut";
export { updater } from "@kitron/updater";
export type { UpdateInfo } from "@kitron/updater";
