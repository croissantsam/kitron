/**
 * `react-native` API-compatible implementation for Kitron.
 *
 * Apps (and pure-JS third-party libs) write `import { View, FlatList } from
 * "react-native"` — resolved via the npm alias
 * `"react-native": "npm:@kitron/react-native@0.1.0"` — and render through
 * Kitron's own reconciler host. No fork, no native build.
 *
 * Compatibility boundary (see docs/RN-COMPAT.md):
 * - WORKS: core components, StyleSheet, Platform, Dimensions, Alert,
 *   Linking, AppState, Clipboard, Animated (JS-driven), FlatList.
 * - NEEDS A KITRON ADAPTER: any library shipping its own native code
 *   (reanimated worklets, gesture-handler, svg, async-storage…).
 *   `NativeModules`/`TurboModuleRegistry` throw a guided error.
 */

import React from "react";
import { invoke, KitronError, onNativeEvent } from "@kitron/core";
import {
  AppRegistry,
  Platform as KitronPlatform,
  StyleSheet as KitronStyleSheet,
  render as kitronRender,
  unmount as kitronUnmount,
} from "@kitron/react";
import type { UiStyle } from "@kitron/ui";

// ---- host tags (same renderer, RN-core names) ----

export { Button, Text, TextInput, View, flushUi } from "@kitron/react";

declare module "react" {
  // eslint-disable-next-line @typescript-eslint/no-namespace
  namespace JSX {
    interface IntrinsicElements {
      View: { style?: UiStyle; children?: React.ReactNode; onPress?: () => void; scrollable?: boolean; onScroll?: (e: { x: number; y: number }) => void };
      Text: { style?: UiStyle; children?: React.ReactNode; onPress?: () => void };
      Button: { title: string; onPress?: () => void; style?: UiStyle };
      TextInput: {
        value?: string;
        defaultValue?: string;
        placeholder?: string;
        onChangeText?: (value: string) => void;
        onChange?: (value: string) => void;
        style?: UiStyle;
      };
      Image: { source?: { uri?: string } | number; style?: UiStyle };
    }
  }
}

// ---- StyleSheet (RN superset mapped onto UiStyle) ----

export interface ViewStyle extends UiStyle {
  display?: "flex" | "none";
  position?: "relative" | "absolute";
  top?: number;
  left?: number;
  right?: number;
  bottom?: number;
  margin?: number;
  marginHorizontal?: number;
  marginVertical?: number;
  marginTop?: number;
  marginBottom?: number;
  marginLeft?: number;
  marginRight?: number;
  paddingHorizontal?: number;
  paddingVertical?: number;
  paddingTop?: number;
  paddingBottom?: number;
  paddingLeft?: number;
  paddingRight?: number;
  borderWidth?: number;
  borderColor?: string;
  borderRadius?: number;
  opacity?: number;
  overflow?: "visible" | "hidden" | "scroll";
  flexGrow?: number;
  flexShrink?: number;
  flexBasis?: number | string;
  alignSelf?: "center" | "start" | "end" | "stretch" | "baseline";
  aspectRatio?: number;
  zIndex?: number;
  [key: string]: unknown;
}

export interface TextStyle extends Omit<ViewStyle, "background"> {
  fontSize?: number;
  color?: string;
  fontWeight?: string;
  textAlign?: "left" | "center" | "right";
  lineHeight?: number;
}

export type StyleProp<T> = T | T[] | undefined;

function flattenStyle<T>(style: StyleProp<T>): T {
  if (Array.isArray(style)) return Object.assign({}, ...style) as T;
  return (style ?? {}) as T;
}

function toUiStyle(style: StyleProp<ViewStyle>): UiStyle {
  const s = flattenStyle(style);
  const ui: Record<string, unknown> = {};
  // Layout subset the renderer implements; the rest is accepted and ignored
  // (documented in RN-COMPAT.md) rather than rejected.
  for (const k of ["flex", "direction", "align", "justify", "width", "height", "padding", "gap", "background", "color", "fontSize", "borderRadius", "shrink"] as const) {
    if (s[k] !== undefined) ui[k] = s[k];
  }
  if (s.flexGrow !== undefined) ui["flex"] = s.flexGrow;
  if (s.flexShrink !== undefined) ui["shrink"] = s.flexShrink;
  if (s.backgroundColor !== undefined) ui["background"] = s.backgroundColor;
  if (typeof s.paddingHorizontal === "number") ui["padding"] = s.paddingHorizontal;
  return ui as UiStyle;
}

export const StyleSheet = {
  ...KitronStyleSheet,
  create<T extends Record<string, ViewStyle | TextStyle>>(styles: T): T {
    return styles;
  },
  flatten: flattenStyle,
  absoluteFill: { position: "absolute", top: 0, left: 0, right: 0, bottom: 0 } as ViewStyle,
};

// ---- Pressable ----

export interface PressableProps {
  onPress?: () => void;
  onLongPress?: () => void;
  disabled?: boolean;
  style?: StyleProp<ViewStyle>;
  children?: React.ReactNode;
}

export function Pressable(props: PressableProps): React.ReactElement {
  const { onPress, disabled, style, children } = props;
  return React.createElement("View", {
    style: toUiStyle(style) as never,
    ...(disabled === true || onPress === undefined ? {} : { onPress }),
    children,
  } as never);
}

// ---- ScrollView ----

export interface ScrollViewProps {
  style?: StyleProp<ViewStyle>;
  contentContainerStyle?: StyleProp<ViewStyle>;
  onScroll?: (e: { contentOffset: { x: number; y: number } }) => void;
  showsVerticalScrollIndicator?: boolean;
  children?: React.ReactNode;
}

export function ScrollView(props: ScrollViewProps): React.ReactElement {
  // `scrollable` + `onScroll` flow to the host View, which registers the
  // ui.scroll routing by node id (see @kitron/react storeHandlers).
  void props.contentContainerStyle;
  void props.showsVerticalScrollIndicator;
  return React.createElement("View", {
    style: toUiStyle(props.style) as never,
    scrollable: true,
    ...(props.onScroll === undefined
      ? {}
      : { onScroll: (pt: { x: number; y: number }) => props.onScroll?.({ contentOffset: pt }) }),
    children: props.children,
  } as never);
}


// ---- FlatList (JS windowing over ScrollView) ----

export interface FlatListProps<Item> {
  data: Item[];
  renderItem: (info: { item: Item; index: number }) => React.ReactElement | null;
  keyExtractor?: (item: Item, index: number) => string;
  style?: StyleProp<ViewStyle>;
  contentContainerStyle?: StyleProp<ViewStyle>;
  ListHeaderComponent?: React.ReactElement | null;
  ListEmptyComponent?: React.ReactElement | null;
  onEndReached?: () => void;
  onEndReachedThreshold?: number;
}

export function FlatList<Item>(props: FlatListProps<Item>): React.ReactElement {
  const { data } = props;
  const kids: React.ReactNode[] = [];
  if (props.ListHeaderComponent !== undefined && props.ListHeaderComponent !== null) {
    kids.push(props.ListHeaderComponent);
  }
  if (data.length === 0 && props.ListEmptyComponent !== undefined && props.ListEmptyComponent !== null) {
    kids.push(props.ListEmptyComponent);
  }
  data.forEach((item, index) => {
    const el = props.renderItem({ item, index });
    if (el !== null && el !== undefined) {
      const key = props.keyExtractor !== undefined ? props.keyExtractor(item, index) : String(index);
      kids.push(React.cloneElement(el as React.ReactElement, { key }));
    }
  });
  return React.createElement(ScrollView, {
    style: props.style,
    contentContainerStyle: props.contentContainerStyle,
    children: kids.length === 1 ? kids[0] : kids,
  });
}

// ---- Image ----

export type ImageSource = { uri?: string } | number;

export interface ImageProps {
  source?: ImageSource;
  style?: StyleProp<ViewStyle>;
  resizeMode?: "cover" | "contain" | "stretch" | "center";
}

export function Image(props: ImageProps): React.ReactElement {
  const source = (typeof props.source === "object" && props.source !== null ? props.source : {}) as { uri?: string };
  return React.createElement("Image" as never, {
    source,
    style: toUiStyle(props.style) as never,
  } as never);
}

// ---- Platform / Dimensions / PixelRatio ----

export const Platform = {
  ...KitronPlatform,
  isTV: false,
  isTesting: false,
};

export interface ScaledSize {
  width: number;
  height: number;
  scale: number;
  fontScale: number;
}

const cured: ScaledSize = { width: 1200, height: 800, scale: 2, fontScale: 1 };

export const Dimensions = {
  /** Sync, RN-compatible. Refresh after creating windows via `syncFromWindow`. */
  get(_dim: "window" | "screen"): ScaledSize {
    return { ...cured };
  },
  /** Pull the real size of a Kitron window into the cache. */
  async syncFromWindow(windowId: string): Promise<ScaledSize> {
    try {
      const info = await invoke<{ width: number; height: number }>("window", "get", { id: windowId });
      cured.width = info.width;
      cured.height = info.height;
    } catch {
      // keep last known
    }
    return { ...cured };
  },
};

export const PixelRatio = {
  get(): number {
    return 2;
  },
  getFontScale(): number {
    return 1;
  },
};

// ---- Alert (native dialog) ----

export interface AlertButton {
  text?: string;
  onPress?: () => void;
  style?: "default" | "cancel" | "destructive";
}

export const Alert = {
  async alert(
    title: string,
    message?: string,
    buttons?: AlertButton[],
    _options?: { cancelable?: boolean },
  ): Promise<void> {
    const labels = (buttons ?? [{ text: "OK" }]).map((b) => b.text ?? "OK");
    const { button } = await invoke<{ button: number }>("dialog", "message", {
      title,
      message: message ?? "",
      buttons: labels,
    });
    buttons?.[button]?.onPress?.();
  },
};

// ---- Linking (shell + deep links) ----

export const Linking = {
  async openURL(url: string): Promise<void> {
    await invoke("shell", "openExternal", { url });
  },
  async canOpenURL(_url: string): Promise<boolean> {
    return true;
  },
  addEventListener(
    type: "url",
    handler: (event: { url: string }) => void,
  ): { remove(): void } {
    const off = onNativeEvent("app.deepLink", (p) => {
      const { url } = p as { url?: string };
      if (typeof url === "string") handler({ url });
    });
    void type;
    return { remove: off };
  },
};

// ---- AppState ----

export type AppStateStatus = "active" | "background" | "inactive";

export const AppState = {
  currentState: "active" as AppStateStatus,
  addEventListener(
    type: "change" | "blur" | "focus",
    handler: (state: AppStateStatus) => void,
  ): { remove(): void } {
    const offs = [
      onNativeEvent("app.quit", () => {
        if (type === "change" || type === "blur") handler("background");
      }),
      onNativeEvent("app.ready", () => {
        if (type === "change" || type === "focus") handler("active");
      }),
    ];
    void type;
    return { remove: () => offs.forEach((o) => o()) };
  },
};

// ---- Clipboard ----

export const Clipboard = {
  async getString(): Promise<string> {
    return invoke<string>("clipboard", "readText");
  },
  async setString(text: string): Promise<void> {
    await invoke("clipboard", "writeText", { text });
  },
};

// ---- Appearance ----

export const Appearance = {
  getColorScheme(): "light" {
    return "light";
  },
  addChangeListener(_handler: (scheme: { colorScheme: "light" }) => void): { remove(): void } {
    return { remove: () => {} };
  },
};

// ---- BackHandler (desktop: no system back; kept for lib compat) ----

export const BackHandler = {
  addEventListener(_event: "hardwareBackPress", _handler: () => boolean): { remove(): void } {
    return { remove: () => {} };
  },
  exitApp(): void {},
};

// ---- Vibration / Keyboard / AccessibilityInfo (documented stubs) ----

export const Vibration = {
  vibrate(): void {},
  cancel(): void {},
};

export const Keyboard = {
  dismiss(): void {},
  addListener(): { remove(): void } {
    return { remove: () => {} };
  },
};

export const AccessibilityInfo = {
  async isScreenReaderEnabled(): Promise<boolean> {
    return false;
  },
};

// ---- Easing / InteractionManager ----

export const Easing = {
  linear: (t: number): number => t,
  ease: (t: number): number => (t < 0.5 ? 2 * t * t : -1 + (4 - 2 * t) * t),
  quad: (t: number): number => t * t,
  cubic: (t: number): number => t * t * t,
};

export const InteractionManager = {
  runAfterInteractions(task?: () => void): Promise<void> {
    return new Promise((resolve) => {
      setImmediate(() => {
        task?.();
        resolve();
      });
    });
  },
};

// ---- Animated (JS-driven values; see RN-COMPAT.md for prop coverage) ----

export class AnimatedValue {
  private _value: number;
  private listeners = new Map<number, (value: number) => void>();
  private nextListener = 1;

  constructor(value: number) {
    this._value = value;
  }

  setValue(value: number): void {
    this._value = value;
    for (const l of this.listeners.values()) l(value);
  }

  getValue(): number {
    return this._value;
  }

  addListener(cb: (value: number) => void): number {
    const id = this.nextListener++;
    this.listeners.set(id, cb);
    return id;
  }

  removeListener(id: number): void {
    this.listeners.delete(id);
  }

  interpolate(config: { inputRange: number[]; outputRange: number[] }): (input?: number) => number {
    const { inputRange, outputRange } = config;
    return (input?: number) => {
      const v = input ?? this._value;
      if (v <= inputRange[0]!) return outputRange[0]!;
      for (let i = 1; i < inputRange.length; i++) {
        if (v <= inputRange[i]!) {
          const t = (v - inputRange[i - 1]!) / (inputRange[i]! - inputRange[i - 1]!);
          return outputRange[i - 1]! + t * (outputRange[i]! - outputRange[i - 1]!);
        }
      }
      return outputRange[outputRange.length - 1]!;
    };
  }
}

export interface TimingConfig {
  toValue: number;
  duration?: number;
  easing?: (t: number) => number;
}

export const Animated = {
  Value: AnimatedValue,
  timing(value: AnimatedValue, config: TimingConfig): { start(cb?: (done: boolean) => void): void; stop(): void } {
    const duration = config.duration ?? 500;
    const easing = config.easing ?? Easing.ease;
    let stopped = false;
    let timer: ReturnType<typeof setInterval> | undefined;
    return {
      start(cb?: (done: boolean) => void): void {
        const from = value.getValue();
        const to = config.toValue;
        const t0 = Date.now();
        timer = setInterval(() => {
          if (stopped) return;
          const t = Math.min(1, (Date.now() - t0) / duration);
          value.setValue(from + (to - from) * easing(t));
          if (t >= 1) {
            if (timer !== undefined) clearInterval(timer);
            cb?.(true);
          }
        }, 16);
      },
      stop(): void {
        stopped = true;
        if (timer !== undefined) clearInterval(timer);
      },
    };
  },
};

// ---- Native module boundary ----

function adapterError(name: string): KitronError {
  return new KitronError({
    code: "NATIVE_ADAPTER_MISSING",
    message:
      `Native module "${name}" is not available: the library ships its own ` +
      `iOS/Android code. Port it to a Kitron bridge adapter (docs/RN-COMPAT.md) ` +
      `or use a pure-JS alternative.`,
  });
}

/** Non-null stub: every property access throws a guided error. */
export const NativeModules: Record<string, never> = new Proxy({} as Record<string, never>, {
  get(_target, name: string | symbol) {
    throw adapterError(String(name));
  },
});

export const TurboModuleRegistry = {
  get<T>(name: string): T {
    throw adapterError(name);
  },
  getEnforcing<T>(name: string): T {
    throw adapterError(name);
  },
};

export function requireNativeComponent<T>(viewName: string): T {
  throw adapterError(`<${viewName}> (requireNativeComponent)`);
}

export function findNodeHandle(): number | null {
  return null;
}

export function processColor(color: string): number {
  // 0xRRGGBBAA packed int, matching RN's processColor contract.
  const h = color.trim().replace("#", "");
  const full = h.length === 3 ? h.split("").map((c) => c + c).join("") : h;
  const rgb = parseInt(full.slice(0, 6), 16);
  const alpha = full.length >= 8 ? parseInt(full.slice(6, 8), 16) : 255;
  return rgb * 256 + alpha;
}

// ---- app lifecycle (re-exported reconciler surface) ----

export { AppRegistry, kitronRender as render, kitronUnmount as unmount };
export type { UiStyle };
