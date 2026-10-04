# Kitron

> **A cross-platform desktop runtime for JavaScript and TypeScript, powered by Node.js, native WebViews and a minimal Rust bridge.**

## 1. Vision

Kitron est une alternative moderne à Electron pour créer des applications desktop avec les technologies web.

Kitron doit permettre de développer une application desktop principalement en :

- TypeScript
- JavaScript
- React
- TanStack Start
- HTML
- CSS
- Node.js

sans demander au développeur d'écrire du code natif.

L'objectif architectural est :

```text
Electron

JavaScript / TypeScript
        │
        ▼
     Node.js
        │
        ▼
    Chromium
```

contre :

```text
Kitron

JavaScript / TypeScript
        │
        ├──────────────────────┐
        ▼                      ▼
    Node.js                Rust Bridge
        │                      │
        │                      ▼
        │                 Native APIs
        │                      │
        │                 Native WebView
        │
        └──────────── IPC ─────┘
```

Kitron ne doit **pas embarquer Chromium**.

---

# 2. Objectifs

Kitron doit être :

- multiplateforme
- TypeScript-first
- Node.js-first
- Web-standard
- léger
- performant
- sécurisé
- extensible
- simple à utiliser
- proche d'Electron dans son expérience développeur

Les plateformes cibles sont :

```text
macOS
Windows
Linux
```

---

# 3. Architecture fondamentale

Architecture globale :

```text
┌─────────────────────────────────────────────────────┐
│                    KITRON APP                       │
│                                                     │
│  ┌───────────────────────────────────────────────┐  │
│  │           TanStack Start + Vite               │  │
│  │                                               │  │
│  │             React / TypeScript                │  │
│  │                                               │  │
│  │              Web Application                  │  │
│  └───────────────────────┬───────────────────────┘  │
│                          │                          │
│                     Kitron API                     │
│                          │                          │
│             ┌────────────┴────────────┐             │
│             │                         │             │
│             ▼                         ▼             │
│        Node.js Runtime          Rust Bridge         │
│             │                         │             │
│             │                  Native OS APIs       │
│             │                         │             │
│             │                         ▼             │
│             │                  Native WebView       │
│             │                                      │
│             └──────────── IPC ─────────────────────┘
└─────────────────────────────────────────────────────┘
```

---

# 4. Responsabilités des couches

## 4.1 Frontend

Le frontend est responsable de :

- UI
- routing
- état de l'application
- interactions utilisateur
- rendu
- logique frontend
- appels vers Kitron

Stack par défaut :

```text
TanStack Start
React
TypeScript
Vite
```

---

## 4.2 Node.js

Node.js est le runtime backend principal.

Il doit être utilisé pour toutes les fonctionnalités que Node.js sait déjà gérer.

Exemples :

```text
Filesystem
Networking
HTTP
TCP
UDP
Crypto
Streams
Child processes
File watching
Environment variables
Process management
npm packages
Databases
CLI tools
```

Règle fondamentale :

> **Si Node.js peut faire correctement quelque chose, Kitron doit utiliser Node.js plutôt que Rust.**

Exemple :

```ts
import fs from "node:fs/promises";

const content = await fs.readFile(
  "./notes.md",
  "utf8"
);
```

Rust n'intervient pas.

---

# 5. Rust Bridge

Rust est une couche native minimale.

Rust ne doit pas devenir le backend de Kitron.

Rust sert uniquement à :

1. créer et gérer les fenêtres natives
2. créer et gérer les WebViews
3. accéder aux APIs natives du système
4. gérer certaines fonctionnalités desktop
5. transporter les messages IPC
6. faire le lien entre Node.js et le système d'exploitation

Exemples :

```text
Native Window
Native WebView
Native Menu
System Tray
Notifications
Global Shortcuts
Native Dialogs
Screen Management
Window Management
OS Permissions
Clipboard integration
Drag & Drop
Power Management
```

---

# 6. Principe Native Bridge

Le développeur ne doit jamais avoir besoin d'écrire directement du code natif.

Exemple :

```ts
await native.notification.show({
  title: "Kitron",
  body: "Build completed",
});
```

Flux :

```text
TypeScript
    │
    ▼
Kitron API
    │
    ▼
IPC
    │
    ▼
Rust
    │
    ▼
Native OS API
```

---

# 7. Plateformes natives

Kitron doit utiliser les WebViews natives de chaque système.

## macOS

```text
Rust
  ↓
WKWebView
  ↓
WebKit
```

Les APIs macOS doivent être accessibles via Rust bindings vers les frameworks Apple lorsque nécessaire.

Exemples :

```text
AppKit
Foundation
WebKit
UserNotifications
CoreGraphics
```

---

## Windows

```text
Rust
  ↓
WebView2
  ↓
Microsoft Edge WebView2 Runtime
```

Kitron ne doit pas embarquer une seconde version de Chromium.

WebView2 doit être utilisé comme composant fourni par l'environnement Windows.

---

## Linux

```text
Rust
  ↓
WebKitGTK
  ↓
WebKit
```

Le backend Linux doit prendre en compte :

```text
Wayland
X11
GTK
```

---

# 8. WebView abstraction

Le reste de Kitron ne doit jamais dépendre directement de :

```text
WKWebView
WebView2
WebKitGTK
```

Créer une abstraction commune.

Conceptuellement :

```rust
trait WebView {
    fn load_url(&self, url: &str);
    fn load_file(&self, path: &str);
    fn reload(&self);
    fn stop(&self);
    fn close(&self);
}
```

Implémentations :

```text
native/
├── macos/
├── windows/
└── linux/
```

---

# 9. Process architecture

Kitron doit séparer le renderer et Node.js.

Architecture :

```text
                    Kitron Host
                        │
             ┌──────────┴──────────┐
             │                     │
             ▼                     ▼
        Native WebView          Node.js
             │                     │
             ▼                     ▼
       Renderer JS            Backend JS
```

Le renderer ne doit pas avoir accès directement à Node.js.

Le backend Node.js ne doit pas être injecté directement dans le renderer.

---

# 10. IPC

L'IPC est le mécanisme central de communication.

Deux communications principales existent :

```text
WebView ↔ Node.js
WebView ↔ Rust
```

Et éventuellement :

```text
Node.js ↔ Rust
```

Le protocole doit être commun et typé.

---

# 11. IPC protocol

Structure minimale :

```ts
interface IPCRequest {
  id: string;
  channel: string;
  payload: unknown;
}

interface IPCResponse {
  id: string;
  success: boolean;
  payload?: unknown;
  error?: IPCError;
}

interface IPCEvent {
  channel: string;
  payload: unknown;
}

interface IPCError {
  code: string;
  message: string;
  details?: unknown;
}
```

Toutes les communications doivent être asynchrones.

---

# 12. Node.js IPC

Une API Node.js peut être enregistrée :

```ts
import { ipc } from "kitron/node";

ipc.handle(
  "app.getVersion",
  async () => {
    return process.env.npm_package_version;
  }
);
```

Le frontend :

```ts
const version = await window.kitron.invoke(
  "app.getVersion"
);
```

---

# 13. Native IPC

Les APIs natives suivent le même principe.

Frontend :

```ts
await native.window.setAlwaysOnTop(true);
```

Flux :

```text
Frontend
   ↓
Kitron JS API
   ↓
IPC
   ↓
Rust
   ↓
OS
```

---

# 14. API publique

Kitron doit exposer une API TypeScript simple.

Exemple :

```ts
import {
  app,
  BrowserWindow,
  native,
} from "kitron";
```

L'API doit être :

- async-first
- fortement typée
- documentée
- stable
- indépendante du framework frontend

---

# 15. App API

```ts
await app.ready();

app.quit();

app.exit(0);

app.getName();

app.getVersion();

app.getPath("userData");

app.getPath("documents");

app.getPath("downloads");

app.getPath("temp");

app.isReady();
```

---

# 16. BrowserWindow

Exemple :

```ts
const window = new BrowserWindow({
  width: 1200,
  height: 800,
});
```

API initiale :

```ts
window.loadURL(url);

window.loadFile(path);

window.show();

window.hide();

window.close();

window.focus();

window.minimize();

window.maximize();

window.restore();

window.setTitle(title);

window.setSize(width, height);

window.setPosition(x, y);

window.setAlwaysOnTop(true);

window.isMaximized();
```

---

# 17. Native API

Les fonctionnalités natives doivent être regroupées dans :

```ts
native
```

Exemple :

```ts
import { native } from "kitron";

await native.notification.show({
  title: "Kitron",
  body: "Hello World",
});
```

API initiale :

```text
native.window
native.notification
native.dialog
native.menu
native.tray
native.clipboard
native.screen
native.shortcut
native.power
```

---

# 18. Filesystem

Kitron ne doit pas réimplémenter inutilement le filesystem.

Utiliser Node.js :

```ts
import fs from "node:fs/promises";

await fs.readFile(path);

await fs.writeFile(path, content);

await fs.readdir(path);

await fs.mkdir(path);
```

Kitron peut ajouter des APIs ergonomiques au-dessus de Node.js uniquement lorsqu'elles apportent une réelle valeur.

---

# 19. Child Processes

Utiliser Node.js :

```ts
import { spawn } from "node:child_process";

const process = spawn(
  "git",
  ["status"]
);
```

Kitron ne doit pas remplacer `child_process`.

Une API optionnelle peut être proposée :

```ts
await shell.exec("git status");
```

mais Node.js doit rester accessible.

---

# 20. Security model

Le renderer ne doit jamais obtenir directement :

```ts
window.require
window.process
window.fs
window.node
```

Interdit :

```ts
window.node = process;
```

Préférer :

```ts
window.kitron = {
  invoke,
  native,
};
```

Le bridge doit appliquer une whitelist d'APIs.

---

# 21. Permissions

Les capacités sensibles doivent être configurables.

Exemple :

```ts
export default {
  permissions: {
    filesystem: true,
    childProcess: true,
    notifications: true,
    globalShortcuts: false,
  },
};
```

Les permissions doivent être vérifiées avant l'exécution d'une opération sensible.

---

# 22. Default frontend stack

**TanStack Start + Vite est le stack frontend par défaut de Kitron.**

Une nouvelle application :

```bash
pnpm create kitron my-app
```

doit être initialisée avec :

```text
TanStack Start
React
TypeScript
Vite
```

---

# 23. Pourquoi TanStack Start

TanStack Start fournit par défaut :

- TanStack Router
- routing type-safe
- React
- TypeScript
- Vite
- code splitting
- conventions modernes
- server functions
- architecture adaptée aux applications complexes

Kitron doit profiter de cet écosystème sans coupler son runtime desktop à TanStack Start.

---

# 24. Vite

Vite est le bundler et dev server officiel de Kitron.

En développement :

```text
kitron dev
     │
     ├── Vite
     │    └── TanStack Start
     │
     ├── Node.js
     │
     └── Rust Host
          └── Native WebView
```

Le WebView charge l'application depuis Vite.

Exemple :

```text
http://localhost:5173
```

Le port doit être configurable et pouvoir être automatiquement choisi.

---

# 25. Framework independence

TanStack Start est le **default**, mais Kitron ne doit pas dépendre de TanStack.

Kitron doit pouvoir supporter :

```text
TanStack Start ← default
React + Vite
Vue + Vite
Svelte + Vite
Vanilla + Vite
```

L'API Kitron doit rester indépendante.

Cette API :

```ts
await native.notification.show(...)
```

doit fonctionner avec n'importe quel frontend.

---

# 26. Application structure

Application par défaut :

```text
my-app/
├── src/
│   ├── routes/
│   │   ├── __root.tsx
│   │   └── index.tsx
│   │
│   ├── components/
│   ├── lib/
│   └── main.tsx
│
├── public/
│
├── app.config.ts
├── vite.config.ts
├── kitron.config.ts
├── tsconfig.json
└── package.json
```

Kitron doit respecter autant que possible les conventions standards de TanStack Start.

---

# 27. Development mode

Commande :

```bash
kitron dev
```

Pipeline :

```text
kitron dev
     │
     ├── Start Vite
     │
     ├── Start TanStack Start
     │
     ├── Start Node.js
     │
     └── Start Rust Host
              │
              └── Create Native WebView
```

Le hot reload doit fonctionner normalement.

---

# 28. Production

Commande :

```bash
kitron build
```

Pipeline :

```text
TanStack Start
       ↓
Vite build
       ↓
Web assets
       ↓
Node backend
       ↓
Rust native host
       ↓
Application bundle
```

---

# 29. Node.js distribution

Kitron doit embarquer son propre Node.js en production.

Une application ne doit jamais dépendre du Node installé par l'utilisateur.

Exemple :

```text
MyApp.app
└── Resources
    ├── node/
    │   └── node
    │
    ├── app/
    │
    └── web/
```

---

# 30. Cross-platform Node.js

Le runtime doit être construit pour :

```text
macOS arm64
macOS x64

Windows x64
Windows arm64

Linux x64
Linux arm64
```

Le code TypeScript de l'application reste identique.

---

# 31. CLI

CLI principal :

```bash
kitron create my-app

kitron dev

kitron build

kitron package

kitron run

kitron doctor
```

---

# 32. Project creation

Commande recommandée :

```bash
pnpm create kitron my-app
```

Par défaut :

```text
TanStack Start
React
TypeScript
Vite
Kitron
```

L'utilisateur peut ensuite choisir un autre frontend si nécessaire.

---

# 33. Repository structure

Kitron doit être organisé comme un monorepo pnpm.

```text
kitron/
├── packages/
│   ├── core/
│   ├── cli/
│   ├── ipc/
│   ├── runtime/
│   ├── types/
│   ├── native-api/
│   └── vite-plugin/
│
├── native/
│   └── rust/
│       └── kitron-host/
│
├── examples/
│   ├── tanstack-start/
│   ├── react/
│   ├── vue/
│   ├── svelte/
│   └── vanilla/
│
├── tests/
│   ├── unit/
│   ├── integration/
│   └── e2e/
│
├── docs/
│
├── package.json
├── pnpm-workspace.yaml
├── tsconfig.json
└── instruction.md
```

---

# 34. Rust repository structure

```text
native/rust/kitron-host/

src/
├── main.rs
├── lib.rs
├── ipc.rs
├── window.rs
├── webview.rs
├── menu.rs
├── notification.rs
├── tray.rs
└── platform/
    ├── mod.rs
    ├── macos.rs
    ├── windows.rs
    └── linux.rs
```

Le code spécifique aux plateformes doit être isolé.

---

# 35. Rust dependency philosophy

Le Rust Bridge doit rester petit.

Éviter d'utiliser Rust pour :

- business logic
- database
- networking applicatif
- filesystem applicatif
- routing
- configuration applicative
- frontend
- Node package management

Ces responsabilités appartiennent à JavaScript/Node.js.

Rust doit rester concentré sur :

```text
Native Window
Native WebView
Native OS APIs
Native IPC
```

---

# 36. Native abstraction

Une fonctionnalité native doit suivre :

```text
TypeScript API
       ↓
IPC
       ↓
Rust abstraction
       ↓
Platform implementation
```

Exemple :

```text
native.notification.show()
       ↓
notification.show
       ↓
NotificationService
       ↓
┌───────────────┬───────────────┬───────────────┐
│ macOS         │ Windows       │ Linux         │
│ UserNotif.    │ WinRT         │ Desktop       │
└───────────────┴───────────────┴───────────────┘
```

---

# 37. Native API rule

Une fonctionnalité doit être implémentée en Rust uniquement lorsqu'elle nécessite réellement une API native.

Exemples :

```text
Read file
→ Node.js

Write file
→ Node.js

HTTP request
→ Node.js

Spawn process
→ Node.js

WebSocket
→ Node.js

Create native window
→ Rust

Create WebView
→ Rust

Native notification
→ Rust

Native menu
→ Rust

System tray
→ Rust

Global shortcut
→ Rust
```

Cette règle est fondamentale.

---

# 38. Performance

Kitron doit viser une empreinte inférieure à Electron lorsque cela est raisonnablement possible.

Les benchmarks doivent être reproductibles.

Mesurer :

```text
Startup time
Cold startup
Warm startup
RAM usage
CPU usage
WebView creation
Node startup
IPC latency
IPC throughput
Application size
```

RAM :

```text
Rust Host
Node.js
WebView
Total
```

---

# 39. No false performance claims

Ne jamais déclarer :

> Kitron est 50 % plus rapide qu'Electron.

sans benchmark reproductible.

Les benchmarks doivent comparer :

```text
Même application
Même machine
Même fonctionnalité
Même environnement
Même scénario
```

---

# 40. Developer Experience

Le développeur doit pouvoir créer une application avec :

```bash
pnpm create kitron my-app

cd my-app

pnpm dev
```

Puis :

```bash
pnpm build
```

Il ne doit pas avoir à écrire de Rust pour utiliser les fonctionnalités standards de Kitron.

---

# 41. Example application

Exemple :

```ts
import {
  app,
  BrowserWindow,
  native,
} from "kitron";

await app.ready();

const window = new BrowserWindow({
  width: 1200,
  height: 800,
  title: "My App",
});

await window.loadURL(
  "http://localhost:5173"
);

await native.notification.show({
  title: "My App",
  body: "Application started",
});
```

---

# 42. DevTools

Kitron doit exposer les outils de développement natifs du moteur utilisé.

```text
macOS
→ WebKit Web Inspector

Windows
→ WebView2 DevTools

Linux
→ WebKit Inspector
```

API :

```ts
window.openDevTools();
window.closeDevTools();
window.reload();
```

---

# 43. Logging

Kitron doit centraliser les logs provenant de :

```text
Renderer
Node.js
Rust
```

Exemple :

```text
[Kitron][Renderer] Application started
[Kitron][Node] Backend started
[Kitron][Rust] WebView created
```

API :

```ts
logger.debug();
logger.info();
logger.warn();
logger.error();
```

---

# 44. Error handling

Toutes les erreurs Kitron doivent être typées.

```ts
class KitronError extends Error {
  code: string;
  details?: unknown;
}
```

Codes minimum :

```text
NODE_START_FAILED
NODE_CRASHED
IPC_FAILED
IPC_TIMEOUT
WEBVIEW_FAILED
WINDOW_FAILED
NATIVE_API_FAILED
PERMISSION_DENIED
PLATFORM_UNSUPPORTED
BUILD_FAILED
```

---

# 45. Plugins

Kitron doit prévoir un système de plugins.

Exemple :

```ts
import { definePlugin } from "kitron";

export default definePlugin({
  name: "my-plugin",

  setup(api) {
    api.command("hello", async () => {
      return "Hello";
    });
  },
});
```

Un plugin peut éventuellement posséder une partie Rust pour accéder à une fonctionnalité native spécifique.

Mais l'API publique du plugin doit rester accessible depuis TypeScript.

---

# 46. Auto Update

Prévoir une API :

```ts
import { updater } from "kitron";

await updater.check();

await updater.download();

await updater.install();
```

L'implémentation peut utiliser des mécanismes différents selon la plateforme.

---

# 47. Packaging

Kitron doit produire :

## macOS

```text
.app
.dmg
```

## Windows

```text
.exe
.msi
```

## Linux

```text
.AppImage
.deb
.rpm
```

Le packaging doit inclure :

```text
Rust host
Node.js runtime
Application backend
Frontend assets
Resources
```

mais **pas Chromium**.

---

# 48. Signing

Le système de build doit prévoir :

```text
macOS code signing
macOS notarization

Windows code signing

Linux package signing
```

Ces fonctionnalités doivent être intégrées progressivement.

---

# 49. Tests

Tests TypeScript :

```text
Core API
IPC
CLI
Configuration
```

Tests Node.js :

```text
Runtime
Process management
Filesystem
Networking
```

Tests Rust :

```text
WebView
Window
Native APIs
IPC
```

Tests E2E :

```text
TanStack Start
        ↓
Kitron
        ↓
Node.js
        ↓
Rust
        ↓
Native OS
```

---

# 50. First milestone

La première milestone doit permettre :

```bash
pnpm create kitron my-app
cd my-app
pnpm dev
```

Résultat :

```text
TanStack Start
      ↓
Vite
      ↓
Native WebView
      ↓
Kitron API
      ↓
Node.js
```

L'application doit pouvoir :

### Afficher une UI

```tsx
export default function App() {
  return <h1>Hello Kitron</h1>;
}
```

### Appeler Node.js

```ts
const result = await window.kitron.invoke("ping");
```

Résultat :

```text
pong
```

### Appeler le système natif

```ts
await native.notification.show({
  title: "Kitron",
  body: "Hello",
});
```

### Contrôler une fenêtre

```ts
await native.window.setAlwaysOnTop(true);
```

---

# 51. MVP Definition of Done

## Runtime

- [ ] Node.js runtime embarqué
- [ ] Node.js process lifecycle
- [ ] Node.js crash handling
- [ ] IPC Node.js ↔ WebView
- [ ] IPC Rust ↔ WebView
- [ ] IPC Node.js ↔ Rust

## WebView

- [ ] macOS WKWebView
- [ ] Windows WebView2
- [ ] Linux WebKitGTK
- [ ] Navigation
- [ ] Reload
- [ ] DevTools
- [ ] JavaScript execution

## Window

- [ ] Create
- [ ] Close
- [ ] Show
- [ ] Hide
- [ ] Resize
- [ ] Move
- [ ] Minimize
- [ ] Maximize
- [ ] Always on top
- [ ] Title

## Native APIs

- [ ] Notifications
- [ ] Dialogs
- [ ] Menus
- [ ] Tray
- [ ] Clipboard
- [ ] Global shortcuts

## Developer experience

- [ ] `kitron create`
- [ ] `kitron dev`
- [ ] `kitron build`
- [ ] `kitron package`
- [ ] TanStack Start template
- [ ] Vite integration
- [ ] React template
- [ ] TypeScript types

## Distribution

- [ ] macOS `.app`
- [ ] macOS `.dmg`
- [ ] Windows `.exe`
- [ ] Windows `.msi`
- [ ] Linux `.AppImage`
- [ ] Linux `.deb`
- [ ] Linux `.rpm`

---

# 52. Long-term architecture

L'objectif final est :

```text
                       KITRON
                          │
             ┌────────────┴────────────┐
             │                         │
       Web Application             Node.js
             │                         │
   TanStack Start / Vite        npm ecosystem
             │                         │
             └──────────┬──────────────┘
                        │
                       IPC
                        │
                 Rust Bridge
                        │
          ┌─────────────┼─────────────┐
          │             │             │
        macOS        Windows         Linux
          │             │             │
      WKWebView       WebView2     WebKitGTK
```

Kitron doit conserver cette séparation :

```text
JavaScript
    ↓
Application logic
    ↓
Node.js
    ↓
Native capability when necessary
    ↓
Rust
    ↓
Operating System
```

---

# 53. Golden Rules

Les règles suivantes sont prioritaires.

### Rule 1

> **Never bundle Chromium.**

### Rule 2

> **Use Node.js when Node.js already provides the required capability.**

### Rule 3

> **Use Rust only when native OS access is required.**

### Rule 4

> **Keep the public API TypeScript-first.**

### Rule 5

> **Keep TanStack Start + Vite as the default application stack.**

### Rule 6

> **Keep Kitron independent from React and TanStack internally.**

### Rule 7

> **Never expose unrestricted Node.js or native APIs to the renderer.**

### Rule 8

> **Keep the Rust bridge as small as possible.**

### Rule 9

> **Provide the same TypeScript API on macOS, Windows and Linux.**

### Rule 10

> **Prefer platform-native capabilities over reimplementing the operating system in JavaScript.**

---

# 54. Final positioning

Kitron doit être présenté comme :

> **Kitron — A lightweight Electron alternative powered by Node.js, native WebViews and Rust.**

Ou :

> **Kitron — Desktop apps with TypeScript, Node.js and native WebViews.**

Conceptuellement :

```text
Electron
    =
Node.js
+
Chromium
+
C++

Kitron
    =
Node.js
+
Native WebView
+
Rust
```

Le principal avantage architectural recherché est de conserver l'écosystème et la flexibilité de Node.js tout en utilisant le moteur WebView fourni par chaque plateforme, plutôt que de distribuer un navigateur Chromium complet avec chaque application.

Le projet doit rester **JavaScript/TypeScript-first** du point de vue du développeur d'applications, avec Rust limité à la couche nécessaire pour connecter le runtime JavaScript aux capacités natives de macOS, Windows et Linux.