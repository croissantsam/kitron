import { invoke } from "@kitron/core";
export interface SpawnOptions { command: string; args?: string[]; cwd?: string; env?: Record<string, string>; }
export interface SpawnedProcess { pid: number; }
export async function spawn(opts: SpawnOptions): Promise<SpawnedProcess> { return invoke<SpawnedProcess>("process", "spawn", opts); }
export async function kill(pid: number): Promise<void> { await invoke("process", "kill", { pid }); }
export const process = { spawn, kill };
