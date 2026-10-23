/**
 * Kitron SDK entry (spec §9): `import { app, window, backend } from "kitron"`.
 *
 * Phase-1 namespaces are real (`app`, `window`, `backend`/`node`, `logger`),
 * plus the full phase-2 batch (`clipboard`, `dialog`, `shell`, `menu`, `tray`,
 * `notification`, `process`, `shortcut`, `updater`) backed by native code.
 */

export { app } from "@kitron/core";
export type { AppInfo } from "@kitron/core";
export { window } from "@kitron/window";
export type {
  WindowCreateOptions,
  WindowFrame,
  WindowId,
  WindowSnapshot,
} from "@kitron/window";
export { backend } from "@kitron/node";
export type { NodeStatusInfo } from "@kitron/node";
export { KitronError, logger } from "@kitron/core";
export type { KitronConfig, Logger } from "@kitron/core";
export * from "./phase2.js";
