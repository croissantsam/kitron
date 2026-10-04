import { invoke } from "@kitron/core";
export interface UpdateInfo { version: string; notes?: string; url?: string; }
export async function checkForUpdates(feedUrl: string): Promise<UpdateInfo | null> { return invoke("updater", "check", { feedUrl }); }
/** Verify a payload against an ed25519 `publicKey` (hex) + `signature` (base64). Rejects with UPDATE_SIGNATURE_INVALID. */
export async function verifySignature(payload: string, signature: string, publicKey: string): Promise<{ valid: boolean }> {
  return invoke("updater", "verify", { payload, signature, publicKey });
}
/** Download a payload to the staging dir. Returns staged path + version. */
export async function downloadUpdate(url: string, version?: string): Promise<{ path: string; version: string }> {
  return invoke("updater", "download", { url, version });
}
/**
 * Verify (ed25519) + swap the staged payload over the current binary.
 * Rejects with UPDATE_NOT_STAGED when nothing was downloaded.
 */
export async function installUpdate(opts: { path?: string; signature: string; publicKey: string }): Promise<{ backup: string }> {
  return invoke("updater", "install", opts);
}
export async function installAndRestart(): Promise<void> { await invoke("updater", "installAndRestart"); }
export const updater = { checkForUpdates, verifySignature, downloadUpdate, installUpdate, installAndRestart };
