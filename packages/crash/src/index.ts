import { invoke } from "@kitron/core";

export interface CrashReport {
  id: string;
  time: string;
  app_version: string;
  platform: string;
  message: string;
  location?: string | null;
}

/** On-device crash reports, newest first. Empty when no crashes recorded. */
export async function list(): Promise<CrashReport[]> {
  return invoke("crash", "list");
}

export async function get(id: string): Promise<CrashReport> {
  return invoke("crash", "get", { id });
}

/** Delete all stored reports. Returns the number removed. */
export async function clear(): Promise<{ cleared: number }> {
  return invoke("crash", "clear");
}

export const crash = { list, get, clear };
