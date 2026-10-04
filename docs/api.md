# Public API

```ts
import { app, BrowserWindow, native } from "@kitron/core";

await app.ready();
const win = new BrowserWindow({ width: 1200, height: 800, title: "My App" });
await win.loadURL("http://127.0.0.1:5173");

await native.notification.show({ title: "My App", body: "started" });
await native.window.setAlwaysOnTop(true);
```

Namespaces: `native.window|notification|dialog|menu|tray|clipboard|
screen|shortcut|power`, plus `shell.exec` (Node sugar), `logger`,
`updater`, `definePlugin`, `app.getPath("userData"|"documents"|…)`.

## Electron-parity tranches

Tranche A — desktop core (Node-first):
`shell.openExternal/openPath/showItemInFolder/trashItem/beep`,
`nativeTheme` (shouldUseDarkColors/themeSource/updated),
`powerMonitor.isOnBatteryPower()`, `requestSingleInstanceLock()` +
`second-instance`, custom protocols (`setAsDefaultProtocolClient`,
`open-url`), real `screen.getDisplays()` from the host, full window
events (`focused/blurred/moved/resized/minimized/maximized/restored/closed`).

Tranche B — `win.webContents`: `executeJavaScript` (round-trip évalué
dans la WebView, timeout typé `EVAL_FAILED` sans bridge),
`getURL/getTitle/get-setZoomFactor/reload`, `findInPage`, `downloadURL`,
`print` (dialogue page).

Tranche C — vraies fenêtres multiples : chaque `window.create` ouvre une
WebView native, événements routés par fenêtre.

## Suite parité (v2)

- Dialogues natifs réels : `dialog.open` (fichiers/multi/dossier/filtres),
  `save`, `message` (boutons/icônes) via osascript / WinForms / zenity.
  Scripts AppleScript validés par `osacompile` en tests.
- `capturePage` : screenshot du rect fenêtre — l'app packagée doit
  demander Screen Recording (refus OS = `NATIVE_API_FAILED` typé).
- Clipboard image PNG base64 dans les deux sens.
- `shell.*`, `nativeTheme`, `powerMonitor` (+ idle réel), single-instance,
  protocoles custom, `crashReporter` (JSON, hook panics Rust inclus),
  `updater` (feed + semver + download), `safeStorage` (AES-GCM + trousseau).
- `setFullscreen/isFullscreen`, min/max size, notification `sound` (macOS).
- `printToPDF` : vrai PDF vectoriel via `WKWebView.createPDF` (objc2,
  réponse async sans bloquer la boucle) — macOS ; autres OS en erreur
  typée (wry 0.6 n'expose pas le WebView natif pour Windows/Linux).

Gaps restants : barre de menus/tray natifs, DevTools
programmatique, suspend/resume OS.
