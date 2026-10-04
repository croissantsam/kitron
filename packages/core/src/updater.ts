import { createWriteStream } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { get as httpsGet } from "node:https";
import { get as httpGet } from "node:http";
import type { FeedVersion, UpdateFeed, UpdateInfo, UpdaterEvent } from "@kitron/types";
import { getPeer, isTransportConfigured } from "./transport.js";

let feedURL: string | null = null;
let currentVersion = "0.1.0";
const listeners = new Map<UpdaterEvent, Set<(payload?: unknown) => void>>();

function emitLocal(event: UpdaterEvent, payload?: unknown): void {
  for (const fn of listeners.get(event) ?? []) {
    try {
      fn(payload);
    } catch {
      // listener errors stay local.
    }
  }
  if (isTransportConfigured()) {
    try {
      getPeer().emit(`updater.${event}`, payload ?? null);
    } catch {
      // bus optional.
    }
  }
}

function getterFor(url: string): typeof httpsGet {
  return url.startsWith("http://") ? (httpGet as unknown as typeof httpsGet) : httpsGet;
}

function fetchJSON(url: string): Promise<unknown> {
  const get = getterFor(url);
  return new Promise((resolve, reject) => {
    get(url, (res) => {
      if (res.statusCode !== 200) {
        reject(new Error(`feed returned ${res.statusCode}`));
        res.resume();
        return;
      }
      let body = "";
      res.on("data", (d: Buffer) => {
        body += String(d);
      });
      res.on("end", () => {
        try {
          resolve(JSON.parse(body));
        } catch (err) {
          reject(err);
        }
      });
    }).on("error", reject);
  });
}

function compareVersions(a: string, b: string): number {
  const pa = a.split(".").map((x) => Number(x) || 0);
  const pb = b.split(".").map((x) => Number(x) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d !== 0) return d > 0 ? 1 : -1;
  }
  return 0;
}

/**
 * Auto-updater — feed check + download are real (Node-first); install
 * stays platform-specific (documented per target: replace bundle, relaunch).
 */
export const updater = {
  setFeedURL(url: string, version?: string): void {
    feedURL = url;
    if (version) currentVersion = version;
  },

  getFeedURL(): string | null {
    return feedURL;
  },

  on(event: UpdaterEvent, listener: (payload?: unknown) => void): () => void {
    let set = listeners.get(event);
    if (!set) {
      set = new Set();
      listeners.set(event, set);
    }
    set.add(listener);
    return () => {
      set!.delete(listener);
    };
  },

  async check(): Promise<UpdateInfo> {
    return this.checkForUpdates().then(
      (v) => ({ available: true, version: v.version, notes: v.notes, url: v.url }),
      () => ({ available: false }),
    );
  },

  async checkForUpdates(): Promise<FeedVersion> {
    if (!feedURL) throw new Error("updater feed URL not set (setFeedURL)");
    emitLocal("checking-for-update");
    const feed = (await fetchJSON(feedURL)) as FeedVersion;
    if (!feed?.version) throw new Error("malformed update feed");
    if (compareVersions(feed.version, currentVersion) <= 0) {
      emitLocal("update-not-available", { version: currentVersion });
      throw Object.assign(new Error("no update available"), { code: "NO_UPDATE" });
    }
    emitLocal("update-available", feed);
    return feed;
  },

  async download(feed?: FeedVersion): Promise<string> {
    const target = feed ?? (await this.checkForUpdates().catch(() => null));
    if (!target?.url) throw new Error("no update artifact URL");
    const get = getterFor(target.url);
    const dest = join(tmpdir(), `kitron-update-${Date.now()}`);
    await new Promise<void>((resolve, reject) => {
      get(target.url!, (res) => {
        if (res.statusCode !== 200) {
          reject(new Error(`download returned ${res.statusCode}`));
          res.resume();
          return;
        }
        const out = createWriteStream(dest);
        res.pipe(out);
        out.on("finish", () => resolve());
        out.on("error", reject);
      }).on("error", reject);
    });
    emitLocal("update-downloaded", { path: dest, version: target.version });
    return dest;
  },

  async install(): Promise<void> {
    // Platform install (bundle replace + relaunch) is target-specific.
    // Staged artifact from download() is applied by the installer step.
    if (isTransportConfigured()) {
      try {
        await getPeer().invoke("updater.install");
        return;
      } catch {
        // fall through to explicit error.
      }
    }
    throw new Error(
      "updater.install needs a platform installer (see kitron package per target)",
    );
  },
};

export type { UpdateFeed };
