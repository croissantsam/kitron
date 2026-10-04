import { describe, expect, it } from "vitest";
import { IPCPeer, NDJSONDecoder, createInMemoryPair, encodeFrame } from "@kitron/ipc";

describe("NDJSON codec", () => {
  it("splits lines and buffers partials", () => {
    const dec = new NDJSONDecoder();
    expect(dec.push('{"a":1}\n{"b":')).toHaveLength(1);
    expect(dec.push("2}\n")).toHaveLength(1);
  });

  it("encodes frames with trailing newline", () => {
    const f = encodeFrame({ channel: "ping", payload: null });
    expect(f.endsWith("\n")).toBe(true);
  });
});

describe("IPCPeer", () => {
  it("routes invoke → handler → response", async () => {
    const [a, b] = createInMemoryPair();
    b.handle("ping", () => "pong");
    await expect(a.invoke("ping")).resolves.toBe("pong");
  });

  it("propagates handler errors as KitronError", async () => {
    const [a, b] = createInMemoryPair();
    b.handle("boom", () => {
      throw new Error("kaput");
    });
    await expect(a.invoke("boom")).rejects.toMatchObject({ message: "kaput" });
  });

  it("rejects unknown channels with HANDLER_NOT_FOUND", async () => {
    const [a] = createInMemoryPair();
    await expect(a.invoke("nope", null, { timeout: 500 })).rejects.toMatchObject({
      code: "HANDLER_NOT_FOUND",
    });
  });

  it("times out", async () => {
    const [a, b] = createInMemoryPair();
    b.handle("slow", () => new Promise(() => undefined));
    await expect(a.invoke("slow", null, { timeout: 50 })).rejects.toMatchObject({
      code: "IPC_TIMEOUT",
    });
  });

  it("delivers events to subscribers", async () => {
    const [a, b] = createInMemoryPair();
    const got = await new Promise<unknown>((resolve) => {
      a.subscribe("tick", resolve);
      b.emit("tick", 42);
    });
    expect(got).toBe(42);
  });

  it("supports request/response in both directions", async () => {
    const peer = new IPCPeer(() => undefined);
    const [a, b] = createInMemoryPair();
    void peer;
    b.handle("add", (n: unknown) => (n as number) + 1);
    await expect(a.invoke("add", 1)).resolves.toBe(2);
  });
});
