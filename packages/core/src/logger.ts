import type { LogLevel, LogRecord, LogSource } from "@kitron/types";
import { formatLog } from "@kitron/types";
import { getPeer, isTransportConfigured } from "./transport.js";

function emit(level: LogLevel, message: string, details?: unknown, source: LogSource = "node"): void {
  const record: LogRecord = {
    level,
    source,
    message,
    time: new Date().toISOString(),
    details,
  };
  // Always visible locally…
  const line = formatLog(record);
  if (level === "error") console.error(line, details ?? "");
  else if (level === "warn") console.warn(line, details ?? "");
  else console.log(line);
  // …and forwarded to the host when a transport exists (§43).
  if (isTransportConfigured()) {
    try {
      getPeer().emit("log", record);
    } catch {
      // Logging must never crash the app.
    }
  }
}

/** Centralized logger — §43. */
export const logger = {
  debug(message: string, details?: unknown): void {
    emit("debug", message, details);
  },
  info(message: string, details?: unknown): void {
    emit("info", message, details);
  },
  warn(message: string, details?: unknown): void {
    emit("warn", message, details);
  },
  error(message: string, details?: unknown): void {
    emit("error", message, details);
  },
};
