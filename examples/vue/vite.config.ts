import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { kitron } from "@kitron/vite-plugin";

export default defineConfig({
  plugins: [vue(), kitron()],
  server: { port: 5173 },
});
