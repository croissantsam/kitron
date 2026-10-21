/**
 * Phase-2 SDK namespaces (spec §42) — reserved by the runtime, implemented
 * after the MVP gate (spec §41).
 *
 * They live in their own `@kitron/*` packages (spec §24); this module
 * re-exports them from the umbrella package so apps can import everything
 * from `kitron`.
 */

export { dialog } from "@kitron/dialog";
export { clipboard } from "@kitron/clipboard";
export { menu } from "@kitron/menu";
export { tray } from "@kitron/tray";
export { notification } from "@kitron/notification";
export { process } from "@kitron/process";
export { shell } from "@kitron/shell";
export { shortcut } from "@kitron/shortcut";
export { updater } from "@kitron/updater";
