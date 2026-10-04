import { describe, expect, it } from "vitest";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createApp } from "@kitron/cli";
import { runBuild, runPackage } from "@kitron/runtime";

/** Full loop: create → build → package (mock, no Rust binary needed). */
describe("create → build → package", () => {
  it("scaffolds a vanilla app and packages the bundle layout", async () => {
    const root = mkdtempSync(join(tmpdir(), "kitron-it-"));
    try {
      const dir = join(root, "my-app");
      await createApp(dir, "vanilla", "my-app");
      const { outDir } = await runBuild({ cwd: dir });
      expect(outDir).toContain("release");
      const { outDir: bundle } = await runPackage({ cwd: dir });
      expect(bundle).toContain("my-app");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }, 60_000);
});
