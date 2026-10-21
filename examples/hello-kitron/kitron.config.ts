/**
 * Example app configuration (spec §37).
 *
 * The CLI serializes this into `KITRON_CONFIG_JSON` and hands it to the Rust
 * core at boot.
 */

import type { KitronConfig } from "kitron";

const config: KitronConfig = {
  app: {
    name: "HelloKitronExample",
    identifier: "dev.croissantlabs.hello-kitron-example",
    version: "0.1.0",
  },
  window: {
    width: 900,
    height: 620,
    title: "Kitron example",
  },
  node: {
    entry: "./backend/index.ts",
    autoStart: true,
  },
  // Sensitive native APIs are opt-in (§23).
  permissions: ["clipboard", "shell"],
  extra: {},
};

export default config;
