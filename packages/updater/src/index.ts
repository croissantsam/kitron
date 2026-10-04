import { invoke } from "@kitron/core";
export interface UpdateInfo { version: string; notes?: string; url?: string; }
export async function checkForUpdates(feedUrl: string): Promise<UpdateInfo | null> { return invoke("updater", "check", { feedUrl }); }
/** Verify a payload against an ed25519 `publicKey` (hex) + `signature` (base64). Rejects with UPDATE_SIGNATURE_INVALID. */
export async function verifySignature(payload: string, signature: string, publicKey: string): Promise<{ valid: boolean }> {
  return invoke("updater", "verify", { payload, signature, publicKey });
}
export async function installAndRestart(): Promise<void> { await invoke("updater", "installAndRestart"); }
export const updater = { checkForUpdates, verifySignature, installAndRestart };
