/**
 * @kitron/core — centralized logger (§34).
 *
 * Centralizes:
 * - React Native / JS logs
 * - Node backend logs
 * - Rust native logs (forwarded over IPC as `kitron.log` events)
 */

export type LogLevel = "debug" | "info" | "warn" | "error";

export interface LogEntry {
  level: LogLevel;
  scope: string;
  message: string;
  time: string;
  data?: unknown;
}

export type LogSink = (entry: LogEntry) => void;

const sinks = new Set<LogSink>();
let minLevel: LogLevel = "info";
let defaultSinkInstalled = false;

const order: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
};

function emit(level: LogLevel, scope: string, message: string, data?: unknown): void {
  if (order[level] < order[minLevel]) return;
  const entry: LogEntry = {
    level,
    scope,
    message,
    time: new Date().toISOString(),
    data,
  };
  if (sinks.size === 0) {
    installDefaultSink();
  }
  for (const s of sinks) {
    try {
      s(entry);
    } catch {
      // never let logging break the app
    }
  }
}

function installDefaultSink(): void {
  if (defaultSinkInstalled) return;
  defaultSinkInstalled = true;
  const sink: LogSink = (e) => {
    const line = `[${e.time}] [${e.level}] [${e.scope}] ${e.message}`;
    if (e.level === "error") console.error(line, e.data ?? "");
    else if (e.level === "warn") console.warn(line, e.data ?? "");
    else console.log(line, e.data ?? "");
  };
  sinks.add(sink);
}

export interface Logger {
  debug(message: string, data?: unknown): void;
  info(message: string, data?: unknown): void;
  warn(message: string, data?: unknown): void;
  error(message: string, data?: unknown): void;
  child(childScope: string): Logger;
}

function makeLogger(scope: string): Logger {
  return {
    debug: (m, d) => emit("debug", scope, m, d),
    info: (m, d) => emit("info", scope, m, d),
    warn: (m, d) => emit("warn", scope, m, d),
    error: (m, d) => emit("error", scope, m, d),
    child: (c) => makeLogger(`${scope}:${c}`),
  };
}

export const logger: Logger = makeLogger("kitron");

export function getLogger(scope: string): Logger {
  return makeLogger(scope);
}

/** Ingest a log line forwarded from Rust (writes through the same sinks). */
export function ingestNativeLog(entry: LogEntry): void {
  if (sinks.size === 0) installDefaultSink();
  for (const s of sinks) {
    try {
      s({ ...entry, scope: `rust:${entry.scope}` });
    } catch {
      // ignore
    }
  }
}

export function addLogSink(sink: LogSink): () => void {
  installDefaultSink();
  sinks.add(sink);
  return () => {
    sinks.delete(sink);
  };
}

export function setLogLevel(level: LogLevel): void {
  minLevel = level;
}
