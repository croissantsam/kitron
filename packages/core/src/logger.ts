/**
 * Structured in-app logger (spec §34 — JS side).
 *
 * Inside a Kitron app, messages flow through the `__kitron.log` JSI host
 * function into the unified NSLog stream (`[kitron:app]` prefix, stdout +
 * system log). Outside Kitron (Storybook-ish plain RN usage) they fall back to
 * console. The `log.*` helpers keep messages greppable with a stable
 * `[kitron:tag]` prefix; native + Node logs already funnel through the same
 * shape on the Rust side.
 */

export type LogLevel = "debug" | "info" | "warn" | "error";

type NativeKitronLog = (level: string, message: string) => void;

interface KitronGlobalLike {
  log?: NativeKitronLog;
}

function nativeLog(): NativeKitronLog | undefined {
  try {
    const kitron = (globalThis as { __kitron?: KitronGlobalLike }).__kitron;
    if (typeof kitron?.log === "function") {
      return kitron.log;
    }
  } catch {
    // The JSI host is absent (plain RN usage) — console fallback below.
  }
  return undefined;
}

/** React Native's dev flag (injected by Metro at build time). */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
declare const __DEV__: boolean | undefined;

export interface Logger {
  debug(message: string, ...context: unknown[]): void;
  info(message: string, ...context: unknown[]): void;
  warn(message: string, ...context: unknown[]): void;
  error(message: string, ...context: unknown[]): void;
  tag(tag: string): Logger;
}

const CONSOLE_FOR_LEVEL: Record<LogLevel, (message: string) => void> = {
  // NOTE: console methods return undefined; prefer-with-fallback must not use
  // `?.() ??` — that always falls through and double-writes every message.
  debug: (message) => (console.debug ?? console.log)(message),
  info: (message) => (console.info ?? console.log)(message),
  warn: (message) => console.warn(message),
  error: (message) => console.error(message),
};

function formatContext(context: unknown[]): string {
  if (context.length === 0) {
    return "";
  }
  try {
    const rendered = context
      .map((value) => (typeof value === "string" ? value : JSON.stringify(value)))
      .join(" ");
    return ` ${rendered}`;
  } catch {
    return "";
  }
}

export function createLogger(tag = "app"): Logger {
  const prefix = `[kitron:${tag}]`;
  const write = (level: LogLevel, message: string, context: unknown[]): void => {
    const line = `${prefix} ${message}${formatContext(context)}`;

    // Inside a Kitron app every level mirrors into the unified native log so
    // the CLI stdout sees JS output; warn/error additionally go to console so
    // the RN devtools/RedBox still surface them.
    const native = nativeLog();
    if (native) {
      try {
        native(level, line);
        if (level === "error") {
          CONSOLE_FOR_LEVEL.error(line);
        } else if (level === "warn") {
          CONSOLE_FOR_LEVEL.warn(line);
        }
        return;
      } catch {
        // Fall through to console when the host function misbehaves.
      }
    }
    if (level === "error") {
      CONSOLE_FOR_LEVEL.error(line);
      return;
    }
    if (level === "warn") {
      CONSOLE_FOR_LEVEL.warn(line);
      return;
    }
    const isDevish =
      typeof __DEV__ !== "undefined"
        ? __DEV__ === true
        : typeof process !== "undefined"
          ? process.env?.NODE_ENV !== "production"
          : true;
    if (level === "debug" && !isDevish) {
      return;
    }
    CONSOLE_FOR_LEVEL[level](line);
  };

  return {
    debug: (message, ...context) => write("debug", message, context),
    info: (message, ...context) => write("info", message, context),
    warn: (message, ...context) => write("warn", message, context),
    error: (message, ...context) => write("error", message, context),
    tag: (nestedTag) => createLogger(`${tag}:${nestedTag}`),
  };
}

export const logger = createLogger();
