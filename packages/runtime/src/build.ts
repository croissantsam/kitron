import { existsSync, mkdirSync, writeFileSync, cpSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { loadConfig } from "./config.js";

export interface BuildOptions {
  cwd?: string;
}

/**
 * `kitron build` pipeline — §28:
 * TanStack Start → Vite build → web assets + node backend manifest.
 */
export async function runBuild(opts: BuildOptions = {}): Promise<{ outDir: string }> {
  const cwd = opts.cwd ?? process.cwd();
  const config = await loadConfig(cwd);
  console.log("[Kitron][Node] Building frontend…");

  const viteBin = join(cwd, "node_modules", ".bin", process.platform === "win32" ? "vite.cmd" : "vite");
  const hasViteConfig =
    existsSync(join(cwd, "vite.config.ts")) || existsSync(join(cwd, "vite.config.js"));
  const hasViteBin = existsSync(viteBin);
  if (hasViteConfig && hasViteBin) {
    try {
      execFileSync(viteBin, ["build"], { cwd, stdio: "inherit", env: { ...process.env } });
    } catch {
      throw new Error("BUILD_FAILED: Vite build failed");
    }
  } else if (hasViteConfig) {
    // No installed Vite (e.g. fresh scaffold without `pnpm install`): still
    // produce a runnable layout from static sources instead of failing.
    console.log("[Kitron][Node] Vite not installed — copying static sources (run pnpm install for a real bundle).");
  } else {
    console.log("[Kitron][Node] No Vite config — copying static assets only.");
  }

  const outDir = join(cwd, config.build.outDir);
  const webDir = join(outDir, "web");
  const appDir = join(outDir, "app");
  mkdirSync(webDir, { recursive: true });
  mkdirSync(appDir, { recursive: true });

  const frontendDist = join(cwd, config.build.frontendDist);
  if (existsSync(frontendDist)) {
    cpSync(frontendDist, webDir, { recursive: true });
  } else {
    // Fallback: ship static entry points so the layout stays launchable.
    for (const f of ["index.html"]) {
      const src = join(cwd, f);
      if (existsSync(src)) cpSync(src, join(webDir, f));
    }
    if (existsSync(join(cwd, "public"))) {
      cpSync(join(cwd, "public"), webDir, { recursive: true });
    }
  }

  const manifest = {
    name: config.build.productName,
    appId: config.build.appId,
    version: config.build.version,
    backendEntry: config.build.backendEntry,
    createdAt: new Date().toISOString(),
  };
  writeFileSync(join(appDir, "kitron-manifest.json"), JSON.stringify(manifest, null, 2));

  // Keep kitron.config for the packager.
  const cfgPath = join(cwd, "kitron.config.ts");
  if (existsSync(cfgPath)) {
    writeFileSync(join(appDir, "kitron.config.snapshot.txt"), readFileSync(cfgPath, "utf8"));
  }

  console.log(`[Kitron][Node] Build complete → ${outDir}`);
  return { outDir };
}
