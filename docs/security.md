# Security

- Preload exposes only `window.kitron = { invoke, emit, subscribe, native }`.
- Channel allowlist enforced in preload AND in the TS client.
- `require / process / fs / node` never leak to the renderer.
- Sensitive capabilities gated by `kitron.config.ts` permissions
  (`filesystem`, `childProcess`, `notifications`, `globalShortcuts`, …);
  violations throw `PERMISSION_DENIED`.
