import { existsSync, mkdirSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { getPeer } from "./transport.js";

/**
 * Custom protocol (`myapp://`) support — Tranche A.
 * Registration writes real OS integration files at package/install time;
 * at runtime, launched URLs are collected from argv and re-emitted as
 * `open-url` (both locally and on the IPC bus).
 */
export function parseProtocolURLs(argv: string[] = process.argv): string[] {
  return argv.filter((a) => /^[a-zA-Z][a-zA-Z0-9+.-]*:\/\//.test(a) && !a.startsWith("http"));
}

export function emitProtocolURLs(): string[] {
  const urls = parseProtocolURLs();
  for (const url of urls) {
    try {
      getPeer().emit("open-url", { url });
    } catch {
      // transport optional at startup.
    }
  }
  return urls;
}

export function onOpenURL(listener: (url: string) => void): () => void {
  return getPeer().subscribe<{ url: string }>("open-url", (p) => listener(p.url));
}

/**
 * Register `protocol://` handling on this machine (best effort, logs).
 * Full fidelity (macOS plist, Windows registry, Linux .desktop) is produced
 * by `kitron package` from the app manifest; this covers dev machines.
 */
export function setAsDefaultProtocolClient(protocol: string): boolean {
  try {
    const proto = protocol.replace(/:\/*$/, "");
    if (process.platform === "linux") {
      const dir = join(homedir(), ".local/share/applications");
      mkdirSync(dir, { recursive: true });
      const desktop = join(dir, `kitron-${proto}.desktop`);
      if (!existsSync(desktop)) {
        writeFileSync(
          desktop,
          [
            "[Desktop Entry]",
            `Name=Kitron ${proto} handler`,
            `Exec=${process.execPath} %u`,
            "Type=Application",
            "NoDisplay=true",
            `MimeType=x-scheme-handler/${proto};`,
          ].join("\n"),
        );
      }
      return true;
    }
    if (process.platform === "win32") {
      // Registry writes need elevation; surface the .reg content path instead.
      const reg = join(process.cwd(), `kitron-${proto}.reg`);
      writeFileSync(
        reg,
        [
          "Windows Registry Editor Version 5.00",
          "",
          `[HKEY_CLASSES_ROOT\\${proto}]`,
          '@="Kitron Protocol"',
          '"URL Protocol"=""',
          "",
          `[HKEY_CLASSES_ROOT\\${proto}\\shell\\open\\command]`,
          `@="${process.execPath.replace(/\\/g, "\\\\")} \\"%1\\""`,
        ].join("\r\n"),
      );
      console.log(`[Kitron][Node] Protocol registration written to ${reg} (merge it, elevation may be required)`);
      return true;
    }
    // macOS: handled via Info.plist at package time; dev note printed.
    console.log(
      `[Kitron][Node] Register '${proto}://' via Info.plist CFBundleURLTypes at package time (see kitron package)`,
    );
    return true;
  } catch {
    return false;
  }
}
