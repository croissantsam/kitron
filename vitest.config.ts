import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { defineConfig } from "vitest/config";

const root = dirname(fileURLToPath(import.meta.url));
const pkg = (name: string, file = "index.ts") =>
  join(root, "packages", name, "src", file);

export default defineConfig({
  test: {
    include: ["tests/**/*.test.ts"],
    testTimeout: 30_000,
  },
  resolve: {
    alias: [
      { find: "@kitron/types", replacement: pkg("types") },
      { find: "@kitron/ipc", replacement: pkg("ipc") },
      { find: "@kitron/core/node", replacement: pkg("core", "node.ts") },
      { find: "@kitron/core", replacement: pkg("core") },
      { find: "@kitron/runtime", replacement: pkg("runtime") },
      { find: "@kitron/native-api", replacement: pkg("native-api") },
      { find: "@kitron/vite-plugin", replacement: pkg("vite-plugin") },
      { find: "@kitron/cli", replacement: pkg("cli") },
    ],
  },
});
