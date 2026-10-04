import { invoke, onNativeEvent } from "@kitron/core";
export interface TrayOptions { icon: string; tooltip?: string; }
export class Tray {
  constructor(readonly id: string) {}
  static async create(opts: TrayOptions): Promise<Tray> {
    const r = await invoke<{ id: string }>("tray", "create", opts);
    return new Tray(r.id);
  }
  async destroy(): Promise<void> { await invoke("tray", "destroy", { id: this.id }); }
  onClick(listener: () => void): () => void {
    return onNativeEvent(`tray.clicked`, (p) => {
      if ((p as { id?: string })?.id === this.id) listener();
    });
  }
}
export async function list(): Promise<{ id: string; tooltip?: string }[]> {
  return invoke("tray", "list");
}
export const tray = { create: Tray.create, list };
