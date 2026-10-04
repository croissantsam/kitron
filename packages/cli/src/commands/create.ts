import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

export type TemplateName = "tanstack-start" | "react" | "vue" | "svelte" | "vanilla";

export const TEMPLATES: TemplateName[] = ["tanstack-start", "react", "vue", "svelte", "vanilla"];

function pkg(name: string, template: TemplateName, kitronTag = "workspace:*"): string {
  const base = {
    name,
    version: "0.1.0",
    private: true,
    type: "module",
    scripts: {
      dev: "kitron dev",
      build: "kitron build",
      package: "kitron package",
    },
    dependencies: {},
    devDependencies: {},
  } as Record<string, unknown>;
  if (template === "react" || template === "tanstack-start") {
    (base["dependencies"] as object) = { react: "^18.3.1", "react-dom": "^18.3.1" };
    (base["devDependencies"] as object) = {
      vite: "^6.0.0",
      "@vitejs/plugin-react": "^4.3.0",
      typescript: "^5.6.0",
    };
  } else if (template === "vue") {
    (base["dependencies"] as object) = { vue: "^3.5.0" };
    (base["devDependencies"] as object) = {
      vite: "^6.0.0",
      "@vitejs/plugin-vue": "^5.1.0",
      typescript: "^5.6.0",
    };
  } else if (template === "svelte") {
    (base["dependencies"] as object) = { svelte: "^5.0.0" };
    (base["devDependencies"] as object) = {
      vite: "^6.0.0",
      "@sveltejs/vite-plugin-svelte": "^4.0.0",
      typescript: "^5.6.0",
    };
  } else {
    (base["devDependencies"] as object) = { vite: "^6.0.0", typescript: "^5.6.0" };
  }
  if (template === "tanstack-start") {
    Object.assign(base["devDependencies"] as object, {
      "@tanstack/react-start": "^1.0.0",
      "@tanstack/react-router": "^1.0.0",
    });
  }
  // Kitron runtime deps: workspace link in-repo, npm range for `create-kitron`.
  Object.assign(base["devDependencies"] as object, {
    "@kitron/cli": kitronTag,
    "@kitron/core": kitronTag,
    "@kitron/types": kitronTag,
    "@kitron/vite-plugin": kitronTag,
  });
  return JSON.stringify(base, null, 2) + "\n";
}

function viteConfig(template: TemplateName): string {
  const plugin =
    template === "react" || template === "tanstack-start"
      ? `import react from "@vitejs/plugin-react";\n`
      : template === "vue"
        ? `import vue from "@vitejs/plugin-vue";\n`
        : template === "svelte"
          ? `import { svelte } from "@sveltejs/vite-plugin-svelte";\n`
          : "";
  const use =
    template === "react" || template === "tanstack-start"
      ? "[react(), kitron()]"
      : template === "vue"
        ? "[vue(), kitron()]"
        : template === "svelte"
          ? "[svelte(), kitron()]"
          : "[kitron()]";
  return `import { defineConfig } from "vite";\n${plugin}import { kitron } from "@kitron/vite-plugin";\n\nexport default defineConfig({\n  plugins: ${use},\n  server: { port: 5173 },\n});\n`;
}

function indexHtml(name: string, entry: string): string {
  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>${name}</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="${entry}"></script>
  </body>
</html>
`;
}

function rendererMain(template: TemplateName): string {
  switch (template) {
    case "react":
      return `import React, { useEffect, useState } from "react";
import { createRoot } from "react-dom/client";

function App() {
  const [pong, setPong] = useState("…");
  useEffect(() => {
    window.kitron?.invoke<string>("ping").then(setPong).catch(() => setPong("offline"));
  }, []);
  return (
    <main style={{ fontFamily: "system-ui", padding: 32 }}>
      <h1>Hello Kitron</h1>
      <p>ping → {pong}</p>
      <button
        onClick={() =>
          window.kitron?.invoke("native.notification.show", { title: "Kitron", body: "Hello" })
        }
      >
        Notify
      </button>
    </main>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
`;
    case "vue":
      return `<script setup lang="ts">
import { onMounted, ref } from "vue";
const pong = ref("…");
onMounted(async () => {
  try {
    pong.value = await window.kitron!.invoke<string>("ping");
  } catch {
    pong.value = "offline";
  }
});
</script>

<template>
  <main style="font-family: system-ui; padding: 32px">
    <h1>Hello Kitron</h1>
    <p>ping → {{ pong }}</p>
  </main>
</template>
`;
    case "svelte":
      return `<script lang="ts">
  import { onMount } from "svelte";
  let pong = "…";
  onMount(async () => {
    try {
      pong = await window.kitron!.invoke("ping");
    } catch {
      pong = "offline";
    }
  });
</script>

<main style="font-family: system-ui; padding: 32px">
  <h1>Hello Kitron</h1>
  <p>ping → {pong}</p>
</main>
`;
    case "tanstack-start":
      return `import React, { useEffect, useState } from "react";

export default function Home() {
  const [pong, setPong] = useState("…");
  useEffect(() => {
    window.kitron?.invoke<string>("ping").then(setPong).catch(() => setPong("offline"));
  }, []);
  return (
    <main style={{ fontFamily: "system-ui", padding: 32 }}>
      <h1>Hello Kitron</h1>
      <p>TanStack Start route — ping → {pong}</p>
    </main>
  );
}
`;
    default:
      return `async function main() {
  const pong = await window.kitron?.invoke("ping").catch(() => "offline");

  document.querySelector("#root")!.innerHTML = \`
    <main style="font-family: system-ui; padding: 32px">
      <h1>Hello Kitron</h1>
      <p>ping → \${pong}</p>
      <button id="notify">Notify</button>
    </main>
  \`;

  document.querySelector("#notify")!.addEventListener("click", () => {
    window.kitron?.invoke("native.notification.show", { title: "Kitron", body: "Hello" });
  });
}

void main();
`;
  }
}

function backendMain(productName: string): string {
  return `import { app, BrowserWindow, native } from "@kitron/core";

await app.ready();

const win = new BrowserWindow({
  width: 1200,
  height: 800,
  title: ${JSON.stringify(productName)},
});

await win.loadURL(process.env["KITRON_DEV_URL"] ?? "http://127.0.0.1:5173");

await native.notification.show({
  title: ${JSON.stringify(productName)},
  body: "Application started",
});
`;
}

/** Scaffold a new Kitron app (§32) — TanStack Start default, others on request. */
export interface CreateAppOptions {
  /** Dependency tag for @kitron/* (default "workspace:*" in-repo). */
  kitronTag?: string;
}

export async function createApp(
  dir: string,
  template: TemplateName,
  appName: string,
  opts: CreateAppOptions = {},
): Promise<void> {
  if (existsSync(dir)) {
    throw new Error(`Directory "${dir}" already exists`);
  }
  mkdirSync(join(dir, "src"), { recursive: true });
  mkdirSync(join(dir, "public"), { recursive: true });

  writeFileSync(join(dir, "package.json"), pkg(appName, template, opts.kitronTag));
  const entry =
    template === "react" || template === "tanstack-start" ? "/src/main.tsx" : "/src/main.ts";
  writeFileSync(join(dir, "index.html"), indexHtml(appName, entry));
  writeFileSync(join(dir, "vite.config.ts"), viteConfig(template));
  writeFileSync(
    join(dir, "kitron.config.ts"),
    `import { defineConfig } from "@kitron/types";\n\nexport default defineConfig({\n  permissions: {\n    notifications: true,\n    globalShortcuts: false,\n  },\n});\n`,
  );
  writeFileSync(
    join(dir, "tsconfig.json"),
    JSON.stringify(
      { compilerOptions: { target: "ES2022", module: "ESNext", strict: true, jsx: "react-jsx" } },
      null,
      2,
    ) + "\n",
  );
  const mainFile = template === "vue" ? "App.vue" : template === "svelte" ? "App.svelte" : "main.tsx";
  writeFileSync(
    join(dir, template === "vanilla" ? "src/main.ts" : `src/${mainFile}`),
    rendererMain(template),
  );
  if (template === "vue") {
    writeFileSync(
      join(dir, "src/main.ts"),
      `import { createApp } from "vue";\nimport App from "./App.vue";\n\ncreateApp(App).mount("#root");\n`,
    );
  }
  if (template === "svelte") {
    writeFileSync(
      join(dir, "src/main.ts"),
      `import App from "./App.svelte";\n\nnew App({ target: document.getElementById("root")! });\n`,
    );
  }
  if (template === "tanstack-start") {
    // TanStack Start route (default export) + plain Vite mount entry.
    writeFileSync(join(dir, "src/index.tsx"), rendererMain("tanstack-start"));
    writeFileSync(
      join(dir, "src/main.tsx"),
      `import React from "react";\nimport { createRoot } from "react-dom/client";\nimport Home from "./index";\n\ncreateRoot(document.getElementById("root")!).render(<Home />);\n`,
    );
  }
  writeFileSync(join(dir, "src/main.kitron.ts"), backendMain(appName));
}
