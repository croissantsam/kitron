import { buildPreloadSource } from "@kitron/native-api";

export interface KitronVitePluginOptions {
  /** Expose `window.kitron` via dev bridge (default true in dev). */
  injectPreload?: boolean;
  /** Extra allowed channels logged at startup. */
  debug?: boolean;
}

/**
 * Official Vite plugin — keeps Vite as the bundler/dev server (§24) while
 * injecting the Kitron preload bridge. Framework-independent: works with
 * TanStack Start (default), React, Vue, Svelte, Vanilla.
 */
export function kitron(options: KitronVitePluginOptions = {}): unknown {
  const inject = options.injectPreload ?? true;
  const preload = buildPreloadSource();
  return {
    name: "vite-plugin-kitron",
    transformIndexHtml(html: string) {
      if (!inject) return html;
      const tag = `<script>${preload}</script>`;
      return html.replace(/<head([^>]*)>/i, `<head$1>\n${tag}`);
    },
    configureServer(server: { middlewares: { use: (fn: unknown) => void } }) {
      if (options.debug) {
        console.log("[Kitron][Vite] preload bridge injected");
      }
      server.middlewares.use((req: unknown, res: unknown, next: () => void) => next());
    },
  };
}

export default kitron;
