import { describe, expect, it } from "vitest";
import { IPCPeer, IpcHub, createInMemoryPair, parseFrameLine } from "@kitron/ipc";

describe("parseFrameLine", () => {
  it("accepts NDJSON frames, rejects logs", () => {
    expect(parseFrameLine('{"id":"1","channel":"ping","payload":null}')).toMatchObject({
      channel: "ping",
    });
    expect(parseFrameLine("[Kitron][Node] Backend started")).toBeNull();
    expect(parseFrameLine("not json")).toBeNull();
  });
});

describe("IpcHub (§10 three-way routing)", () => {
  it("routes host channels to the host link, others to the backend", async () => {
    const hub = new IpcHub();
    const hostPeer = new IPCPeer((f) => hub.routeFrom("host", f));
    hostPeer.handle("ping", () => "pong");
    hostPeer.handle("app.getName", () => "Mock");
    hub.addLink({ name: "host", deliver: (f) => hostPeer.receive(f) }, { host: true });

    const backendPeer = new IPCPeer((f) => hub.routeFrom("backend", f));
    backendPeer.handle("todos.list", () => ["a", "b"]);
    hub.addLink({ name: "backend", deliver: (f) => backendPeer.receive(f) }, { isDefault: true });

    const caller = new IPCPeer((f) => hub.routeFrom("caller", f));
    hub.addLink({ name: "caller", deliver: (f) => caller.receive(f) });

    await expect(caller.invoke("ping")).resolves.toBe("pong");
    await expect(caller.invoke("app.getName")).resolves.toBe("Mock");
    await expect(caller.invoke("todos.list")).resolves.toEqual(["a", "b"]);
    await expect(caller.invoke("nothing.here", null, { timeout: 300 })).rejects.toMatchObject({
      code: "HANDLER_NOT_FOUND",
    });
  });

  it("broadcasts events to every other link", async () => {
    const hub = new IpcHub();
    const seen: string[] = [];
    const mk = (name: string, opts = {}) => {
      const p = new IPCPeer((f) => hub.routeFrom(name, f));
      p.subscribe("tick", () => void seen.push(name));
      hub.addLink({ name, deliver: (f) => p.receive(f) }, opts);
      return p;
    };
    const a = mk("a");
    mk("b");
    mk("c");
    a.emit("tick", 1);
    await new Promise((r) => setTimeout(r, 50));
    expect(seen.sort()).toEqual(["b", "c"]);
  });

  it("supports the in-memory pair as hub links", async () => {
    const [a, b] = createInMemoryPair();
    b.handle("echo", (p: unknown) => p);
    await expect(a.invoke("echo", 7)).resolves.toBe(7);
  });
});
