/**
 * Typed Kitron error model (spec §33).
 *
 * Rust generates `{ code, message, platform }`; the TS layer rehydrates these
 * into `KitronError` so app code can do stable reconciliation:
 *
 * ```ts
 * try {
 *   await window.open({ width: 800, height: 600 });
 * } catch (error) {
 *   if (error instanceof KitronError && error.code === "WINDOW_CREATION_FAILED") {
 *     // handle
 *   }
 * }
 * ```
 */
export type PlatformName = "macos" | "windows" | "kitron";

export interface KitronErrorShape {
  code: string;
  message: string;
  platform?: PlatformName | string;
  details?: unknown;
}

export const KITRON_ERROR = Symbol("kitron-error");

export class KitronError extends Error {
  readonly code: string;
  readonly platform: string;
  readonly details?: unknown;

  constructor(shape: KitronErrorShape) {
    super(shape.message);
    this.name = "KitronError";
    this.code = shape.code;
    this.platform = shape.platform ?? "kitron";
    this.details = shape.details;
  }

  /** Rehydrates an envelope error coming back from the native bridge. */
  static from(shape: unknown): KitronError {
    if (shape instanceof KitronError) {
      return shape;
    }
    if (typeof shape === "object" && shape !== null) {
      const candidate = shape as Record<string, unknown>;
      if (typeof candidate.code === "string" && typeof candidate.message === "string") {
        return new KitronError({
          code: candidate.code,
          message: candidate.message,
          platform:
            typeof candidate.platform === "string" ? candidate.platform : "kitron",
          details: candidate.details,
        });
      }
    }
    return new KitronError({
      code: "KITRON_UNKNOWN_ERROR",
      message: String(shape),
      platform: "kitron",
    });
  }

  /** True when this error carries the given stable code. */
  is(code: string): boolean {
    return this.code === code;
  }

  toJSON(): KitronErrorShape {
    return {
      code: this.code,
      message: this.message,
      platform: this.platform,
      ...(this.details !== undefined ? { details: this.details } : {}),
    };
  }
}

/**
 * Native replying with a `{ ok: false, error: {...} }` envelope.
 *
 * The native module returns the envelope as a JSON *string*; both shapes are
 * accepted so callers can pass the raw reply straight through.
 */
export function throwIfFailed(envelope: unknown): void {
  let value: unknown = envelope;
  if (typeof value === "string") {
    try {
      value = JSON.parse(value);
    } catch {
      // Not an envelope — nothing to unwrap.
      return;
    }
  }
  if (
    typeof value === "object" &&
    value !== null &&
    "ok" in value &&
    (value as { ok: boolean }).ok === false
  ) {
    throw KitronError.from(
      (value as unknown as { error?: unknown }).error,
    );
  }
}
