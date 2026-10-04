import type { IPCEvent, IPCRequest, IPCResponse } from "@kitron/types";
import { isIPCEvent, isIPCRequest, isIPCResponse } from "@kitron/types";

/**
 * NDJSON codec: one JSON object per line (`\n` delimited).
 * Used on stdio pipes between Node.js and the Rust host, and over the
 * WebView bridge (string messages).
 */
export class NDJSONDecoder {
  private buffer = "";

  /** Push a chunk, return all complete frames decoded. */
  push(chunk: string): unknown[] {
    this.buffer += chunk;
    const out: unknown[] = [];
    let idx: number;
    while ((idx = this.buffer.indexOf("\n")) >= 0) {
      const line = this.buffer.slice(0, idx).trim();
      this.buffer = this.buffer.slice(idx + 1);
      if (line.length === 0) continue;
      out.push(JSON.parse(line));
    }
    return out;
  }

  reset(): void {
    this.buffer = "";
  }
}

export function encodeFrame(value: IPCRequest | IPCResponse | IPCEvent): string {
  return `${JSON.stringify(value)}\n`;
}

export type DecodedFrame =
  | { kind: "request"; value: IPCRequest }
  | { kind: "response"; value: IPCResponse }
  | { kind: "event"; value: IPCEvent }
  | { kind: "unknown"; value: unknown };

export function classifyFrame(value: unknown): DecodedFrame {
  if (isIPCResponse(value)) return { kind: "response", value };
  if (isIPCRequest(value)) return { kind: "request", value };
  if (isIPCEvent(value)) return { kind: "event", value };
  return { kind: "unknown", value };
}
