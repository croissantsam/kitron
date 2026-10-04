import { defineConfig } from "@kitron/types";

export default defineConfig({
  permissions: {
    notifications: true,
    globalShortcuts: false,
  },
  dev: { port: 5173 },
  build: {
    productName: "Kitron Vanilla",
    appId: "com.kitron.vanilla",
  },
});
