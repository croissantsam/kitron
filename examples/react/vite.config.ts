import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { kitron } from "@kitron/vite-plugin";

export default defineConfig({
  plugins: [react(), kitron()],
  server: { port: 5173 },
});
