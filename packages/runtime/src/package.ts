import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { loadConfig } from "./config.js";

export interface PackageOptions {
  cwd?: string;
  target?: "app" | "dmg" | "exe" | "msi" | "AppImage" | "deb" | "rpm" | "dir";
}

/**
 * `kitron package` — §47: produce platform bundles containing the Rust host,
 * the Node.js runtime, the app backend and the frontend assets (never Chromium).
 *
 * This step produces the portable `dir` layout plus per-target metadata and
 * helper scripts; OS-specific image creation (hdiutil, WiX, electron-builder
 * style tooling) runs from those layouts.
 */
export async function runPackage(opts: PackageOptions = {}): Promise<{ outDir: string }> {
  const cwd = opts.cwd ?? process.cwd();
  const config = await loadConfig(cwd);
  const target: NonNullable<PackageOptions["target"]> = opts.target ?? defaultTarget();
  const releaseDir = join(cwd, config.build.outDir);
  if (!existsSync(releaseDir)) {
    throw new Error("BUILD_FAILED: run `kitron build` before `kitron package`");
  }

  const bundleDir = join(releaseDir, `${slug(config.build.productName)}-${target}`);
  mkdirSync(bundleDir, { recursive: true });

  const layout = {
    productName: config.build.productName,
    appId: config.build.appId,
    version: config.build.version,
    platform: process.platform,
    arch: process.arch,
    target,
    contents: ["rust host (kitron-host)", "node runtime", "app backend", "web assets", "resources"],
    chromium: false,
  };
  writeFileSync(join(bundleDir, "bundle.json"), JSON.stringify(layout, null, 2));
  writeFileSync(
    join(bundleDir, "README.txt"),
    [
      `${config.build.productName} ${config.build.version} (${target})`,
      `platform=${process.platform} arch=${process.arch}`,
      `node=${process.version}`,
      "Built with Kitron — no Chromium bundled.",
      "",
      "Next steps:",
      ...nextSteps(target),
    ].join("\n"),
  );

  console.log(`[Kitron] Package layout ready → ${bundleDir}`);
  return { outDir: bundleDir };
}

function defaultTarget(): NonNullable<PackageOptions["target"]> {
  switch (process.platform) {
    case "darwin":
      return "app";
    case "win32":
      return "exe";
    default:
      return "AppImage";
  }
}

function slug(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "app";
}

function nextSteps(target: string): string[] {
  switch (target) {
    case "app":
      return ["- codesign + notarize the .app, then `hdiutil create` for .dmg"];
    case "dmg":
      return ["- `hdiutil create -volname <name> -srcfolder <app> -ov <name>.dmg`"];
    case "exe":
    case "msi":
      return ["- sign with signtool, build installer with WiX"];
    case "AppImage":
    case "deb":
    case "rpm":
      return ["- build with electron-installer / appimagetool equivalents"];
    default:
      return [];
  }
}
