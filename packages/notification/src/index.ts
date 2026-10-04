import { invoke } from "@kitron/core";
export interface NotificationOptions { title: string; body: string; silent?: boolean; }
export async function show(opts: NotificationOptions): Promise<void> { await invoke("notification", "show", opts); }
export const notification = { show };
