import type { KitronErrorCode } from "./ipc.js";

/** Typed Kitron error — §44. */
export class KitronError extends Error {
  readonly code: KitronErrorCode;
  readonly details?: unknown;

  constructor(code: KitronErrorCode, message: string, details?: unknown) {
    super(message);
    this.name = "KitronError";
    this.code = code;
    this.details = details;
  }

  toJSON(): { name: string; code: string; message: string; details?: unknown } {
    return {
      name: this.name,
      code: this.code,
      message: this.message,
      details: this.details,
    };
  }

  static fromUnknown(value: unknown): KitronError {
    if (value instanceof KitronError) return value;
    if (value instanceof Error) {
      return new KitronError("NATIVE_API_FAILED", value.message);
    }
    return new KitronError("IPC_FAILED", String(value));
  }
}
