import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { kitron } from "@kitron/vite-plugin";

export default defineConfig({
  plugins: [svelte(), kitron()],
  server: { port: 5173 },
});
