import { execFile } from "node:child_process";
import type { PowerIdleState, ThemeInfo, ThemeSource } from "@kitron/types";

function run(cmd: string, args: string[]): Promise<string> {
  return new Promise((resolve) => {
    execFile(cmd, args, (error, stdout) => {
      resolve(error ? "" : String(stdout).trim());
    });
  });
}

let themeSource: ThemeSource = "system";
let watcher: NodeJS.Timeout | null = null;
const themeListeners = new Set<(info: ThemeInfo) => void>();
let lastDark: boolean | null = null;

/** Read the OS-effective dark mode (Node-first: defaults/gsettings/registry). */
export async function readSystemDark(): Promise<boolean> {
  if (themeSource === "light") return false;
  if (themeSource === "dark") return true;
  try {
    if (process.platform === "darwin") {
      const out = await run("defaults", ["read", "-g", "AppleInterfaceStyle"]);
      return out.toLowerCase().includes("dark");
    }
    if (process.platform === "win32") {
      const out = await run("reg", [
        "query",
        "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
        "/v",
        "AppsUseLightTheme",
      ]);
      return /0x0\s*$/.test(out);
    }
    const out = await run("gsettings", ["get", "org.gnome.desktop.interface", "gtk-theme"]);
    return out.toLowerCase().includes("dark");
  } catch {
    return false;
  }
}

/** `nativeTheme` equivalent — Tranche A. */
export const nativeTheme = {
  get themeSource(): ThemeSource {
    return themeSource;
  },
  set themeSource(v: ThemeSource) {
    themeSource = v;
  },

  async shouldUseDarkColors(): Promise<boolean> {
    return readSystemDark();
  },

  async getInfo(): Promise<ThemeInfo> {
    return { source: themeSource, shouldUseDarkColors: await readSystemDark() };
  },

  /** Poll the OS theme; resolves the unsubscribe fn. Emits via peer "theme.updated". */
  onUpdated(listener: (info: ThemeInfo) => void): () => void {
    themeListeners.add(listener);
    if (!watcher) {
      lastDark = null;
      watcher = setInterval(async () => {
        const dark = await readSystemDark().catch(() => null);
        if (dark !== null && dark !== lastDark) {
          lastDark = dark;
          const info: ThemeInfo = { source: themeSource, shouldUseDarkColors: dark };
          for (const fn of [...themeListeners]) {
            try {
              fn(info);
            } catch {
              // never break the poller.
            }
          }
        }
      }, 2000);
      watcher.unref?.();
    }
    return () => {
      themeListeners.delete(listener);
      if (themeListeners.size === 0 && watcher) {
        clearInterval(watcher);
        watcher = null;
      }
    };
  },
};

/** `powerMonitor` equivalent — Tranche A (poll-based, Node-first). */
export const powerMonitor = {
  /** True when running on battery (unknown → false). */
  async isOnBatteryPower(): Promise<boolean> {
    try {
      if (process.platform === "darwin") {
        const out = await run("pmset", ["-g", "batt"]);
        const m = out.match(/Now drawing from '([^']+)'/);
        if (m) return m[1]!.toLowerCase().includes("batter");
        return out.includes("Battery Power");
      }
      if (process.platform === "linux") {
        const { readFile } = await import("node:fs/promises");
        const names = ["BAT0", "BAT1"];
        for (const n of names) {
          try {
            const status = await readFile(`/sys/class/power_supply/${n}/status`, "utf8");
            const s = status.trim().toLowerCase();
            if (s === "discharging") return true;
            if (s === "charging" || s === "full") return false;
          } catch {
            // no such battery — try next.
          }
        }
      }
    } catch {
      // fall through to unknown → false.
    }
    return false;
  },

  async getSystemIdleState(idleThreshold = 60): Promise<PowerIdleState> {
    const idle = await powerMonitor.getSystemIdleTime().catch(() => -1);
    if (idle < 0) return "unknown";
    return idle >= idleThreshold ? "idle" : "active";
  },

  /** Seconds since last input (unknown → -1). */
  async getSystemIdleTime(): Promise<number> {
    try {
      if (process.platform === "darwin") {
        const out = await run("ioreg", ["-c", "IOHIDSystem", "-r"]);
        const m = out.match(/"HIDIdleTime"\s*=\s*(\d+)/);
        if (m) return Math.floor(Number(m[1]) / 1_000_000_000);
        return -1;
      }
      if (process.platform === "linux") {
        const out = await run("xprintidle", []);
        const ms = Number(out);
        if (Number.isFinite(ms)) return Math.floor(ms / 1000);
        return -1;
      }
      if (process.platform === "win32") {
        const out = await run("powershell", [
          "-NoProfile",
          "-Command",
          "Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public static class Idle { [DllImport(\"user32.dll\")] public static extern bool GetLastInputInfo(ref LASTINPUTINFO i); public struct LASTINPUTINFO { public uint cbSize; public uint dwTime; } public static uint GetIdle() { var i = new LASTINPUTINFO(); i.cbSize = (uint)System.Runtime.InteropServices.Marshal.SizeOf(i); return GetLastInputInfo(ref i) ? ((uint)Environment.TickCount - i.dwTime)/1000 : 999999; } }'; [Idle]::GetIdle()",
        ]);
        const s = Number(out);
        if (Number.isFinite(s)) return s;
        return -1;
      }
    } catch {
      // fall through.
    }
    return -1;
  },
};
