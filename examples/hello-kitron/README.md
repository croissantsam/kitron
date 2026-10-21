# hello-kitron — Kitron example app

Demonstrates the three runtime surfaces of a Kitron app:

| Surface            | Transport                | What it shows                                  |
|--------------------|--------------------------|------------------------------------------------|
| `app`              | JSI → Rust               | process identity, lifecycle events              |
| `backend`          | typed IPC → Node.js      | `backend.invoke`, `backend.emit`                |
| `window`           | Rust → AppKit            | native multi-window management                  |
| `clipboard`/`shell`| Rust → AppKit            | phase-2 native APIs, permission-gated (§23)     |

## Run it

The example ships sources only — the Xcode project, Pods and the Rust runtime
are generated. Materialise a runnable app with:

```bash
bash examples/hello-kitron/scaffold.sh
cd examples/.hello-kitron-app
node <kitron>/cli/src/kitron.ts dev
```

`scaffold.sh` runs `kitron create` (which installs dependencies, generates the
Xcode project and runs `pod install`) and then overlays this example's
`App.tsx`, `backend/` and `kitron.config.ts` on the template.

## What to look for

* The window shows `app.info` (JSI), the result of `math.add` (IPC) and the
  live window count.
* **Open a window** creates a second native `NSWindow` through the Rust core.
* **Run report** streams `report.progress` events from the Node backend to the
  UI.
* **Copy greeting** round-trips the system clipboard (`NSPasteboard`), and
  **Open the docs** hands a URL to the OS handler (`NSWorkspace`) — both need
  the matching entry in `permissions` (§23).
* The app log (`kitron dev`) shows the unified log stream:

  ```text
  [kitron:app] [info] app.info {"pid":…,"name":"HelloKitronExample",…}
  [kitron:app] [info] backend.invoke result {"sum":42}
  ```

## Files

```text
examples/hello-kitron/
├── App.tsx           # React Native UI
├── backend/index.ts  # embedded Node.js backend (handlers + events)
├── kitron.config.ts  # app config (identity, window, node entry)
├── package.json
└── scaffold.sh       # materialises a runnable app
```
