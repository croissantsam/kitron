# Kitron

> Build desktop applications with JavaScript, Node.js and native WebViews — without bundling Chromium.

```
Electron            Kitron
JavaScript    →     JavaScript
   ↓                   ↓
Node.js             Node.js ─┬─ Node APIs (fs, net, crypto, processes, npm)
                          └─ IPC → Rust Native Bridge → Native WebView
```

- **JavaScript-first** : React / Vue / Svelte / Vanilla, jamais de Rust à écrire.
- **Node.js-first** : filesystem, networking, child processes via Node. Pas de doublon Rust.
- **Rust = Native Bridge uniquement** : windows, WebView, menus, tray, notifications, dialogs, shortcuts.
- **Native WebView** : WKWebView (macOS), WebView2 (Windows), WebKitGTK (Linux).
- **API multiplateforme** : même TypeScript partout.

## Monorepo

```
kitron/
├── packages/
│   ├── types/       # types partagés (IPC, config, window, native…)
│   ├── ipc/         # IPC typé (NDJSON, handlers, peer)
│   ├── core/        # API publique (app, BrowserWindow, native, shell…)
│   ├── runtime/     # orchestration Node (dev/build, process managers)
│   ├── native-api/  # preload + client renderer (window.kitron)
│   └── cli/         # CLI kitron create/dev/build/package/run/doctor
├── native/rust/kitron-host/  # Rust Bridge (wry + tokio)
├── examples/react|vue|vanilla/
├── tests/unit|integration|e2e/
├── benchmarks/
└── docs/
```

## Démarrage

```bash
pnpm install
pnpm --filter @kitron/cli build

# Nouvelle app
node packages/cli/dist/bin.js create my-app --template react
cd my-app
pnpm install
pnpm dev
```

## Starter public (`npx`)

```bash
npx create-kitron my-app --template react
# alias npm : npm init kitron my-app --template react
cd my-app
npm install
npm run dev
```

Le starter émet des ranges npm (`^0.1.0`) pour `@kitron/*`.
Publication (ordre topologique automatique) :

```bash
npm login
pnpm -r publish --access public   # types → ipc → core → runtime → cli → create-kitron
```

`pnpm` réécrit `workspace:*` en numéros de version à la publication ;
`create-kitron` épingle ses templates sur sa propre release line.

MVP : `invoke("ping") → "pong"`, `native.window.setTitle`, `native.notification.show`.

Voir `docs/` pour architecture, IPC, API, sécurité, builds.

## Couverture fonctionnelle (vs Electron)

| Zone | État | Notes |
|---|---|---|
| App, fenêtres, multi-fenêtres | ✅ | `app.*`, `BrowserWindow` + `webContents` (`executeJavaScript`, zoom, find, download, `print`, `capturePage`, `printToPDF` macOS) |
| IPC | ✅ | `ipc.handle/invoke/emit/subscribe`, hub 3-branches |
| Notifications, dialogues | ✅ | natifs réels (osascript / WinForms / zenity) |
| Menu applicatif, menu contextuel, tray | ✅ macOS / ⚠️ ailleurs | AppKit via objc2 (rôles, accelerators, clicks IPC) ; stubs sur Win/Linux |
| Presse-papiers | ✅ | texte / HTML / RTF / image PNG (pas de bookmarks) |
| Écrans, curseur, capture desktop | ✅ | displays réels, curseur tracké, `desktopCapturer.getSources` (macOS) |
| shell, theme, power, instance, protocoles | ✅ | Node-first ; suspend/resume OS non branchés |
| crashReporter, updater, safeStorage | ✅ | JSON + hook panics Rust, feed semver, AES-GCM + trousseau |
| CLI / packaging | ✅ | create/dev/build/package/run/doctor |
| DevTools programmatique | ⚠️ | wry 0.6 ne l'expose pas ; `kitron dev --devtools` active Inspect Element (macOS) |
| `printToPDF` Windows/Linux | ❌ | wry 0.6 ne donne pas accès au WebView natif ; macOS via `WKWebView.createPDF` |
| Dock / JumpList / TouchBar / `inAppPurchase` | ❌ | niches OS, pas priorisées |

Pourquoi ces gaps : chaque item ❌/⚠️ exige soit du glue natif non exposé par
wry 0.6 (fenêtre native sous Win/Linux), soit une intégration event-loop
lourde (menus/tray hors macOS), soit une niche (TouchBar). La règle reste :
Node.js quand Node.js suffit, Rust uniquement pour l'accès OS — sans
dupliquer les moteurs (jamais de Chromium embarqué).
