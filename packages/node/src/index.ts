/**
 * @kitron/node (§28-§30) — embedded Node backend lifecycle + RPC.
 * Avoid forcing everything through HTTP localhost; prefer Kitron IPC.
 */
import { invoke } from "@kitron/core";
export interface NodeStartOptions { entry: string; args?: string[]; env?: Record<string, string>; }
export type NodeStatus = "stopped" | "starting" | "running" | "failed";
export async function start(opts: NodeStartOptions): Promise<void> { await invoke("node", "start", opts); }
export async function stop(): Promise<void> { await invoke("node", "stop"); }
export async function restart(opts?: NodeStartOptions): Promise<void> { await invoke("node", "restart", opts ?? {}); }
export async function status(): Promise<{ status: NodeStatus; pid?: number }> { return invoke("node", "status"); }
/** Invoke a backend procedure: backend.invoke("users.get", { id }) */
export async function invokeBackend<T = unknown>(procedure: string, params?: unknown): Promise<T> {
  return invoke<T>("backend", "invoke", { procedure, params });
}
export const node = { start, stop, restart, status };
export const backend = { invoke: invokeBackend };
