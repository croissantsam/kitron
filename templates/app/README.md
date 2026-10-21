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

The Kitron CLI ships with the app (`node_modules/.bin/kitron`), so no global
install is needed:

```sh
pnpm dev            # Rust build + Metro + launch the app with dev env
pnpm build          # Release .app with the backend + Node runtime bundled
pnpm start          # Metro only (the bundler)
pnpm doctor         # Toolchain sanity checks
```

The same commands are available through the CLI directly:

```sh
kitron create my-app  # (already done if this folder was created for you)
kitron dev
kitron build
kitron package        # Wrap the Release .app into a .dmg
kitron run            # Launch the last-built app
kitron doctor
```

Metro hot-reloads the UI; backend changes restart the Node host (spec §29).

A packaged `.app` runs on its own — the JS bundle, the backend and the Node
runtime are embedded, so no Metro and no environment variables are needed.
