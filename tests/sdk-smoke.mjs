#!/usr/bin/env node
/**
 * SDK smoke test — exercises the TypeScript layer outside a native runtime.
 *
 * It vendors the SDK into a temporary app (the same pipeline `kitron create`
 * uses), then asserts the typed-error contract (spec §33) and the "no native
 * runtime" behaviour of the public namespaces.
 *
 * This catches the class of bug where a native reply envelope is swallowed
 * instead of surfacing a typed error.
 */
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

import { vendorSdkPackages } from "../cli/src/create.ts";

const repoRoot = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");

const workdir = fs.mkdtempSync(path.join(os.tmpdir(), "kitron-sdk-smoke-"));
const appRoot = path.join(workdir, "app");
fs.mkdirSync(appRoot, { recursive: true });

const results = {};
function check(name, fn) {
  try {
    const ok = fn();
    results[name] = ok === true;
  } catch (error) {
    results[name] = false;
    console.error(`  ✗ ${name}: ${error?.message ?? error}`);
  }
}

try {
  vendorSdkPackages(appRoot, repoRoot);

  const kitron = await import(
    pathToFileURL(path.join(appRoot, "node_modules", "kitron", "dist", "index.js")).href
  );
  const core = await import(
    pathToFileURL(path.join(appRoot, "node_modules", "@kitron", "core", "dist", "index.js")).href
  );

  check("throwIfFailed throws on a JSON error envelope", () => {
    let thrown;
    try {
      core.throwIfFailed(
        JSON.stringify({
          ok: false,
          error: { code: "WINDOW_CREATION_FAILED", message: "boom", platform: "macos" },
        }),
      );
    } catch (error) {
      thrown = error;
    }
    return (
      thrown instanceof kitron.KitronError &&
      thrown.code === "WINDOW_CREATION_FAILED" &&
      thrown.platform === "macos"
    );
  });

  check("throwIfFailed accepts a success envelope", () => {
    core.throwIfFailed(JSON.stringify({ ok: true, result: { id: 1 } }));
    return true;
  });

  check("throwIfFailed ignores a non-envelope payload", () => {
    core.throwIfFailed("not json");
    core.throwIfFailed(undefined);
    return true;
  });

  check("KitronError.from rehydrates a native error", () => {
    const error = kitron.KitronError.from({
      code: "PERMISSION_DENIED",
      message: "nope",
      platform: "macos",
    });
    return error.code === "PERMISSION_DENIED" && error.is("PERMISSION_DENIED");
  });

  check("app.info works without a native runtime", () => {
    process.env.KITRON_APP_NAME = "SmokeApp";
    process.env.KITRON_APP_VERSION = "1.2.3";
    const info = kitron.app.info();
    return info.name === "SmokeApp" && info.version === "1.2.3";
  });

  let invokeError;
  try {
    await kitron.backend.invoke("math.add", { a: 1, b: 2 });
  } catch (error) {
    invokeError = error;
  }
  results["backend.invoke throws KITRON_UNAVAILABLE"] =
    invokeError instanceof kitron.KitronError &&
    invokeError.code === "KITRON_UNAVAILABLE";

  let clipboardError;
  try {
    await kitron.clipboard.write("x");
  } catch (error) {
    clipboardError = error;
  }
  results["clipboard.write throws KITRON_UNAVAILABLE"] =
    clipboardError instanceof kitron.KitronError &&
    clipboardError.code === "KITRON_UNAVAILABLE";

  let trayError;
  try {
    await kitron.tray.create({});
  } catch (error) {
    trayError = error;
  }
  results["tray.create throws KITRON_UNAVAILABLE"] =
    trayError instanceof kitron.KitronError && trayError.code === "KITRON_UNAVAILABLE";
} finally {
  fs.rmSync(workdir, { recursive: true, force: true });
}

let pass = true;
const failures = [];
for (const [name, ok] of Object.entries(results)) {
  if (!ok) {
    pass = false;
    failures.push(name);
  }
}

console.log("results:", results);
if (!pass) {
  console.error("FAIL:", failures.join(", "));
  process.exit(1);
}
console.log("sdk smoke: PASS");
