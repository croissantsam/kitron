import { spawn, type ChildProcess } from "node:child_process";

/**
 * Node.js backend process lifecycle — §51 (Node.js runtime embarqué,
 * process lifecycle, crash handling).
 *
 * The backend entry (`src/main.kitron.ts`, run with the project's Node)
 * self-connects over stdio (`KITRON_IPC=stdio`): every stdout line that
 * parses as an IPC frame is routed by the orchestrator, other lines are logs.
 */
export interface NodeBackendOptions {
  entry: string;
  cwd: string;
  env?: Record<string, string>;
  args?: string[];
  /** Executor: defaults to `process.execPath`. */
  executor?: string;
  executorArgs?: string[];
  restartOnCrash?: boolean;
  maxRestarts?: number;
}

export class NodeBackendManager {
  private child: ChildProcess | null = null;
  private restarts = 0;
  private stopped = false;
  private stdoutBuf = "";
  onCrash: ((code: number | null, restarts: number) => void) | null = null;
  /** Raw stdout lines (including non-frame logs). */
  onLine: ((line: string) => void) | null = null;
  onStderr: ((line: string) => void) | null = null;

  constructor(private readonly opts: NodeBackendOptions) {}

  start(): void {
    this.stopped = false;
    this.spawnOnce();
  }

  private spawnOnce(): void {
    const { entry, cwd, env, args, executor, executorArgs } = this.opts;
    const cmd = executor ?? process.execPath;
    const child = spawn(cmd, [...(executorArgs ?? []), entry, ...(args ?? [])], {
      cwd,
      env: { ...process.env, KITRON_IPC: "stdio", ...env },
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.child = child;
    console.log(`[Kitron][Node] Backend started (pid ${child.pid})`);

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
        if (line.trim()) this.onStderr?.(line);
      }
    });

    child.on("exit", (code) => {
      console.log(`[Kitron][Node] Backend exited with code ${code}`);
      this.child = null;
      if (this.stopped) return;
      const shouldRestart = (this.opts.restartOnCrash ?? true) && (code ?? 0) !== 0;
      if (shouldRestart && this.restarts < (this.opts.maxRestarts ?? 3)) {
        this.restarts += 1;
        this.onCrash?.(code, this.restarts);
        console.log(`[Kitron][Node] Restarting backend (attempt ${this.restarts})…`);
        setTimeout(() => this.spawnOnce(), 500).unref?.();
      } else if ((code ?? 0) !== 0) {
        this.onCrash?.(code, this.restarts);
      }
    });
  }

  /** Deliver one NDJSON frame to the backend's stdin. */
  send(frame: string): void {
    try {
      this.child?.stdin?.write(frame.endsWith("\n") ? frame : `${frame}\n`);
    } catch {
      // Child gone — drop.
    }
  }

  get alive(): boolean {
    return this.child !== null;
  }

  stop(): void {
    this.stopped = true;
    this.child?.kill("SIGTERM");
    this.child = null;
  }

  get pid(): number | undefined {
    return this.child?.pid;
  }

  get restartCount(): number {
    return this.restarts;
  }
}
