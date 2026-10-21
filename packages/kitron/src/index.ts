/**
 * Kitron SDK entry (spec §9): `import { app, window, backend } from "kitron"`.
 *
 * Phase-1 namespaces are real (`app`, `window`, `backend`/`node`, `logger`),
 * plus the first phase-2 batch (`clipboard`, `dialog`, `shell`). The rest of
 * the spec'd surface ships as typed stubs that fail with `KITRON_PHASE2` —
 * honest signals inside the MVP gate (spec §41).
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
