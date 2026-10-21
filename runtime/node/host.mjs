//! Kitron Node host bootstrap (spec §29).
//!
//! Spawned by the Rust supervisor as: `node host.mjs <backend-entry>`.
//!
//! Responsibilities:
//! - evaluate the app's backend entry (TypeScript runs natively on Node 24+)
//! - expose the `KitronNode` backend API (`onInvoke`, `emit`) before the entry
//!   loads so it can register handlers at import time
//! - speak the Kitron NDJSON protocol over stdio:
//!     hello → ready → (invoke ⇄ result/error)* / log* / event* → shutdown
//!
//! Everything here is app-agnostic; backends never import Kitron runtime
//! internals — they use the `KitronNode` global or the generated `backend/kitron.ts`
//! shim in the app template.

import process from "node:process";
import readline from "node:readline";
import path from "node:path";
import { pathToFileURL } from "node:url";

const PROTOCOL_VERSION = 1;
const HOST_VERSION = "0.1.0";

/** Registered backend handlers: method → (payload) => result | Promise<result> */
const handlers = new Map();
/** Invoke ids with work in flight (drained before shutdown). */
const pending = new Set();
let shuttingDown = false;

// -- KitronNode backend API (installed BEFORE the app entry is imported) -----

globalThis.KitronNode = Object.freeze({
  protocolVersion: PROTOCOL_VERSION,
  hostVersion: HOST_VERSION,
  /** Registers a handler for Rust-side `backend.invoke(method, payload)` calls. */
  onInvoke(method, handler) {
    if (typeof method !== "string" || method.length === 0) {
      throw new TypeError("KitronNode.onInvoke: method must be a non-empty string");
    }
    if (typeof handler !== "function") {
      throw new TypeError("KitronNode.onInvoke: handler must be a function");
    }
    handlers.set(method, handler);
  },
  /** Emits a one-way event to the app (surfaced as `node.on("backend.event")`). */
  emit(name, payload) {
    if (typeof name !== "string" || name.length === 0) {
      throw new TypeError("KitronNode.emit: name must be a non-empty string");
    }
    send({ type: "event", name, payload });
  },
  appMeta() {
    try {
      return JSON.parse(process.env.KITRON_APP_META || "{}");
    } catch {
      return {};
    }
  },
});

// -- Console forwarding → unified logging (spec §34) --------------------------
//
// Node console output is re-routed into structured log frames; the Rust core
// relays them into the unified logger so dev output has one consistent shape.

const CONSOLE_LEVELS = [
  ["debug", "debug"],
  ["log", "info"],
  ["info", "info"],
  ["warn", "warn"],
  ["error", "error"],
];
for (const [method, level] of CONSOLE_LEVELS) {
  console[method] = (...args) => {
    const message = args
      .map((part) => (typeof part === "string" ? part : inspect(part)))
      .join(" ");
    send({ type: "log", level, message });
  };
}

function inspect(value) {
  // Risk-free stringification for the common shapes; falls back repr.
  if (value instanceof Error) {
    return value.stack || `${value.name}: ${value.message}`;
  }
  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

// -- Stdout protocol writer ---------------------------------------------------

function send(frame) {
  if (shuttingDown && frame.type !== "result" && frame.type !== "error") {
    return;
  }
  process.stdout.write(`${JSON.stringify(frame)}\n`);
}

function errorResult(id, code, message) {
  send({ type: "error", id, code, message });
}

// -- Invoke handling (spec §30) -----------------------------------------------

async function handleInvoke(frame) {
  if (typeof frame.id !== "number" || typeof frame.method !== "string") {
    send({ type: "error", id: 0, code: "NODE_BAD_FRAME", message: "malformed invoke frame" });
    return;
  }
  const handler = handlers.get(frame.method);
  if (!handler) {
    errorResult(frame.id, "NODE_NO_HANDLER", `no backend handler for "${frame.method}"`);
    return;
  }
  pending.add(frame.id);
  try {
    const result = await handler(frame.payload);
    send({ type: "result", id: frame.id, result: result === undefined ? null : result });
  } catch (error) {
    const code = typeof error?.code === "string" ? error.code : "NODE_HANDLER_ERROR";
    const message = error?.message || String(error);
    errorResult(frame.id, code, message);
  } finally {
    pending.delete(frame.id);
  }
}

// -- Lifecycle ----------------------------------------------------------------

function shutdown(reason) {
  if (shuttingDown) {
    return;
  }
  shuttingDown = true;
  const drainedAt = Date.now();
  const graceful = setInterval(() => {
    const drained = pending.size === 0 || Date.now() - drainedAt > 1500;
    if (drained) {
      clearInterval(graceful);
      // eslint-disable-next-line n/no-process-exit
      process.exit(0);
    }
  }, 25);
}

process.on("SIGTERM", () => shutdown("sigterm"));
process.on("SIGINT", () => shutdown("sigint"));
process.on("uncaughtException", (error) => {
  send({
    type: "log",
    level: "error",
    message: `uncaught exception: ${error?.stack || error}`,
  });
});

// -- Boot the backend ----------------------------------------------------------

const entry = process.argv[2];
if (!entry) {
  send({ type: "log", level: "error", message: "no backend entry provided (argv[2])" });
  process.exit(2);
}

const entryUrl = pathToFileURL(path.resolve(entry)).href;

try {
  await import(entryUrl);
} catch (error) {
  send({
    type: "log",
    level: "error",
    message: `backend "${entry}" failed to load: ${error?.stack || error}`,
  });
  process.exit(3);
}

// Handshake: register → ready. The Rust supervisor flips to Ready on the
// second frame and starts servicing invoke calls.
send({
  type: "hello",
  protocol: PROTOCOL_VERSION,
  pid: process.pid,
  hostVersion: HOST_VERSION,
  entry,
});
send({ type: "ready", handlers: handlers.size });

const readlineInterface = readline.createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
});
readlineInterface.on("line", (line) => {
  if (line.trim().length === 0) {
    return;
  }
  let frame;
  try {
    frame = JSON.parse(line);
  } catch {
    send({ type: "log", level: "warn", message: `malformed frame: ${line.slice(0, 120)}` });
    return;
  }
  switch (frame.type) {
    case "invoke": {
      void handleInvoke(frame);
      break;
    }
    case "shutdown": {
      shutdown(frame.reason);
      break;
    }
    default:
      // Unknown frames from future protocol versions are ignored (§16 forward
      // compatibility); log at debug for developer visibility.
      send({ type: "log", level: "debug", message: `ignored frame: ${frame.type ?? "?"}` });
  }
});
