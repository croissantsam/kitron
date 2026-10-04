import { describe, expect, it } from "vitest";
import { existsSync } from "node:fs";
import { join } from "node:path";

/**
 * E2E: TanStack Start → Kitron → Node.js → Rust → native OS (§49).
 * Requires a compiled host binary + display; skipped otherwise.
 */
describe("e2e (native host)", () => {
  const bin = join(process.cwd(), "native/rust/target/debug/kitron-host");
  const hasBinary = existsSync(bin);

  it.skipIf(!hasBinary)("boots the host and answers ping over stdio", async () => {
    const { spawn } = await import("node:child_process");
    const child = spawn(bin, ["--url", "http://127.0.0.1:5173"], {
      stdio: ["pipe", "pipe", "ignore"],
    });
    const frame = JSON.stringify({ id: "e2e-1", channel: "ping", payload: null }) + "\n";
    const res = await new Promise<string>((resolve, reject) => {
      let buf = "";
      const timer = setTimeout(() => reject(new Error("timeout")), 10_000);
      child.stdout.on("data", (d: Buffer) => {
        buf += String(d);
        const idx = buf.indexOf("\n");
        if (idx >= 0) {
          clearTimeout(timer);
          resolve(buf.slice(0, idx));
        }
      });
      child.stdin.write(frame);
    });
    child.kill();
    const parsed = JSON.parse(res);
    expect(parsed.success).toBe(true);
    expect(parsed.payload).toBe("pong");
  });
});
