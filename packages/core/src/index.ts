/** @kitron/core — app lifecycle, events, typed errors, shared bridge (spec §40). */

export { KitronError, throwIfFailed } from "./error.js";
export type { KitronErrorShape, PlatformName } from "./error.js";
export { app, watchAppReady } from "./app.js";
export type { AppInfo } from "./app.js";
export { events, KitronEventEmitter } from "./events.js";
export type { KitronEvent, KitronEventName } from "./events.js";
export { createLogger, logger } from "./logger.js";
export type { Logger, LogLevel } from "./logger.js";
export type {
  KitronAppConfig,
  KitronConfig,
  KitronNodeConfig,
  KitronWindowConfig,
} from "./config-types.js";
export {
  getJsiHost,
  getNativeModule,
  isKitronAppProcess,
  parseEnvelope,
} from "./bridge.js";
export type { KitronJsiHost, KitronNativeModule } from "./bridge.js";
