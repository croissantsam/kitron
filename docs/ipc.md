# IPC protocol

Frames are NDJSON (`{...}\n`) on stdio (Node↔Rust) and string messages
on the WebView bridge.

```ts
interface IPCRequest { id: string; channel: string; payload: unknown; }
interface IPCResponse { id: string; success: boolean; payload?: unknown; error?: IPCError; }
interface IPCEvent { channel: string; payload: unknown; }
interface IPCError { code: string; message: string; details?: unknown; }
```

Channels: `<domain>.<action>` — `app.*`, `window.*`, `native.*`,
`plugin.<name>.*`, `updater.*`, `ping`, `log`.

Node registration:

```ts
import { ipc } from "@kitron/core/node";
ipc.handle("app.getVersion", async () => "1.0.0");
```

Renderer (whitelisted only):

```ts
const version = await window.kitron.invoke("app.getVersion");
```

Timeouts default to 30s; aborts supported via `AbortSignal`.
