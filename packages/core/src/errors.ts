/**
 * @kitron/core — typed errors.
 *
 * Rule (§33): never throw bare `Error("Something went wrong")`.
 * Always throw `KitronError` with a stable `code`, human `message`,
 * and originating `platform` when known.
 */

export type KitronPlatform = "macos" | "windows" | "linux" | "unknown";

export function currentPlatform(): KitronPlatform {
  if (typeof process !== "undefined") {
    if (process.platform === "darwin") return "macos";
    if (process.platform === "win32") return "windows";
    if (process.platform === "linux") return "linux";
  }
  return "unknown";
}

export type KitronErrorCode =
  // runtime / bridge
  | "KITRON_RUNTIME_UNAVAILABLE"
  | "KITRON_IPC_TIMEOUT"
  | "KITRON_IPC_TRANSPORT_CLOSED"
  | "KITRON_INVALID_PARAMS"
  | "KITRON_NOT_IMPLEMENTED"
  // window
  | "WINDOW_CREATION_FAILED"
  | "WINDOW_NOT_FOUND"
  | "WINDOW_OPERATION_FAILED"
  // app
  | "APP_LIFECYCLE_FAILED"
  // dialog
  | "DIALOG_FAILED"
  | "DIALOG_CANCELLED"
  // clipboard
  | "CLIPBOARD_READ_FAILED"
  | "CLIPBOARD_WRITE_FAILED"
  // notification
  | "NOTIFICATION_FAILED"
  | "NOTIFICATION_PERMISSION_DENIED"
  // shell
  | "SHELL_OPEN_FAILED"
  // process
  | "PROCESS_SPAWN_FAILED"
  | "PROCESS_NOT_FOUND"
  // tray
  | "TRAY_CREATION_FAILED"
  // menu
  | "MENU_CREATION_FAILED"
  // shortcut
  | "SHORTCUT_REGISTER_FAILED"
  // updater
  | "UPDATE_CHECK_FAILED"
  | "UPDATE_SIGNATURE_INVALID"
  // node backend
  | "NODE_START_FAILED"
  | "NODE_STOP_FAILED"
  | "BACKEND_INVOKE_FAILED";

export interface KitronErrorDetails {
  code: KitronErrorCode;
  message: string;
  platform?: KitronPlatform;
  /** Machine-readable extra context (native os error, errno, etc.). */
  details?: unknown;
}

export class KitronError extends Error {
  readonly code: KitronErrorCode;
  readonly platform: KitronPlatform;
  readonly details?: unknown;

  constructor(init: KitronErrorDetails) {
    super(init.message);
    this.name = "KitronError";
    this.code = init.code;
    this.platform = init.platform ?? currentPlatform();
    this.details = init.details;
  }

  toJSON(): KitronErrorDetails & { name: string } {
    return {
      name: this.name,
      code: this.code,
      message: this.message,
      platform: this.platform,
      details: this.details,
    };
  }

  static is(e: unknown): e is KitronError {
    return e instanceof KitronError;
  }

  /** Rehydrate an error received over IPC (Rust → JS). */
  static fromJSON(json: unknown): KitronError {
    if (typeof json === "object" && json !== null) {
      const o = json as Record<string, unknown>;
      if (typeof o["code"] === "string" && typeof o["message"] === "string") {
        return new KitronError({
          code: o["code"] as KitronErrorCode,
          message: o["message"] as string,
          platform: (o["platform"] as KitronPlatform) ?? currentPlatform(),
          details: o["details"],
        });
      }
    }
    return new KitronError({
      code: "KITRON_IPC_TRANSPORT_CLOSED",
      message: String(json),
    });
  }
}
