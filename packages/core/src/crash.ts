import { randomUUID } from "node:crypto";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { CrashReport } from "@kitron/types";

function crashDir(): string {
  const appId = (process.env["KITRON_APP_ID"] ?? "com.kitron.app").replace(
    /[^a-zA-Z0-9_.-]+/g,
    "-",
  );
  const dir =
    process.env["KITRON_CRASH_DIR"] ?? join(tmpdir(), `kitron-${appId}`, "Crashes");
  mkdirSync(dir, { recursive: true });
  return dir;
}

function writeReport(kind: CrashReport["kind"], message: string, stack?: string): CrashReport {
  const report: CrashReport = {
    id: randomUUID(),
    time: new Date().toISOString(),
    kind,
    message,
    stack,
    platform: `${process.platform}-${process.arch}`,
  };
  try {
    writeFileSync(join(crashDir(), `${report.id}.json`), JSON.stringify(report, null, 2));
  } catch {
    // reporting must never crash the reporter.
  }
  return report;
}

let started = false;

/** `crashReporter` equivalent — writes JSON reports, never uploads. */
export const crashReporter = {
  start(): void {
    if (started) return;
    started = true;
    crashDir();
    process.on("uncaughtException", (err) => {
      writeReport("node-uncaught", String((err as Error)?.message ?? err), (err as Error)?.stack);
    });
    process.on("unhandledRejection", (reason) => {
      const err = reason as Error;
      writeReport("node-unhandled", String(err?.message ?? reason), err?.stack);
    });
  },

  getLastCrashReport(): CrashReport | null {
    try {
      const dir = crashDir();
      if (!existsSync(dir)) return null;
      const files = readdirSync(dir)
        .filter((f) => f.endsWith(".json"))
        .map((f) => join(dir, f))
        .sort();
      if (files.length === 0) return null;
      return JSON.parse(readFileSync(files[files.length - 1]!, "utf8")) as CrashReport;
    } catch {
      return null;
    }
  },

  /** Test hook: write a synthetic report. */
  addReport(kind: CrashReport["kind"], message: string): CrashReport {
    return writeReport(kind, message);
  },
};
