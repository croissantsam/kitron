/**
 * Crash reporting, auto-update and safe storage — continued Electron parity.
 * crashReporter/safeStorage are Node-first; updater check/download is Node,
 * platform install stays OS-specific (documented per target).
 */

export interface CrashReport {
  id: string;
  time: string;
  kind: "node-uncaught" | "node-unhandled" | "rust-panic";
  message: string;
  stack?: string;
  platform: string;
}

export interface UpdateFeed {
  url: string;
  currentVersion: string;
}

export interface FeedVersion {
  version: string;
  notes?: string;
  url?: string;
}

export type UpdaterEvent =
  | "checking-for-update"
  | "update-available"
  | "update-not-available"
  | "update-downloaded"
  | "error";
