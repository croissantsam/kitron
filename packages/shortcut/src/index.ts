import { invoke, onNativeEvent } from "@kitron/core";
export async function register(accelerator: string, handler?: () => void): Promise<void> {
  if (handler) {
    onNativeEvent(`shortcut.${accelerator}`, () => handler());
  }
  await invoke("shortcut", "register", { accelerator });
}
export async function unregister(accelerator: string): Promise<void> { await invoke("shortcut", "unregister", { accelerator }); }
export async function list(): Promise<string[]> { return invoke("shortcut", "list"); }
export const shortcut = { register, unregister, list };
