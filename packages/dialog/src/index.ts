import { invoke } from "@kitron/core";

export interface OpenFileOptions { multiple?: boolean; filters?: { name: string; extensions: string[] }[]; defaultPath?: string; }
export interface SaveFileOptions { defaultPath?: string; filters?: { name: string; extensions: string[] }[]; }
export interface MessageOptions { title?: string; message: string; kind?: "info" | "warning" | "error" | "confirmation"; buttons?: string[]; }

/** Native file-open dialog. Returns null when cancelled. */
export async function openFile(opts: OpenFileOptions = {}): Promise<string[] | null> {
  try {
    return await invoke<string[] | null>("dialog", "openFile", opts);
  } catch (e: unknown) {
    const err = e as { code?: string };
    if (err?.code === "DIALOG_CANCELLED") return null;
    throw e;
  }
}
export async function saveFile(opts: SaveFileOptions = {}): Promise<string | null> {
  try {
    return await invoke<string | null>("dialog", "saveFile", opts);
  } catch (e: unknown) {
    if ((e as { code?: string })?.code === "DIALOG_CANCELLED") return null;
    throw e;
  }
}
export async function selectDirectory(): Promise<string | null> {
  try {
    return await invoke<string | null>("dialog", "selectDirectory", {});
  } catch (e: unknown) {
    if ((e as { code?: string })?.code === "DIALOG_CANCELLED") return null;
    throw e;
  }
}
export async function showMessage(opts: MessageOptions): Promise<{ button: number }> {
  return invoke("dialog", "message", opts);
}
export const dialog = { openFile, saveFile, selectDirectory, showMessage };
