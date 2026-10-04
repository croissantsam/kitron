import { invoke } from "@kitron/core";
export async function writeText(text: string): Promise<void> { await invoke("clipboard", "writeText", { text }); }
export async function readText(): Promise<string> { return invoke<string>("clipboard", "readText"); }
export const clipboard = { writeText, readText };
