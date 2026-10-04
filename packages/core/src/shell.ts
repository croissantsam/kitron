import { execFile, spawn } from "node:child_process";
import { rename } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import type { OpenExternalOptions } from "@kitron/types";

function opener(): { cmd: string; args: string[] } {
  switch (process.platform) {
    case "darwin":
      return { cmd: "open", args: [] };
    case "win32":
      return { cmd: "cmd", args: ["/c", "start", ""] };
    default:
      return { cmd: "xdg-open", args: [] };
  }
}

function detach(cmd: string, args: string[]): void {
  const child = spawn(cmd, args, { detached: true, stdio: "ignore" });
  child.unref();
}

/**
 * Electron-style `shell` — Tranche A, implemented with Node.js + OS
 * openers (Rule 2: no Rust needed).
 */
export const shell = {
  exec(command: string, args: string[] = []): Promise<{ stdout: string; stderr: string }> {
    return new Promise((resolve, reject) => {
      execFile(command, args, (error, stdout, stderr) => {
        if (error) {
          reject(
            Object.assign(error, {
              code: "NODE_START_FAILED",
              stdout: String(stdout),
              stderr: String(stderr),
            }),
          );
          return;
        }
        resolve({ stdout: String(stdout), stderr: String(stderr) });
      });
    });
  },

  /** Open a URL/file with the OS default handler (browser, mail…). */
  async openExternal(url: string, _options: OpenExternalOptions = {}): Promise<void> {
    const { cmd, args } = opener();
    await new Promise<void>((resolve, reject) => {
      execFile(cmd, [...args, url], (error) => {
        if (error) reject(error);
        else resolve();
      });
    });
  },

  /**
   * Open a path with its default app. Resolves to "" on success or an
   * error message (Electron convention).
   */
  async openPath(path: string): Promise<string> {
    try {
      const { cmd, args } = opener();
      await new Promise<void>((resolve, reject) => {
        execFile(cmd, [...args, path], (error) => {
          if (error) reject(error);
          else resolve();
        });
      });
      return "";
    } catch (err) {
      return (err as Error).message;
    }
  },

  /** Reveal a file in Finder / Explorer / the file manager. */
  async showItemInFolder(path: string): Promise<void> {
    if (process.platform === "darwin") {
      detach("open", ["-R", path]);
      return;
    }
    if (process.platform === "win32") {
      detach("explorer", [`/select,"${path}"`]);
      return;
    }
    detach("xdg-open", [dirname(path)]);
  },

  /** Move a file to the OS trash. Resolves true when trashed. */
  async trashItem(path: string): Promise<boolean> {
    try {
      if (process.platform === "darwin") {
        await shell.exec("osascript", [
          "-e",
          `tell application "Finder" to delete POSIX file ${JSON.stringify(path)}`,
        ]);
        return true;
      }
      if (process.platform === "win32") {
        const ps = [
          "Add-Type -AssemblyName Microsoft.VisualBasic",
          `[Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile(${singleQuote(path)},'OnlyErrorDialogs','SendToRecycleBin')`,
        ].join("; ");
        await shell.exec("powershell", ["-NoProfile", "-Command", ps]);
        return true;
      }
      // Linux: Freedesktop trash via gio, fallback to manual move.
      try {
        await shell.exec("gio", ["trash", path]);
        return true;
      } catch {
        const base = process.env["XDG_DATA_HOME"] ?? join(home(), ".local/share");
        const dest = join(base, "Trash", "files", `${basename(path)}.${Date.now()}`);
        await shell.exec("mkdir", ["-p", dirname(dest)]);
        await rename(path, dest);
        return true;
      }
    } catch {
      return false;
    }
  },

  beep(): void {
    process.stdout.write("\x07");
  },
};

function singleQuote(s: string): string {
  return `'${s.replace(/'/g, "''")}'`;
}

function home(): string {
  return process.env["HOME"] ?? tmpdir();
}
