/**
 * Desktop integration — Tranche A.
 * Native look (dark mode), power state, single instance, custom protocols.
 * Implemented in Node.js where the OS exposes a CLI/sysctl surface (Rule 2);
 * the rest crosses IPC to the Rust host.
 */

export type ThemeSource = "system" | "light" | "dark";

export interface ThemeInfo {
  source: ThemeSource;
  shouldUseDarkColors: boolean;
}

export type PowerIdleState = "active" | "idle" | "locked" | "unknown";

export interface SecondInstanceInfo {
  argv: string[];
  workingDirectory: string;
}

export type PowerMonitorEvent =
  | "suspend"
  | "resume"
  | "on-battery"
  | "on-ac";

export interface ProtocolRegistration {
  protocol: string;
  registered: boolean;
}
