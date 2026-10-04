import { defineConfig } from "vite";
import { kitron } from "@kitron/vite-plugin";

export default defineConfig({
  plugins: [kitron()],
  server: { port: 5173 },
});
