import type { IPCEvent, IPCRequest, IPCResponse } from "@kitron/types";
import { isIPCEvent, isIPCRequest, isIPCResponse } from "@kitron/types";
import { classifyFrame } from "./codec.js";

/** Try to parse a stdio line as an IPC frame. Returns null for log lines. */
export function parseFrameLine(line: string): IPCRequest | IPCResponse | IPCEvent | null {
  const trimmed = line.trim();
  if (!trimmed.startsWith("{")) return null;
  try {
    const value: unknown = JSON.parse(trimmed);
    const c = classifyFrame(value);
    if (c.kind === "unknown") return null;
    return c.value as IPCRequest | IPCResponse | IPCEvent;
  } catch {
    return null;
  }
}

export function isIPCRequestFrame(v: unknown): v is IPCRequest {
  return isIPCRequest(v);
}
export function isIPCResponseFrame(v: unknown): v is IPCResponse {
  return isIPCResponse(v);
}
export function isIPCEventFrame(v: unknown): v is IPCEvent {
  return isIPCEvent(v);
}

/**
 * Frame-level router between IPC participants (§10).
 *
 * Participants are uniformly `{ name, deliver }`: in-process peers
 * (`peer.receive`) or subprocess stdio links (`child.stdin.write`).
 * The hub owns no handlers itself — it routes:
 * - request → owner link by channel (host patterns vs default link),
 * - response → origin link by pending id,
 * - event → broadcast to every other link.
 */
export interface HubLink {
  name: string;
  deliver: (frame: string) => void;
}

const HOST_PATTERN = /^(ping|window\.|webcontents\.|native\.|updater\.|log$)/;

/** `app.*` channels owned by the native host; every other `app.*` channel
 * (single-instance, protocols…) is served by the Node.js backend. */
const HOST_APP_CHANNELS: ReadonlySet<string> = new Set([
  "app.ready",
  "app.isReady",
  "app.getName",
  "app.getVersion",
  "app.getInfo",
  "app.getPath",
  "app.quit",
  "app.exit",
]);

export class IpcHub {
  private links = new Map<string, HubLink>();
  private pending = new Map<string, string>();
  private hostLink: string | null = null;
  private defaultLink: string | null = null;

  addLink(link: HubLink, opts: { host?: boolean; isDefault?: boolean } = {}): void {
    this.links.set(link.name, link);
    if (opts.host) this.hostLink = link.name;
    if (opts.isDefault) this.defaultLink = link.name;
  }

  removeLink(name: string): void {
    this.links.delete(name);
    this.pending.forEach((origin, id) => {
      if (origin === name) this.pending.delete(id);
    });
    if (this.hostLink === name) this.hostLink = null;
    if (this.defaultLink === name) this.defaultLink = null;
  }

  /** Feed one raw line coming FROM `origin` into the router. */
  routeFrom(origin: string, line: string): void {
    const frame = parseFrameLine(line);
    if (!frame) return;
    const text = line.endsWith("\n") ? line : `${line}\n`;

    if (isIPCResponse(frame)) {
      const dest = this.pending.get(frame.id);
      if (dest) {
        this.pending.delete(frame.id);
        this.links.get(dest)?.deliver(text);
      }
      return;
    }
    if (isIPCRequest(frame)) {
      const dest = this.ownerOf(frame.channel) ?? this.defaultLink;
      if (dest && dest !== origin) {
        this.pending.set(frame.id, origin);
        this.links.get(dest)?.deliver(text);
      } else if (dest === origin) {
        // Owner is the sender itself (in-process peer answering its own
        // calls is unsupported) — reply with a typed error.
        this.links.get(origin)?.deliver(
          `${JSON.stringify({
            id: frame.id,
            success: false,
            error: { code: "HANDLER_NOT_FOUND", message: `No remote handler for "${frame.channel}"` },
          })}\n`,
        );
      } else {
        this.links.get(origin)?.deliver(
          `${JSON.stringify({
            id: frame.id,
            success: false,
            error: { code: "HANDLER_NOT_FOUND", message: `No handler for channel "${frame.channel}"` },
          })}\n`,
        );
      }
      return;
    }
    // Event → broadcast.
    for (const [name, link] of this.links) {
      if (name !== origin) {
        try {
          link.deliver(text);
        } catch {
          // A dead link must not break broadcast.
        }
      }
    }
  }

  private ownerOf(channel: string): string | null {
    if (channel.startsWith("app.")) {
      return HOST_APP_CHANNELS.has(channel) ? this.hostLink : this.defaultLink;
    }
    if (HOST_PATTERN.test(channel)) return this.hostLink;
    return this.defaultLink;
  }

  linkNames(): string[] {
    return [...this.links.keys()];
  }

  pendingCount(): number {
    return this.pending.size;
  }
}
