import { existsSync } from "node:fs";
import { spawn, type ChildProcess } from "node:child_process";
import { dirname, join } from "node:path";

/**
 * Rust host process link — spawns `kitron-host` and speaks NDJSON
 * over stdio (§10: Node.js ↔ Rust). The host logs on stderr so its
 * stdout stays pure NDJSON.
 *
 * When the binary is missing (native not compiled), the caller falls back
 * to the in-process mock host so `kitron dev` still works.
 */
export interface RustHostOptions {
  cwd: string;
  url: string;
  title?: string;
  width?: number;
  height?: number;
  devtools?: boolean;
  /** Explicit binary path. Defaults to upward search for the workspace binary. */
  binaryPath?: string;
  env?: Record<string, string>;
}

/** Walk up from cwd looking for the workspace host binary. */
export function defaultHostBinary(cwd: string): string {
  let dir = cwd;
  for (let i = 0; i < 8; i++) {
    const candidate = join(dir, "native", "rust", "target", "debug", "kitron-host");
    if (existsSync(candidate)) return candidate;
    const parent = dirname(dir);
    if (parent === dir) break;
    dir = parent;
  }
  return join(cwd, "native", "rust", "target", "debug", "kitron-host");
}

export class RustHostManager {
  private child: ChildProcess | null = null;
  private stdoutBuf = "";
  /** Every stdout line (frames and stray logs alike). */
  onLine: ((line: string) => void) | null = null;
  onStderr: ((line: string) => void) | null = null;

  constructor(private readonly opts: RustHostOptions) {}

  binaryPath(): string {
    return this.opts.binaryPath ?? defaultHostBinary(this.opts.cwd);
  }

  binaryExists(): boolean {
    return existsSync(this.binaryPath());
  }

  start(): boolean {
    const bin = this.binaryPath();
    if (!existsSync(bin)) {
      console.log("[Kitron][Rust] Host binary not found — using mock host.");
      return false;
    }
    const child = spawn(
      bin,
      [
        "--url",
        this.opts.url,
        "--title",
        this.opts.title ?? "Kitron App",
        "--width",
        String(this.opts.width ?? 1200),
        "--height",
        String(this.opts.height ?? 800),
        ...(this.opts.devtools ? ["--devtools"] : []),
      ],
      {
        cwd: this.opts.cwd,
        env: { ...process.env, ...this.opts.env },
        stdio: ["pipe", "pipe", "pipe"],
      },
    );
    this.child = child;
    console.log(`[Kitron][Rust] Host started (pid ${child.pid})`);
    child.stdout?.on("data", (d: Buffer) => {
      this.stdoutBuf += String(d);
      let idx: number;
      while ((idx = this.stdoutBuf.indexOf("\n")) >= 0) {
        const line = this.stdoutBuf.slice(0, idx);
        this.stdoutBuf = this.stdoutBuf.slice(idx + 1);
        this.onLine?.(line);
      }
    });
    child.stderr?.on("data", (d: Buffer) => {
      for (const line of String(d).split("\n")) {
        if (line.trim()) {
          if (this.onStderr) this.onStderr(line);
          else console.error(line);
        }
      }
    });
    child.on("exit", (code) => {
      console.log(`[Kitron][Rust] Host exited with code ${code}`);
      this.child = null;
    });
    return true;
  }

  /** Deliver one NDJSON frame to the host's stdin. */
  send(frame: string): void {
    try {
      this.child?.stdin?.write(frame.endsWith("\n") ? frame : `${frame}\n`);
    } catch {
      // Host gone — drop.
    }
  }

  get alive(): boolean {
    return this.child !== null;
  }

  stop(): void {
    this.child?.kill("SIGTERM");
    this.child = null;
  }
}
