/**
 * @kitron/core — application lifecycle (§11).
 *
 * macOS and Windows lifecycles differ (activate vs. quit-on-last-window);
 * that divergence lives in Rust. This module is the typed TS facade.
 */

import { invoke, onNativeEvent } from "./bridge.js";

export type AppEvent =
  | "ready"
  | "activate"
  | "beforeQuit"
  | "quit"
  | "deepLink"
  | "fileOpen";

export function on(event: AppEvent, listener: (payload?: unknown) => void): () => void {
  if (event === "deepLink") return onNativeEvent("app.deepLink", listener);
  if (event === "fileOpen") return onNativeEvent("app.fileOpen", listener);
  return onNativeEvent(`app.${event}`, listener);
}

export async function ready(): Promise<void> {
  await invoke("app", "ready");
}

export async function quit(): Promise<void> {
  await invoke("app", "quit");
}

export async function hide(): Promise<void> {
  await invoke("app", "hide");
}

export async function show(): Promise<void> {
  await invoke("app", "show");
}

/**
 * Test/OS hooks (§20-§21): inject a deep link or opened file as if the OS
 * delivered it. Listeners registered via `on("deepLink")` / `on("fileOpen")`
 * fire through the normal native-event path.
 */
export async function emitDeepLink(url: string): Promise<void> {
  await invoke("app", "deepLink", { url });
}

export async function emitFileOpen(path: string): Promise<void> {
  await invoke("app", "fileOpen", { path });
}

export const app = {
  on,
  ready,
  quit,
  hide,
  show,
  emitDeepLink,
  emitFileOpen,
};
