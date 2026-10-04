/** Electron-style `shell` — Tranche A. Implemented in Node.js (Rule 2). */

export interface OpenExternalOptions {
  activate?: boolean;
  workingDirectory?: string;
}

export interface ShellTrashItemResult {
  trashed: boolean;
}
