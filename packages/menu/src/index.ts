import { invoke, onNativeEvent } from "@kitron/core";
export interface MenuItem { label?: string; action?: string; checked?: boolean; enabled?: boolean; items?: MenuItem[]; separator?: boolean; }
export async function create(items: MenuItem[]): Promise<{ id: string }> { return invoke("menu", "create", { items }); }
export async function setAppMenu(id: string): Promise<void> { await invoke("menu", "setAppMenu", { id }); }
/** Fires when a menu item with `action` is activated (native menu → JS). */
export function onAction(listener: (e: { menuId: string; action: string }) => void): () => void {
  return onNativeEvent("menu.action", (p) => listener(p as { menuId: string; action: string }));
}
export const Menu = { create, setAppMenu, onAction };
export const menu = { create, setAppMenu, onAction };
