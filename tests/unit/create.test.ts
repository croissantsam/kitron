import { describe, expect, it } from "vitest";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createApp } from "@kitron/cli";

describe("create-kitron starter tags", () => {
  it("emits workspace:* by default (in-repo `kitron create`)", async () => {
    const root = mkdtempSync(join(tmpdir(), "kitron-tag-"));
    try {
      const dir = join(root, "a");
      await createApp(dir, "vanilla", "a");
      const pkg = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
      expect(pkg.devDependencies["@kitron/cli"]).toBe("workspace:*");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });

  it("emits npm ranges with kitronTag (npx create-kitron)", async () => {
    const root = mkdtempSync(join(tmpdir(), "kitron-tag-"));
    try {
      const dir = join(root, "b");
      await createApp(dir, "react", "b", { kitronTag: "^0.1.0" });
      const raw = readFileSync(join(dir, "package.json"), "utf8");
      expect(raw).not.toContain("workspace:");
      const pkg = JSON.parse(raw);
      expect(pkg.devDependencies["@kitron/cli"]).toBe("^0.1.0");
      expect(pkg.devDependencies["@kitron/core"]).toBe("^0.1.0");
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
});
