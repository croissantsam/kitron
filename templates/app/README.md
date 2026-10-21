# <App Name>

A desktop app built with [Kitron](https://github.com/croissantlabs/kitron):
React Native for the UI, Node.js for the backend, and a Rust/native core — no
Chromium, no Electron.

## Layout

```
<name>/
├── App.tsx          # RN UI (native windows + backend RPC demo)
├── index.ts         # RN entry — registers the root component
├── backend/index.ts # Embedded Node.js backend (`backend.onInvoke` handlers)
├── kitron.config.ts # App/window/node configuration
└── macos/           # Xcode project (generated), Kitron pod, Rust staticlib
```

## Commands

```sh
kitron create my-app  # (already done if this folder was created for you)
kitron dev            # Rust build + Metro + launch the app with dev env
kitron build          # Release .app with the backend + Node runtime bundled
kitron package        # Wrap the Release .app into a .dmg
kitron run            # Launch the last-built app
kitron doctor         # Toolchain sanity checks
```

Metro hot-reloads the UI; backend changes restart the Node host (spec §29).

A packaged `.app` runs on its own — the JS bundle, the backend and the Node
runtime are embedded, so no Metro and no environment variables are needed.
