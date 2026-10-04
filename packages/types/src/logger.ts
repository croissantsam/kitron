/** Centralized log levels — §43. */
export type LogLevel = "debug" | "info" | "warn" | "error";

export type LogSource = "renderer" | "node" | "rust" | "cli";

export interface LogRecord {
  level: LogLevel;
  source: LogSource;
  message: string;
  time: string;
  details?: unknown;
}

export function formatLog(record: LogRecord): string {
  const source =
    record.source.charAt(0).toUpperCase() + record.source.slice(1);
  return `[Kitron][${source}] ${record.message}`;
}
