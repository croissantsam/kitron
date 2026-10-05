import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(here, "..");
const bin = path.join(repoRoot, "runtime/rust/target/debug/kitron-runtime");

// Integration: real binary over a Unix socket (the `kitron dev` UI path).
// Skipped when the binary isn't built (e.g. `node --test` without cargo).
const haveBin = fs.existsSync(bin);

describe("socket transport (integration)", { skip: !haveBin }, () => {
  it("window.create + ui render + layout through the real binary", async () => {
    const core = await import("@kitron/core");
    const sock = path.join(os.tmpdir(), `kitron-test-${process.pid}.sock`);
    try {
      fs.unlinkSync(sock);
    } catch {
      // fresh
    }
    const proc = spawn(bin, ["run", "--headless", "--socket", sock], { stdio: "ignore" });
    try {
      // Wait for the runtime to bind (spawn + tokio startup).
      const deadline = Date.now() + 15_000;
      for (;;) {
        try {
          fs.statSync(sock);
          break;
        } catch {
          if (Date.now() > deadline) throw new Error("socket never appeared");
          await new Promise((r) => setTimeout(r, 100));
        }
      }
      const t = new core.SocketTransport({ path: sock, timeoutMs: 15_000 });
      await t.ready;
      core.setBridgeTransport(t);
      const { window } = await import("@kitron/window");
      const w = await window.create({ width: 800, height: 600, title: "sock-test" });
      assert.ok(typeof w.id === "string" && w.id.length > 0);
      assert.deepEqual((await window.list()).map((x) => x.id), [w.id]);

      // Minimal UI tree through the same socket the UI runner uses.
      await core.invoke("ui", "create", { id: "s-root", kind: "view", props: {}, style: { flex: 1 } });
      await core.invoke("ui", "create", { id: "s-t", kind: "text", props: { text: "hi" }, style: {} });
      await core.invoke("ui", "append", { parent: "s-root", child: "s-t" });
      await core.invoke("ui", "setRoot", { window: w.id, root: "s-root" });
      const placed = await core.invoke("ui", "layout", { window: w.id, width: 800, height: 600 });
      assert.ok(placed.some((n) => n.text === "hi"));

      // Unknown domain still typed; permissions still enforced.
      await assert.rejects(() => core.invoke("nope", "nope", {}), (e) => e.code === "KITRON_NOT_IMPLEMENTED");
      await assert.rejects(
        () => core.invoke("shell", "openExternal", { url: "https://example.com" }),
        (e) => e.code === "PERMISSION_DENIED",
      );
      t.close();
    } finally {
      proc.kill("SIGKILL");
      core.setBridgeTransport(null);
      try {
        fs.unlinkSync(sock);
      } catch {
        // gone
      }
    }
  });
});
