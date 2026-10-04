import { defineConfig } from "@kitron/core/config";

export default defineConfig({
  app: {
    name: "My App",
    identifier: "com.example.myapp",
    version: "1.0.0",
  },

  window: {
    width: 1200,
    height: 800,
  },

  node: {
    entry: "./backend/index.ts",
  },

  // Sensitive capabilities are denied unless listed here (§23).
  // Available: "shell", "process".
  permissions: [],
});
