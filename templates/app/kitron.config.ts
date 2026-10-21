/**
 * Kitron app configuration (spec §21).
 *
 * The CLI serializes this into `KITRON_CONFIG_JSON` and hands it to the Rust
 * core at boot. Shape mirrors `@kitron/protocol::KitronConfig` (camelCase).
 */

import type { KitronConfig } from "kitron";

const config: KitronConfig = {
  app: {
    name: "HelloKitron",
    identifier: "dev.croissantlabs.hello-kitron",
    version: "0.1.0",
  },
  window: {
    width: 1024,
    height: 640,
    title: "Hello Kitron",
  },
  node: {
    entry: "./backend/index.ts",
    autoStart: true,
  },
  permissions: [],
  extra: {},
};

export default config;
