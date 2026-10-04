/**
 * Backend-side handler for the Kitron Node sidecar protocol.
 *
 * Imported by `backend/index.ts` (the child process), NOT by app UI code:
 *
 * ```ts
 * import { defineBackend } from "@kitron/node/backend";
 *
 * defineBackend({
 *   "users.get": async (params: { id: number }) => ({ id: params.id }),
 * });
 * ```
 *
 * Plain `console.log` lines are ignored by the Rust reader (non-JSON lines
 * are skipped); prefer `console.error` for backend logs so they surface in
 * the host's stderr.
 */
export type BackendProcedure<TParams = unknown, TResult = unknown> = (
  params: TParams,
) => TResult | Promise<TResult>;

export function defineBackend(procedures: Record<string, BackendProcedure>): void {
  let buffer = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk: string) => {
    buffer += chunk;
    let idx: number;
    while ((idx = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, idx).trim();
      buffer = buffer.slice(idx + 1);
      if (line) void handleLine(line, procedures);
    }
  });
}

async function handleLine(line: string, procedures: Record<string, BackendProcedure>): Promise<void> {
  let msg: { id?: unknown; procedure?: unknown; params?: unknown };
  try {
    msg = JSON.parse(line) as typeof msg;
  } catch {
    return;
  }
  if (typeof msg.id !== "number" || typeof msg.procedure !== "string") return;
  const { id, procedure } = msg;
  const fn = procedures[procedure];
  if (!fn) {
    respond({ id, ok: false, error: { code: "UNKNOWN_PROCEDURE", message: `unknown procedure: ${procedure}` } });
    return;
  }
  try {
    const result = await fn(msg.params);
    respond({ id, ok: true, result: result ?? null });
  } catch (e) {
    respond({
      id,
      ok: false,
      error: {
        code: (e as { code?: string })?.code ?? "BACKEND_PROCEDURE_FAILED",
        message: e instanceof Error ? e.message : String(e),
      },
    });
  }
}

function respond(frame: unknown): void {
  process.stdout.write(`${JSON.stringify(frame)}\n`);
}
