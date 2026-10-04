import { invoke } from "@kitron/core";
export async function openExternal(url: string): Promise<void> { await invoke("shell", "openExternal", { url }); }
export async function openPath(path: string): Promise<void> { await invoke("shell", "openPath", { path }); }
export async function showItemInFolder(path: string): Promise<void> { await invoke("shell", "showItemInFolder", { path }); }
export const shell = { openExternal, openPath, showItemInFolder };
