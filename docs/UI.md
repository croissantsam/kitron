# Kitron UI — our own renderer

No react-native fork. No webview. No Chromium. Two layers:

```text
React (@kitron/react)                    View store (kitron-runtime)
  mainline react + react-reconciler ──►  ui.create / append / setProps…
  JSX, hooks, state                ──►  UiManager (tree store, per-window)
  <View>/<Text>/<Button>/<Input>   ──►  taffy layout (viewport-rooted)
  onPress / onChangeText           ◄──  hit-test + ui.press / ui.change
  pixels                           ◄──  tiny-skia + cosmic-text
```

`@kitron/ui` is the imperative protocol layer underneath
(`View()/Text()/render()` without React); `@kitron/react` is the
recommended programming model. Both speak the same `ui.*` ops.

## React layer

Host tags are RN-core names (`View`, `Text`, `Button`, `TextInput`) as
intrinsic elements — no fork, just `react` from npm:

```tsx
import { useState } from "react";
import { Button, Text, View, render } from "@kitron/react";

function App() {
  const [count, setCount] = useState(0);
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center" }}>
      <Text>Clicked {count} times</Text>
      <Button title="Go" onPress={() => setCount((c) => c + 1)} />
    </View>
  );
}
await render(<App />, main.id);
```

- `render(element, windowId)` flushes through the scheduler, then binds
  the root (`ui.setRoot`). State updates patch live nodes.
- Raw strings become text; nested `<Text>` flattens; `Button` takes
  `title`; inputs take `value`/`onChangeText`.
- `AppRegistry.registerComponent` + `runApplication(key, { rootTag })`,
  `StyleSheet.create` (identity), `Platform.OS === "kitron"` included.
- The reconciler carries React 19.x scheduling stubs mirroring
  react-native's own host config (`resolveUpdatePriority`, …).

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

HTML/CSS engine, webview fallback, react-native fork compatibility. The
protocol is small on purpose: four node kinds, one layout engine, typed
errors (`UI_NOT_FOUND`) for everything else. React compatibility means the
`react` programming model — not the `react-native` native modules.
