# react-native compatibility

Kitron implements the `react-native` API on its own renderer. Pure-JS
libraries work; libraries shipping iOS/Android native code need a Kitron
bridge adapter.

## Using the `react-native` specifier

In your app's `package.json`, alias the specifier to Kitron's implementation:

```json
{
  "dependencies": {
    "react-native": "npm:@kitron/react-native@0.1.0"
  }
}
```

Then `import { View, FlatList } from "react-native"` — including *inside*
third-party libraries — resolves to Kitron. (`examples/hello-kitron` proves
this with a `file:` alias.)

## What works

| Area | Status |
|---|---|
| `View`, `Text` (+ nested, `onPress`), `Button`, `TextInput`, `Pressable` | ✅ native |
| `ScrollView` (+ `onScroll`), `FlatList`, `Image` (`file://`, `http(s)://`) | ✅ native |
| `StyleSheet.create/flatten`, `Platform`, `Dimensions`, `PixelRatio` | ✅ |
| `Alert`, `Linking`, `AppState`, `Clipboard`, `Appearance`, `BackHandler`, `Vibration`, `Keyboard`, `AccessibilityInfo`, `Easing`, `InteractionManager` | ✅ (stubs where desktop has no equivalent) |
| `Animated.Value` + `timing` (JS-driven) | ✅ |
| Hooks, context, suspense-free trees, `AppRegistry` | ✅ via mainline `react-reconciler` |

Style props beyond the renderer subset (`opacity`, `position: absolute`,
`borderWidth`, …) are accepted and ignored — never a crash. Unsupported
layout falls back gracefully; file an issue for what you need next.

## What needs an adapter

Anything with its own `.podspec` / Gradle module / JSI C++: reanimated
worklets, gesture-handler, svg, async-storage, MMKV, vector-icons fonts…
Accessing them throws `NATIVE_ADAPTER_MISSING` with this guidance:

1. Check for a pure-JS alternative first (most formatters, parsers, date,
   validation, navigation state libs just work).
2. Otherwise port the native side to a Kitron bridge domain
   (`ui.*`-style typed `invoke` + Rust module, §7 of the spec) and expose
   the same JS API. The adapter lives in userland — Kitron core stays clean.

`NativeModules.*`, `TurboModuleRegistry.get()`, and
`requireNativeComponent()` all throw the guided error (never `undefined`).

## Porting checklist (pure-JS lib)

1. `npm install` it — imports resolve through the alias.
2. Render it — missing components throw `UI_NOT_FOUND` with the node kind.
3. Replace `Modal` (no multi-root overlays yet), `RefreshControl`,
   `ActivityIndicator` (no animation loop yet) with Kitron equivalents.
4. `Dimensions.get` is sync-cached — call `Dimensions.syncFromWindow(id)`
   after `window.create` for exact pixels.
