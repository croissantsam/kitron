# Architecture

```
TanStack Start + Vite (React/TS)      ← default frontend, replaceable
        │ Kitron API (async, typed)
        ├──────────────┐
        ▼              ▼
   Node.js runtime   Rust bridge (windows, WebView, OS APIs)
        │              │
        └──── IPC ─────┘  (NDJSON: WebView↔Node, WebView↔Rust, Node↔Rust)
```

Golden rules:

1. Never bundle Chromium — WKWebView / WebView2 / WebKitGTK only.
2. Node.js first — fs, net, crypto, processes, npm stay in Node.
3. Rust only for native OS access — windows, WebView, menus, tray,
   notifications, dialogs, shortcuts.
4. TypeScript-first public API, framework-independent internally.
5. Renderer never gets `require` / `process` / `fs` — only `window.kitron`.
6. Async everywhere; typed `KitronError`s; permissions gated by config.

See `instruction.md` §§1–53 for the full spec.
