import { describe, expect, it } from "vitest";
import { defineConfig, resolveConfig } from "@kitron/types";
import { isChannelAllowed } from "@kitron/native-api";
import { createKitronClient } from "@kitron/native-api";
import { createMockBackend } from "@kitron/runtime";
import { parseArgs } from "@kitron/cli";

describe("config (§21)", () => {
  it("resolves defaults", () => {
    const cfg = resolveConfig(defineConfig({}));
    expect(cfg.permissions.notifications).toBe(true);
    expect(cfg.permissions.globalShortcuts).toBe(false);
    expect(cfg.dev.host).toBe("127.0.0.1");
  });

  it("honours overrides", () => {
    const cfg = resolveConfig({ permissions: { globalShortcuts: true } });
    expect(cfg.permissions.globalShortcuts).toBe(true);
  });
});

describe("renderer allowlist (§20)", () => {
  it("allows app/window/native channels", () => {
    expect(isChannelAllowed("ping")).toBe(true);
    expect(isChannelAllowed("native.notification.show")).toBe(true);
    expect(isChannelAllowed("window.setTitle")).toBe(true);
  });

  it("blocks node internals", () => {
    expect(isChannelAllowed("node.exec")).toBe(false);
    expect(isChannelAllowed("fs.read")).toBe(false);
    expect(isChannelAllowed("require")).toBe(false);
  });

  it("client enforces the allowlist + ping/pong works", async () => {
    const { nodePeer, hostPeer } = createMockBackend();
    const client = createKitronClient((frame) => hostPeer.receive(frame));
    hostPeer.setSender((frame) => client.peer.receive(frame));
    // Wire nodePeer symmetrically is unnecessary here: hostPeer already serves ping.
    await expect(client.invoke("ping")).resolves.toBe("pong");
    await expect(client.invoke("node.exec")).rejects.toThrow();
    void nodePeer;
  });
});

describe("cli args", () => {
  it("parses create flags", () => {
    const parsed = parseArgs(["create", "my-app", "--template", "react"]);
    expect(parsed.command).toBe("create");
    expect(parsed.positional).toEqual(["my-app"]);
    expect(parsed.flags["template"]).toBe("react");
  });
});
