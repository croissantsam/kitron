import { execSync } from "node:child_process";

/** `kitron doctor` — environment diagnostics. */
export async function cmdDoctor(): Promise<void> {
  const rows: Array<[string, string, boolean]> = [];
  const check = (label: string, fn: () => string) => {
    try {
      rows.push([label, fn(), true]);
    } catch (err) {
      rows.push([label, (err as Error).message, false]);
    }
  };
  check("node", () => execSync("node --version").toString().trim());
  check("pnpm", () => execSync("pnpm --version").toString().trim());
  check("rustc", () => execSync("rustc --version").toString().trim());
  check("cargo", () => execSync("cargo --version").toString().trim());
  check("platform", () => `${process.platform}-${process.arch}`);

  // Native WebView availability per OS (§7).
  const webview =
    process.platform === "darwin"
      ? "WKWebView (system WebKit)"
      : process.platform === "win32"
        ? "WebView2 (Edge runtime — must be installed)"
        : "WebKitGTK (libwebkit2gtk required)";
  rows.push(["webview", webview, true]);

  let failed = false;
  for (const [label, value, ok] of rows) {
    console.log(`${ok ? "✓" : "✗"} ${label}: ${value}`);
    if (!ok) failed = true;
  }
  if (failed) process.exitCode = 1;
}
