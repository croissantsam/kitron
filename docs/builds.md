# Builds

- `kitron dev` → Vite (+TanStack Start) + Node backend + Rust host.
  Hot reload via Vite.
- Native host binary: `cd native/rust && cargo build --features native-webview`
  (headless IPC-only without the feature — no window opens). `kitron dev`
  finds `native/rust/target/debug/kitron-host` by upward search and falls
  back to the in-process mock host when absent. With the feature binary,
  `kitron dev` opens a real native window (single window for now: every
  `window.create` binds to it).
- `kitron build` → Vite build + `release/{web,app}` + manifest.
- `kitron package` → per-OS bundle layout (`.app/.dmg`, `.exe/.msi`,
  `.AppImage/.deb/.rpm`) containing host + Node runtime + backend +
  frontend — never Chromium. Sign/notarize progressively.
- Production embeds its own Node.js; never the user's install.
  Targets: macOS arm64/x64, Windows x64/arm64, Linux x64/arm64.
