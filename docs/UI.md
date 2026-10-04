# Kitron UI — our own renderer

No React Native. No webview. No Chromium. Kitron renders its own retained
view tree, natively, on every window.

```text
TypeScript (@kitron/ui)              Rust (kitron-runtime)
  View/Text/Button/Input  ──►  ui.create / append / setProps / remove
  render(window, element) ──►  UiManager (tree store, per-window roots)
  flexbox style           ──►  taffy layout (viewport-rooted)
  pixels                  ◄──  tiny-skia (rounded rects) + cosmic-text
  onPress / onChange      ◄──  hit-test + ui.press / ui.change events
```

## Components

| Kind | Props | Notes |
|---|---|---|
| `View` | `style` | Only container; `flex/direction/align/justify/width/height/padding/gap/background` |
| `Text` | `style.fontSize/color`, text | Pre-measured (viewport-bounded, word wrap) |
| `Button` | `title`, `onPress` | Press → `ui.press {windowId, nodeId}` → TS handler |
| `TextInput` | `value?`, `placeholder?`, `onChange?` | Click focuses; typing → `ui.change`; IME is a later milestone |

Styles are a flexbox subset. Anything else is rejected at the type level
(TS) or ignored at the protocol level (Rust `UiStyle`).

## Reconciler rules (`@kitron/ui`)

- Stable ids from the child-index path (`ui-1.0.2`). Re-render patches
  props/styles in place; kind changes rebuild the subtree.
- No keyed moves yet: inserting a child mid-list shifts paths. Documented
  limitation, keys planned.
- `render()` is async and idempotent; `unmount()` tears down + clears root.

## Painting

- `UiManager::paint_window` lays out and fills a `tiny-skia::Pixmap`.
- Windowed mode presents via `softbuffer`; headless/tests render offscreen.
- Glyphs rasterize through `cosmic-text` `SwashCache` (mask/color/subpixel).
- Dirty tracking per window; `request_redraw` on every mutation.

## Input

- Mouse: cursor tracking + `press_at` → button press or input focus.
- Keyboard: focused input receives text + backspace → `ui.change`.
- Multi-window: each OS window owns its surface, cursor, and UI root;
  `SyncWindows` reconciles headless records with live windows.

## Deliberate non-goals

HTML/CSS engine, webview fallback, Fabric compatibility. The protocol is
small on purpose: four node kinds, one layout engine, typed errors
(`UI_NOT_FOUND`) for everything else.
