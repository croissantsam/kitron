#!/usr/bin/env node
/**
 * Bridge conformance (anti-drift): the three hand-written surfaces that must
 * stay in lock-step are checked against each other.
 *
 *   1. The native module  — `RCT_EXPORT_METHOD(name: …)` in KitronBridge.mm
 *   2. The TypeScript API — `KitronNativeModule` methods in packages/core
 *   3. The C ABI          — `kitron_*` functions in the Rust shell + kitron.h
 *
 * A method that exists on one surface but not the others fails the run, and so
 * does an `extern "C"` Rust shell function missing its `#[unsafe(no_mangle)]`
 * (a missing no_mangle mangles the symbol into a Rust v0 name and the app
 * fails to link — this exact bug has happened once and is now a test).
 */

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const MM = path.join(ROOT, "runtime/macos/pod/Kitron/KitronBridge.mm");
const HEADER = path.join(ROOT, "runtime/macos/pod/Kitron/kitron.h");
const BRIDGE_TS = path.join(ROOT, "packages/core/src/bridge.ts");
const RUST_SHELL = path.join(ROOT, "runtime/rust/crates/kitron-macos/src/lib.rs");

const read = (file) => fs.readFileSync(file, "utf8");

/** Every JS-callable method the native module exports. */
function nativeMethods() {
  const source = read(MM);
  const methods = [...source.matchAll(/RCT_EXPORT_METHOD\((\w+):/g)].map((m) => m[1]);
  if (methods.length === 0) {
    throw new Error("no RCT_EXPORT_METHOD found — is the .mm still the bridge source?");
  }
  return new Set(methods);
}

/** Method names declared on the TS native-module interface. */
function typeScriptMethods() {
  const source = read(BRIDGE_TS);
  const start = source.indexOf("export interface KitronNativeModule");
  if (start === -1) {
    throw new Error("KitronNativeModule not found in bridge.ts");
  }
  const end = source.indexOf("\n}", start);
  const body = source.slice(start, end);
  const methods = [...body.matchAll(/\n  (\w+)\(/g)].map((m) => m[1]);
  if (methods.length === 0) {
    throw new Error("no methods parsed on the TypeScript interface");
  }
  return new Set(methods);
}

/** `pub … extern "C" fn kitron_*` defined by the Rust shell, + no_mangle audit. */
function rustExternFns() {
  const source = read(RUST_SHELL);
  const names = [];
  const missingNoMangle = [];

  // A declaration is a `pub … extern "C" fn kitron_*(` with the attributes
  // expected on the preceding lines (doc comments are skipped over — the regex
  // allows any `#[…]` line directly above `pub`).
  const re = /(^|\n)[ \t]*(#{1,2}\[[^\]\n]*\][ \t]*\n[ \t]*)*pub (unsafe )?extern "C" fn (kitron_\w+)/g;
  for (const match of source.matchAll(re)) {
    const name = match[4];
    names.push(name);
    // Attributes for THIS fn: the snippet between the previous `fn` boundary
    // and this declaration. Enough to see whether no_mangle is attached.
    const declaration = source.lastIndexOf(`fn ${name}(`, match.index + match[0].length);
    const preceding = source.slice(Math.max(0, declaration - 400), declaration);
    if (!/no_mangle/.test(preceding)) {
      missingNoMangle.push(name);
    }
  }
  return { names, missingNoMangle };
}

/** Names declared in the pod's public C header. */
function headerFns() {
  const source = read(HEADER);
  return new Set([...source.matchAll(/\b(kitron_[a-z0-9_]+)\s*\(/g)].map((m) => m[1]));
}

let failures = [];
const report = (label, condition, detail) => {
  if (!condition) {
    failures.push(`  ${label}: ${detail}`);
  }
};

try {
  const native = nativeMethods();
  const ts = typeScriptMethods();
  const rust = rustExternFns();
  const header = headerFns();

  for (const name of native) {
    report("native → TypeScript", ts.has(name), `.${name}() is missing from KitronNativeModule`);
  }
  for (const name of ts) {
    report("TypeScript → native", native.has(name), `bridge.ts declares ${name}() but the shell never exports it`);
  }
  for (const name of rust.missingNoMangle) {
    report("Rust ABI", false, `${name} has no #[unsafe(no_mangle)] — it would link under a Rust-mangled name`);
    if (!header.has(name)) {
      report("Rust ABI → header", false, `${name} is defined in Rust but missing from kitron.h`);
    }
  }
  for (const name of rust.names) {
    report("Rust ABI → header", header.has(name), `${name} is implemented in Rust but missing from kitron.h`);
  }
  for (const name of header) {
    if (!rust.names.includes(name)) {
      report("header → Rust ABI", false, `${name} is declared in kitron.h but no Rust shell implements it`);
    }
  }

  // Sanity: the JSI getter surface is also part of the module type contract.

  if (failures.length > 0) {
    console.error("bridge conformance failed:\n" + failures.join("\n"));
    process.exit(1);
  }

  console.log(
    `  bridge: PASS (${native.size} native methods, ${ts.size} TS methods, ` +
      `${rust.names.length} C ABI fns, all aligned)`,
  );
} catch (error) {
  console.error("bridge conformance could not run:", error.message);
  process.exit(1);
}
