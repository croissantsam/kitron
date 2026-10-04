# Kitron

## React Native + Node.js + Rust Desktop Runtime

Kitron est un runtime desktop cross-platform destiné à créer des applications desktop modernes avec :

- React Native pour l'interface utilisateur
- TypeScript/JavaScript pour toute la logique applicative
- Node.js pour les capacités backend/runtime JavaScript
- Rust pour la couche native cross-platform
- APIs natives macOS et Windows

L'objectif est de proposer une alternative à Electron qui n'utilise pas Chromium.

---

# 1. Vision

Kitron doit permettre de construire une application desktop avec une expérience de développement proche de :

```text
React Native
+
TypeScript
+
Node.js
```

tout en donnant accès aux fonctionnalités natives de l'ordinateur :

```text
Windows
macOS
```

Architecture générale :

```text
┌───────────────────────────────────────────┐
│                KITRON APP                 │
│                                           │
│       React Native + TypeScript           │
│                                           │
└─────────────────────┬─────────────────────┘
                      │
                Native Bridge
                      │
                      ▼
             ┌────────────────┐
             │   Rust Core    │
             │                │
             │ Native Runtime │
             └───────┬────────┘
                     │
          ┌──────────┴──────────┐
          │                     │
       macOS                 Windows
      Native APIs           Native APIs
```

Node.js est intégré au runtime :

```text
                    Kitron
                      │
          ┌───────────┴───────────┐
          │                       │
    React Native               Node.js
          │                       │
          └──────────┬────────────┘
                     │
                Rust Core
                     │
              Native OS APIs
```

---

# 2. Objectif principal

Le développeur d'une application Kitron doit écrire principalement :

```text
TypeScript
TSX
JavaScript
```

Il ne doit pas avoir besoin d'écrire :

```text
Rust
Swift
Objective-C
C++
Win32
WinRT
```

La complexité native doit être encapsulée dans Kitron.

---

# 3. Philosophie d'architecture

Kitron ne doit PAS devenir un framework frontend.

Ne pas intégrer :

- Vite
- TanStack
- Next.js
- Chromium
- Electron
- un framework web
- HTML/CSS comme renderer principal

React Native est le renderer.

Kitron est le runtime desktop.

---

# 4. Responsabilités des composants

## 4.1 React Native

React Native est responsable de :

- UI
- rendu
- composants
- animations
- gestures
- interactions utilisateur
- état UI
- navigation
- accessibilité

Exemple :

```tsx
export function App() {
  return (
    <View>
      <Text>Hello Kitron</Text>
    </View>
  );
}
```

React Native doit utiliser son moteur de rendu natif.

Ne pas créer un renderer HTML.

---

# 5. TypeScript / JavaScript

TypeScript est le langage principal utilisé par les développeurs.

Il doit être utilisé pour :

- logique applicative
- services
- état
- API
- communication avec le backend local
- gestion des fichiers lorsque Node peut le faire
- logique métier
- plugins

Exemple :

```ts
import { window } from "@kitron/window";

const mainWindow = await window.create({
  width: 1200,
  height: 800,
});
```

---

# 6. Node.js

Node.js doit être embarqué dans Kitron.

Node sert notamment à fournir :

- npm
- filesystem
- HTTP
- WebSocket
- TCP
- processus
- workers
- crypto
- streams
- TypeScript runtime si nécessaire
- bibliothèques Node existantes

Exemples :

```ts
import fs from "node:fs/promises";

const files = await fs.readdir("./data");
```

ou :

```ts
import { spawn } from "node:child_process";

spawn("git", ["status"]);
```

Ne pas réimplémenter en Rust des fonctionnalités que Node fournit déjà correctement.

---

# 7. Rust

Rust est le cœur natif de Kitron.

Rust doit être utilisé lorsque JavaScript/Node/React Native doivent communiquer avec le système d'exploitation.

Rust doit notamment gérer :

- création de fenêtres
- gestion des fenêtres
- menus natifs
- tray
- notifications natives
- dialogs natifs
- raccourcis globaux
- deep links
- file associations
- intégration OS
- permissions natives
- lifecycle de l'application
- processus natifs lorsque nécessaire
- auto-update
- communication avec les APIs natives
- fonctionnalités cross-platform difficiles à abstraire en JS

Rust ne doit PAS contenir la logique métier de l'application.

---

# 8. Bridge JavaScript ↔ Rust

Le bridge est un composant central de Kitron.

Architecture :

```text
React Native
     │
     ▼
JavaScript / TypeScript
     │
     ▼
JSI / Native Module
     │
     ▼
Rust
     │
     ▼
Native OS
```

Éviter autant que possible un bridge JSON lent pour les opérations fréquentes.

Privilégier :

```text
JSI
Native Modules
Typed APIs
Async primitives
```

pour les appels haute fréquence.

---

# 9. API Kitron

L'API publique doit être simple et TypeScript-first.

Exemple :

```ts
import {
  app,
  window,
  dialog,
  clipboard,
  notification,
  shell,
  process,
} from "kitron";
```

---

# 10. Window API

Exemple :

```ts
const mainWindow = await window.create({
  width: 1200,
  height: 800,
  title: "My App",
});
```

API minimale :

```ts
window.create()
window.get()
window.close()
window.minimize()
window.maximize()
window.restore()
window.focus()
window.setSize()
window.setPosition()
window.show()
window.hide()
```

Supporter plusieurs fenêtres.

---

# 11. Application lifecycle

API :

```ts
app.ready()

app.quit()

app.hide()

app.show()

app.on("ready")

app.on("activate")

app.on("beforeQuit")

app.on("quit")
```

L'implémentation native doit gérer correctement le lifecycle différent de macOS et Windows.

---

# 12. Native dialogs

API :

```ts
const result = await dialog.openFile({
  multiple: true,
});
```

Supporter :

```text
Open file
Save file
Select directory
Message
Confirmation
Error
```

Les dialogs doivent utiliser les dialogs natifs de l'OS.

---

# 13. Clipboard

API :

```ts
await clipboard.writeText("Hello");

const text = await clipboard.readText();
```

Prévoir ultérieurement :

```text
text
HTML
image
files
```

---

# 14. Notifications

API :

```ts
await notification.show({
  title: "Build complete",
  body: "Your application is ready.",
});
```

Utiliser les APIs natives :

```text
macOS → UserNotifications
Windows → Windows notifications
```

---

# 15. Shell

API :

```ts
await shell.openExternal("https://example.com");
```

Autres fonctionnalités possibles :

```ts
shell.openPath()
shell.showItemInFolder()
```

---

# 16. Process management

Node doit être utilisé lorsque possible.

Exemple :

```ts
import { spawn } from "node:child_process";

const process = spawn("git", ["status"]);
```

Kitron peut néanmoins fournir une abstraction native lorsque Node n'est pas suffisant :

```ts
import { process } from "@kitron/process";

const child = await process.spawn({
  command: "git",
  args: ["status"],
});
```

L'API doit rester asynchrone.

---

# 17. Tray

API :

```ts
const tray = await tray.create({
  icon: "./icon.png",
  tooltip: "My App",
});
```

Supporter :

```text
Tray icon
Context menu
Click
Double click
Show/hide window
Quit
```

---

# 18. Menus

API :

```ts
const menu = await Menu.create([
  {
    label: "File",
    items: [
      {
        label: "New",
        action: "new-file",
      },
    ],
  },
]);
```

Les menus doivent être natifs.

---

# 19. Global shortcuts

API :

```ts
shortcut.register(
  "CommandOrControl+Shift+K",
  () => {
    // ...
  }
);
```

Rust doit gérer l'enregistrement natif du raccourci.

---

# 20. Deep links

Kitron doit permettre :

```text
myapp://open/project/123
```

L'application doit pouvoir recevoir le lien :

```ts
app.on("deepLink", (url) => {
  console.log(url);
});
```

---

# 21. File associations

Permettre à une application de déclarer :

```text
.md
.json
.myapp
```

et recevoir les fichiers ouverts par l'utilisateur.

Exemple :

```ts
app.on("fileOpen", (file) => {
  console.log(file.path);
});
```

---

# 22. Auto update

Kitron doit prévoir un système d'update natif.

Architecture :

```text
Application
    │
    ▼
Kitron updater
    │
    ▼
Check update
    │
    ▼
Download
    │
    ▼
Verify signature
    │
    ▼
Install
    │
    ▼
Restart
```

Les mises à jour doivent être cryptographiquement vérifiées.

---

# 23. Sécurité

Ne jamais donner automatiquement à une application toutes les permissions natives.

Les APIs sensibles doivent être explicites.

Exemples :

```text
Filesystem
Shell
Process
Network
Camera
Microphone
Notifications
```

Éviter une API du type :

```ts
native.executeAnything()
```

qui contournerait toute abstraction de sécurité.

---

# 24. Architecture des packages

Proposer une structure :

```text
@kitron/core
@kitron/window
@kitron/dialog
@kitron/menu
@kitron/tray
@kitron/clipboard
@kitron/notification
@kitron/process
@kitron/shell
@kitron/shortcut
@kitron/updater
```

Un package doit exposer une API TypeScript.

Exemple :

```text
@kitron/window
       │
       ▼
TypeScript API
       │
       ▼
JSI / Native Module
       │
       ▼
Rust
       │
       ▼
OS
```

---

# 25. Support macOS

Première plateforme à supporter.

Le runtime doit utiliser les APIs natives macOS lorsque nécessaire :

```text
AppKit
Foundation
UserNotifications
NSWorkspace
NSPasteboard
NSWindow
NSStatusItem
```

Rust doit communiquer avec ces APIs via une couche native appropriée.

Ne pas simuler les fonctionnalités macOS en JavaScript.

---

# 26. Support Windows

Deuxième plateforme.

Utiliser les APIs natives Windows nécessaires :

```text
Win32
WinRT
Windows App SDK
Shell APIs
Notification APIs
```

Le développeur ne doit pas connaître ces APIs.

---

# 27. React Native Desktop

React Native doit être utilisé comme moteur UI.

Il faut déterminer la meilleure stratégie pour le desktop :

```text
React Native Fabric
+
Native Components
+
JSI
```

Le projet doit éviter de réinventer le renderer React Native.

L'objectif est de réutiliser au maximum l'écosystème React Native existant.

---

# 28. Node.js + React Native

Node.js et React Native sont deux environnements JavaScript différents.

Ils doivent pouvoir communiquer proprement.

Architecture :

```text
React Native JS
       │
       │
       ▼
   Kitron Runtime
       │
       ▼
   Node.js Runtime
```

Prévoir une API :

```ts
node.start()
node.stop()
node.restart()
node.status()
```

Exemple :

```ts
await node.start({
  entry: "./backend/index.ts",
});
```

---

# 29. Backend local

Une application peut embarquer son propre backend :

```text
my-app/
├── src/
│   ├── App.tsx
│   └── ...
│
├── backend/
│   └── index.ts
│
└── package.json
```

Kitron démarre Node automatiquement.

```text
Application start
       │
       ▼
React Native
       │
       └──── Node backend
```

Le backend peut fournir :

```text
HTTP
WebSocket
RPC
Database
Workers
File processing
AI
```

---

# 30. Communication React Native ↔ Node

Éviter de forcer toutes les communications à passer par HTTP localhost.

Prévoir un mécanisme IPC/RPC natif :

```text
React Native
      │
      ▼
Kitron IPC
      │
      ▼
Node.js
```

API possible :

```ts
const result = await backend.invoke("users.get", {
  id: 123,
});
```

Le système doit utiliser des transports efficaces.

---

# 31. Performance

La performance est un objectif majeur.

Principes :

1. Éviter Chromium.
2. Utiliser React Native pour le rendu.
3. Utiliser JSI pour les appels haute fréquence.
4. Éviter les sérialisations JSON inutiles.
5. Éviter les copies mémoire inutiles.
6. Utiliser Rust pour les opérations natives lourdes.
7. Utiliser Node pour les APIs qu'il gère déjà efficacement.
8. Garder les communications IPC asynchrones.
9. Éviter de bloquer le thread UI.
10. Utiliser des workers pour les tâches lourdes.

---

# 32. Architecture des threads

Ne jamais bloquer le thread UI.

Architecture cible :

```text
UI Thread
    │
    ├── React Native
    │
    └── User interaction

Rust
    │
    ├── Native operations
    ├── IO
    └── background tasks

Node
    │
    ├── Backend
    ├── Workers
    └── IO
```

---

# 33. Gestion des erreurs

Toutes les APIs doivent avoir des erreurs typées.

Exemple :

```ts
try {
  await window.create();
} catch (error) {
  if (error.code === "WINDOW_CREATION_FAILED") {
    // ...
  }
}
```

Ne jamais retourner uniquement :

```ts
throw new Error("Something went wrong");
```

Préférer :

```ts
{
  code: "WINDOW_CREATION_FAILED",
  message: "...",
  platform: "macos"
}
```

---

# 34. Logs

Kitron doit fournir :

```ts
logger.info()
logger.warn()
logger.error()
logger.debug()
```

Les logs Rust et Node doivent pouvoir être centralisés.

Exemple :

```text
Kitron
 ├── React Native logs
 ├── Node logs
 └── Rust logs
```

---

# 35. CLI

Kitron doit avoir un CLI.

Exemples :

```bash
kitron create my-app
```

```bash
kitron dev
```

```bash
kitron build
```

```bash
kitron package
```

```bash
kitron run
```

```bash
kitron doctor
```

---

# 36. Création d'une application

Commande :

```bash
kitron create my-app
```

Résultat :

```text
my-app/
├── src/
│   ├── App.tsx
│   └── ...
│
├── backend/
│   └── index.ts
│
├── assets/
│
├── package.json
└── kitron.config.ts
```

Le développeur doit pouvoir immédiatement :

```bash
cd my-app
kitron dev
```

---

# 37. Configuration

Exemple :

```ts
export default {
  app: {
    name: "My App",
    identifier: "com.example.myapp",
    version: "1.0.0",
  },

  window: {
    width: 1200,
    height: 800,
  },

  node: {
    entry: "./backend/index.ts",
  },
};
```

---

# 38. Build

Commande :

```bash
kitron build
```

Doit produire une application native :

macOS :

```text
MyApp.app
```

Windows :

```text
MyApp.exe
```

Plus tard :

```text
.dmg
.pkg
.msi
.exe
```

---

# 39. Distribution

Prévoir :

```text
macOS
├── .app
├── .dmg
└── notarization

Windows
├── .exe
├── installer
└── code signing
```

Le système de packaging doit être intégré au CLI.

---

# 40. Structure du repository Kitron

Proposer :

```text
kitron/
│
├── packages/
│   ├── core/
│   ├── window/
│   ├── dialog/
│   ├── menu/
│   ├── tray/
│   ├── clipboard/
│   ├── notification/
│   ├── process/
│   ├── shell/
│   ├── shortcut/
│   ├── updater/
│   └── node/
│
├── runtime/
│   ├── rust/
│   ├── macos/
│   └── windows/
│
├── cli/
│
├── templates/
│
├── examples/
│
├── tests/
│
└── docs/
```

---

# 41. MVP

Ne pas essayer de construire toutes les fonctionnalités immédiatement.

Le MVP doit uniquement permettre :

```text
1. React Native fonctionne
2. Une fenêtre desktop native fonctionne
3. TypeScript fonctionne
4. JSI fonctionne
5. Rust fonctionne
6. Node.js peut être lancé
7. React Native peut communiquer avec Rust
8. React Native peut communiquer avec Node
9. macOS fonctionne
10. Une application peut être packagée
```

Premier exemple :

```ts
import { window } from "kitron";

const mainWindow = await window.create({
  width: 1200,
  height: 800,
});
```

Si cela fonctionne, Kitron possède son premier vrai vertical slice.

---

# 42. Phase 2

Ajouter :

```text
Filesystem
Dialog
Clipboard
Shell
Process
Notification
Menu
Tray
```

---

# 43. Phase 3

Ajouter :

```text
Node backend
IPC
Workers
Deep links
File associations
Global shortcuts
Auto updater
```

---

# 44. Phase 4

Ajouter :

```text
Windows
Code signing
Installers
Production updater
Crash reporting
Security hardening
Performance profiling
```

---

# 45. Règles absolues

## Ne pas faire

Ne pas :

- intégrer Chromium
- utiliser Electron
- utiliser Vite comme dépendance du runtime
- utiliser TanStack comme dépendance du runtime
- transformer Kitron en framework web
- réimplémenter Node en Rust
- mettre la logique métier dans Rust
- bloquer le thread React Native
- exposer directement les APIs OS au développeur

## Faire

Toujours :

- privilégier React Native
- privilégier TypeScript
- réutiliser Node.js
- utiliser Rust pour la couche native
- utiliser JSI lorsque nécessaire
- conserver une API TypeScript simple
- abstraire macOS et Windows
- garder les APIs natives derrière Kitron
- mesurer les performances
- éviter les copies de données
- garder le runtime minimal

---

# 46. Principe final

Kitron doit être perçu comme :

```text
             Electron
                 │
        Chromium + Node
                 │
                 ▼

              KITRON
                 │
     ┌───────────┼───────────┐
     │           │           │
React Native   Node        Rust
     │           │           │
     └───────────┼───────────┘
                 │
                 ▼
            Native OS
```

La promesse :

> **Build desktop applications with React Native and TypeScript, powered by Node.js and native Rust infrastructure — without Chromium.**

Le développeur écrit principalement :

```text
React
React Native
TypeScript
Node.js
```

Kitron s'occupe de :

```text
Rust
macOS
Windows
Native APIs
Packaging
IPC
Runtime
```

L'objectif ultime est d'obtenir une expérience développeur aussi simple qu'Electron, mais avec une architecture beaucoup plus proche du natif et sans embarquer Chromium.