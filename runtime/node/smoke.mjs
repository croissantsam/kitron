#!/usr/bin/env node
// Integration smoke for the Kitron node host protocol (runtime/node/host.mjs).
// Drives: hello/ready handshake, successful invoke, error invoke, log frame.
import { spawn } from "node:child_process";
import path from "node:path";
import process from "node:process";

import url from "node:url";
const here = path.dirname(url.fileURLToPath(import.meta.url));
const host = path.join(here, "host.mjs");
const backend = path.join(here, "test-backend.mjs");

const child = spawn(process.execPath, [host, backend], {
  env: { ...process.env, KITRON_NODE_HOST: "1" },
  stdio: ["pipe", "pipe", "pipe"],
});

const frames = [];
let buffer = "";
let stderr = "";
child.stdout.setEncoding("utf8");
child.stdout.on("data", (chunk) => {
  buffer += chunk;
  let index;
  while ((index = buffer.indexOf("\n")) >= 0) {
    const line = buffer.slice(0, index);
    buffer = buffer.slice(index + 1);
    if (line.trim()) {
      frames.push(JSON.parse(line));
    }
  }
});
child.stderr.setEncoding("utf8");
child.stderr.on("data", (chunk) => {
  stderr += chunk;
});

const exitCode = child.once("exit", (code) => recordExit(code));
let timedOut = false;
const timeout = setTimeout(() => {
  timedOut = true;
  process.kill(child.pid, "SIGTERM");
}, 9000);
function recordExit() {}

function send(frame) {
  child.stdin.write(`${JSON.stringify(frame)}\n`);
}

await new Promise((resolve) => {
  const poll = setInterval(() => {
    const ready = frames.find((frame) => frame.type === "ready");
    if (ready) {
      clearInterval(poll);
      resolve(ready);
    }
  }, 50);
});

const results = {};
function waitFor(type, filter = () => true) {
  return new Promise((resolve, reject) => {
    const start = Date.now();
    const poll = setInterval(() => {
      if (timedOut) {
        clearInterval(poll);
        reject(new Error("host timed out"));
        return;
      }
      const match = frames.find((frame) => frame.type === type && filter(frame));
      if (match) {
        clearInterval(poll);
        resolve(match);
      } else if (Date.now() - start > 5000) {
        clearInterval(poll);
        reject(new Error(`no ${type} frame arrived`));
      }
    }, 25);
  });
}

// 1. Successful invoke.
send({ type: "invoke", id: 1, method: "math.add", payload: { a: 19, b: 23 } });
const add = await waitFor("result", (frame) => frame.id === 1);
results.add = add.result?.sum === 42;

// 2. Async invoke.
send({ type: "invoke", id: 2, method: "users.get", payload: { id: 7 } });
const user = await waitFor("result", (frame) => frame.id === 2);
results.user = user.result?.name === "Ada";

// 3. Handler error with typed code.
send({ type: "invoke", id: 3, method: "users.get", payload: { id: 404 } });
const notFound = await waitFor("error", (frame) => frame.id === 3);
results.typedError = notFound.code === "USER_NOT_FOUND";

// 4. Missing handler.
send({ type: "invoke", id: 4, method: "does.notExist" });
const noHandler = await waitFor("error", (frame) => frame.id === 4);
results.noHandler = noHandler.code === "NODE_NO_HANDLER";

// 5. Throw inside handler.
send({ type: "invoke", id: 5, method: "echo.throw" });
const thrown = await waitFor("error", (frame) => frame.id === 5);
results.thrown = thrown.code === "NODE_HANDLER_ERROR";

// 6. Log frame from console.forwarding.
const log = await waitFor("log", (frame) => frame.message === "smoke backend loaded");
results.logFrame = log.level === "info";

// 7. Graceful shutdown via frame.
send({ type: "shutdown", reason: "smoke-done" });
const exitCodeSeen = await new Promise((resolve) => {
  child.once("exit", (code) => resolve(code));
});
results.gracefulExit = exitCodeSeen === 0;

clearTimeout(timeout);

let pass = true;
const failures = [];
for (const [name, ok] of Object.entries(results)) {
  if (!ok) {
    pass = false;
    failures.push(name);
  }
}

console.log("results:", results);
if (stderr.trim()) {
  console.log("host stderr:", stderr.trim().slice(0, 500));
}
if (!pass) {
  console.error("FAIL: nodes:", failures.join(", "));
  process.exit(1);
}
console.log("node host smoke: PASS");
