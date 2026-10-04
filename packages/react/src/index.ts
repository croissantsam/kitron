/**
 * @kitron/react — mainline React renderer for Kitron UI.
 *
 * Real `react` + `react-reconciler` (same packages React Native itself is
 * built on), with a host config that targets Kitron's own view store
 * (`ui.*` ops) instead of a platform fork. No react-native-macos, no
 * webview, no Chromium.
 *
 * ```tsx
 * import React, { useState } from "react";
 * import { Button, Text, View, render } from "@kitron/react";
 * import { window } from "kitron";
 *
 * const main = await window.create({ width: 1200, height: 800 });
 *
 * function App() {
 *   const [count, setCount] = useState(0);
 *   return (
 *     <View style={{ flex: 1, align: "center", justify: "center" }}>
 *       <Text style={{ fontSize: 28 }}>Hello Kitron</Text>
 *       <Text>Clicked {count} times</Text>
 *       <Button title="Click me" onPress={() => setCount((c) => c + 1)} />
 *     </View>
 *   );
 * }
 *
 * await render(<App />, main.id);
 * ```
 */

import { invoke, onNativeEvent } from "@kitron/core";
import type { UiStyle } from "@kitron/ui";
import Reconciler from "react-reconciler";

// ---- host component tags (intrinsic elements, like RN core names) ----

export const View = "View";
export const Text = "Text";
export const Button = "Button";
export const TextInput = "TextInput";
export const Input = "TextInput";
export const Image = "Image";

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
      Image: {
        source?: { uri?: string } | number;
        style?: UiStyle;
      };
    }
  }
}

type HostType = "View" | "Text" | "Button" | "TextInput" | "Image";

interface Instance {
  uiId: string | null;
  kind: "node" | "text";
  hostType?: HostType;
  /** Raw JSX props (for diffing). */
  props: Record<string, unknown>;
  /** Virtual text (text markers only). */
  text?: string;
  parent: Instance | Container | null;
  children: Instance[];
}

interface Container {
  windowId: string;
  rootUiId: string | null;
}

// ---- ordered async op queue (single chain preserves protocol order) ----

let tail: Promise<void> = Promise.resolve();

function enqueue(op: () => Promise<unknown>): Promise<void> {
  const run = tail.then(op, op).then(() => undefined);
  tail = run.catch(() => undefined);
  return run;
}

/** Resolve when all queued ui ops have flushed. */
export function flushUi(): Promise<void> {
  return tail;
}

// ---- event routing (ui.press / ui.change → props handlers) ----

const handlers = new Map<string, { press?: () => void; change?: (value: string) => void; scroll?: (e: { x: number; y: number }) => void }>();

let listenerInstalled = false;
function ensureListener(): void {
  if (listenerInstalled) return;
  listenerInstalled = true;
  onNativeEvent("ui.press", (p) => {
    const { nodeId } = p as { nodeId?: string };
    if (nodeId) handlers.get(nodeId)?.press?.();
  });
  onNativeEvent("ui.change", (p) => {
    const { nodeId, value } = p as { nodeId?: string; value?: string };
    if (nodeId) handlers.get(nodeId)?.change?.(value ?? "");
  });
  onNativeEvent("ui.scroll", (p) => {
    const { nodeId, x, y } = p as { nodeId?: string; x?: number; y?: number };
    if (nodeId) handlers.get(nodeId)?.scroll?.({ x: x ?? 0, y: y ?? 0 });
  });
}

// ---- props mapping (wire split mirrors @kitron/ui) ----

const PAINT_KEYS = new Set(["background", "color", "fontSize", "borderRadius"]);

function splitProps(hostType: HostType, props: Record<string, unknown>): { uiProps: Record<string, unknown>; style: UiStyle } {
  const style: Record<string, unknown> = { ...((props["style"] as object) ?? {}) };
  const uiProps: Record<string, unknown> = {};
  for (const k of PAINT_KEYS) {
    if (style[k] !== undefined) {
      uiProps[k] = style[k];
      delete style[k];
    }
  }
  if (hostType === "Button") {
    uiProps["text"] = (props["title"] as string) ?? "";
  }
  if (hostType === "TextInput") {
    uiProps["text"] = (props["value"] as string) ?? (props["defaultValue"] as string) ?? "";
    uiProps["placeholder"] = (props["placeholder"] as string) ?? "";
  }
  if (hostType === "Image") {
    const source = props["source"] as { uri?: string } | undefined;
    uiProps["uri"] = source?.uri ?? "";
  }
  // Views and Texts with onPress participate in hit-testing (Pressable).
  if ((hostType === "View" || hostType === "Text") && props["onPress"] !== undefined) {
    uiProps["pressable"] = true;
  }
  if (props["scrollable"] === true) {
    uiProps["scrollable"] = true;
  }
  return { uiProps, style: style as UiStyle };
}

/** Concatenated text of an instance subtree (markers + nested Text). */
function textContent(inst: Instance): string {
  if (inst.kind === "text") return inst.text ?? "";
  if (inst.hostType === "Text") {
    return inst.children.map(textContent).join("");
  }
  return "";
}

function refreshText(inst: Instance): void {
  if (inst.hostType !== "Text" && inst.hostType !== "Button") return;
  if (inst.uiId === null) return;
  const text = inst.hostType === "Button"
    ? ((inst.props["title"] as string) ?? "")
    : textContent(inst);
  const uiId = inst.uiId;
  void enqueue(() => invoke("ui", "setProps", { id: uiId, props: { text } }));
}

function storeHandlers(inst: Instance): void {
  if (inst.uiId === null) return;
  const press = inst.props["onPress"] as (() => void) | undefined;
  const change = (inst.props["onChangeText"] as ((v: string) => void) | undefined)
    ?? (inst.props["onChange"] as ((v: string) => void) | undefined);
  const scroll = inst.props["onScroll"] as ((e: { x: number; y: number }) => void) | undefined;
  if (press ?? change ?? scroll) {
    handlers.set(inst.uiId, { press, change, scroll });
  } else {
    handlers.delete(inst.uiId);
  }
}

function dropSubtreeHandlers(inst: Instance): void {
  if (inst.uiId !== null) handlers.delete(inst.uiId);
  for (const c of inst.children) dropSubtreeHandlers(c);
}

let nextId = 1;

// ---- React 19.x scheduling stubs (mirror react-native's host config) ----

// Lane bitmask values from ReactFiberLane (stable across 18/19).
const NoLane = 0;
const DefaultLane = 16;
let currentUpdatePriority: number = DefaultLane;

function setCurrentUpdatePriority(priority: number): void {
  currentUpdatePriority = priority;
}

function getCurrentUpdatePriority(): number {
  return currentUpdatePriority;
}

function resolveUpdatePriority(): number {
  return currentUpdatePriority !== NoLane ? currentUpdatePriority : DefaultLane;
}

// ---- reconciler ----

const hostConfig = {
  supportsMutation: true,
  isPrimaryRenderer: true,
  supportsPersistence: false,
  supportsHydration: false,
  noTimeout: -1 as const,

  setCurrentUpdatePriority,
  getCurrentUpdatePriority,
  resolveUpdatePriority,
  shouldAttemptEagerTransition: () => false,
  trackSchedulerEvent: () => {},
  resolveEventType: () => null,
  resolveEventTimeStamp: () => 0,
  maySuspendCommit: () => false,
  maySuspendCommitOnUpdate: () => false,
  maySuspendCommitInSyncRender: () => false,
  preloadInstance: () => true,
  startSuspendingCommit: () => ({}),
  suspendInstance: () => {},
  waitForCommitToBeReady: (_state: unknown, flags: number) => flags,
  getSuspendedCommitReason: () => null,
  resetFormInstance: () => {},
  requestPostPaintCallback: () => {},
  NotPendingTransition: null,
  HostTransitionContext: null,
  bindToConsole: (methodName: string, args: unknown[]) => {
    const fn = (console as unknown as Record<string, unknown>)[methodName];
    if (typeof fn === "function") {
      return (fn as (...a: unknown[]) => void).bind(console, ...args);
    }
    return () => {};
  },

  scheduleTimeout: (typeof setTimeout === "function" ? setTimeout : undefined) as unknown as (
    fn: (...args: unknown[]) => void,
    delay?: number,
  ) => number,
  cancelTimeout: (typeof clearTimeout === "function" ? clearTimeout : undefined) as unknown as (id: number) => void,
  scheduleMicrotask:
    typeof queueMicrotask === "function"
      ? queueMicrotask
      : ((fn: () => void) => Promise.resolve().then(fn)) as (fn: () => void) => void,

  getRootHostContext: () => ({}),
  getChildHostContext: () => ({}),
  getPublicInstance: (inst: Instance) => inst,
  getInstanceFromNode: (inst: Instance) => inst,
  beforeActiveInstanceBlur: () => {},
  afterActiveInstanceBlur: () => {},
  prepareForCommit: () => null,
  resetAfterCommit: () => {},

  createInstance(
    type: HostType,
    props: Record<string, unknown>,
  ): Instance {
    const uiId = `r-${nextId++}`;
    const inst: Instance = { uiId, kind: "node", hostType: type, props, parent: null, children: [] };
    const kind =
      type === "View" ? "view"
      : type === "Text" ? "text"
      : type === "Button" ? "button"
      : type === "Image" ? "image"
      : "input";
    const { uiProps, style } = splitProps(type, props);
    if (type === "Text") uiProps["text"] = "";
    void enqueue(() => invoke("ui", "create", { id: uiId, kind, props: uiProps, style }));
    storeHandlers(inst);
    return inst;
  },

  createTextInstance(text: string): Instance {
    return { uiId: null, kind: "text", props: {}, text: String(text), parent: null, children: [] };
  },

  appendInitialChild(parent: Instance, child: Instance): void {
    attach(parent, child);
  },
  appendChild(parent: Instance, child: Instance): void {
    attach(parent, child);
  },
  appendChildToContainer(container: Container, child: Instance): void {
    child.parent = container;
    containerChildren(container).push(child);
  },

  insertBefore(parent: Instance, child: Instance, before: Instance): void {
    detach(parent, child);
    insertAt(parent, child, indexOf(parent, before));
  },
  insertInContainerBefore(container: Container, child: Instance, before: Instance): void {
    const kids = containerChildren(container);
    const at = kids.indexOf(before);
    child.parent = container;
    if (at < 0) kids.push(child);
    else kids.splice(at, 0, child);
  },

  removeChild(parent: Instance, child: Instance): void {
    detach(parent, child);
  },
  removeChildFromContainer(container: Container, child: Instance): void {
    const kids = containerChildren(container);
    const at = kids.indexOf(child);
    if (at >= 0) kids.splice(at, 1);
    child.parent = null;
    if (child.uiId !== null) {
      const uiId = child.uiId;
      dropSubtreeHandlers(child);
      void enqueue(() => invoke("ui", "remove", { id: uiId }));
    }
  },

  finalizeInitialChildren: () => false,
  shouldSetTextContent: () => false,

  prepareUpdate(
    _instance: Instance,
    _type: HostType,
    oldProps: Record<string, unknown>,
    newProps: Record<string, unknown>,
  ): Record<string, unknown> | null {
    const keys = new Set([...Object.keys(oldProps), ...Object.keys(newProps)]);
    for (const k of keys) {
      if (k === "children") continue;
      if (!Object.is(oldProps[k], newProps[k])) return { ...newProps };
    }
    return null;
  },

  commitUpdate(instance: Instance, payload: Record<string, unknown>): void {
    instance.props = payload;
    if (instance.uiId === null || instance.hostType === undefined) return;
    const { uiProps, style } = splitProps(instance.hostType, payload);
    if (instance.hostType === "Text") {
      uiProps["text"] = textContent(instance);
    }
    if (instance.hostType === "TextInput" && uiProps["text"] === undefined) {
      uiProps["text"] = (payload["value"] as string) ?? (payload["defaultValue"] as string) ?? "";
      uiProps["placeholder"] = (payload["placeholder"] as string) ?? "";
    }
    const uiId = instance.uiId;
    void enqueue(() => invoke("ui", "setProps", { id: uiId, props: uiProps }));
    void enqueue(() => invoke("ui", "setStyle", { id: uiId, style }));
    storeHandlers(instance);
  },

  commitTextUpdate(textInstance: Instance, _oldText: string, newText: string): void {
    textInstance.text = String(newText);
    const parent = textInstance.parent;
    if (parent !== null && !isContainer(parent) && parent.hostType === "Text") {
      refreshText(parent);
    }
  },

  hideInstance: () => {},
  unhideInstance: () => {},
  hideTextInstance: () => {},
  unhideTextInstance: () => {},
  clearContainer: () => {},
  detachDeletedInstance: () => {},

  preparePortalMount: () => {},
};

function isContainer(p: Instance | Container): p is Container {
  return (p as Container).windowId !== undefined;
}

const containerKids = new WeakMap<Container, Instance[]>();
function containerChildren(c: Container): Instance[] {
  let kids = containerKids.get(c);
  if (!kids) {
    kids = [];
    containerKids.set(c, kids);
  }
  return kids;
}

function indexOf(parent: Instance, child: Instance): number {
  return parent.children.indexOf(child);
}

function attach(parent: Instance, child: Instance): void {
  child.parent = parent;
  parent.children.push(child);
  if (child.kind === "text") {
    if (parent.hostType === "Text") refreshText(parent);
    return;
  }
  if (child.uiId === null) return;
  const uiId = child.uiId;
  const at = parent.children.filter((c) => c.kind === "node").length - 1;
  void enqueue(() => invoke("ui", "append", { parent: parent.uiId, child: uiId, index: at }));
  if (parent.hostType === "Text") refreshText(parent);
}

function insertAt(parent: Instance, child: Instance, at: number): void {
  child.parent = parent;
  const kids = parent.children;
  const nodeKids = kids.filter((c) => c.kind === "node");
  const ref = nodeKids[at];
  const nodeIndex = ref !== undefined ? kids.indexOf(ref) : kids.length;
  kids.splice(nodeIndex, 0, child);
  if (child.kind === "text") {
    if (parent.hostType === "Text") refreshText(parent);
    return;
  }
  if (child.uiId === null) return;
  const uiId = child.uiId;
  void enqueue(() => invoke("ui", "append", { parent: parent.uiId, child: uiId, index: nodeIndex }));
}

function detach(parent: Instance, child: Instance): void {
  const at = parent.children.indexOf(child);
  if (at >= 0) parent.children.splice(at, 1);
  child.parent = null;
  if (child.kind === "text") {
    if (parent.hostType === "Text") refreshText(parent);
    return;
  }
  if (child.uiId === null) return;
  const uiId = child.uiId;
  dropSubtreeHandlers(child);
  void enqueue(() => invoke("ui", "remove", { id: uiId }));
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const reconciler = Reconciler(hostConfig as any);

interface Root {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  container: any;
  info: Container;
  windowId: string;
}

const roots = new Map<string, Root>();

/**
 * Render React into a Kitron window. Resolves after all ui ops flush.
 * Re-renders patch the live tree (state updates Just Work).
 */
export async function render(element: React.ReactNode, windowId: string): Promise<void> {
  ensureListener();
  let root = roots.get(windowId);
  if (!root) {
    const info: Container = { windowId, rootUiId: null };
    const container = reconciler.createContainer(
      info as unknown as Container,
      0,
      null,
      false,
      null,
      "",
      (error: Error) => {
        throw error;
      },
      (error: Error) => {
        throw error;
      },
      () => {},
      () => {},
      null,
    );
    root = { container, info, windowId };
    roots.set(windowId, root);
  }
  reconciler.updateContainerSync(element, root.container, null, null);
  // React 19 schedules even sync lanes through the scheduler package (a
  // macrotask in Node), so yield once before flushing our op queue.
  await new Promise<void>((resolve) => setImmediate(resolve));
  // Root bookkeeping: find the single top instance and bind it.
  const kids = containerKids.get(root.info) ?? [];
  const top = kids.find((k) => k.kind === "node" && k.uiId !== null);
  await flushUi();
  if (top?.uiId) {
    await invoke("ui", "setRoot", { window: windowId, root: top.uiId });
  }
}

/** Unmount a window's React tree + clear its native root. */
export async function unmount(windowId: string): Promise<void> {
  const root = roots.get(windowId);
  if (root) {
    reconciler.updateContainerSync(null, root.container, null, null);
    roots.delete(windowId);
  }
  await new Promise<void>((resolve) => setImmediate(resolve));
  await flushUi();
  await invoke("ui", "clearRoot", { window: windowId }).catch(() => undefined);
}

/** @internal — reset renderer state (tests). */
export function __resetReactForTests(): void {
  roots.clear();
  handlers.clear();
  nextId = 1;
  tail = Promise.resolve();
}

// ---- RN-core-compatible surface ----

/** `StyleSheet.create` — identity (plain objects, validated by TS types). */
export const StyleSheet = {
  create<T extends Record<string, UiStyle>>(styles: T): T {
    return styles;
  },
  flatten(style: UiStyle | UiStyle[] | undefined): UiStyle {
    if (Array.isArray(style)) return Object.assign({}, ...style);
    return style ?? {};
  },
  hairlineWidth: 1,
};

export const Platform = {
  OS: "kitron" as const,
  get Version(): string {
    return "0.1.0";
  },
  select<T>(options: { kitron?: T; default?: T } & Record<string, T | undefined>): T | undefined {
    return options["kitron"] ?? options["default"];
  },
};

const registry = new Map<string, React.ComponentType>();

export const AppRegistry = {
  registerComponent(appKey: string, component: () => React.ComponentType): void {
    registry.set(appKey, component());
  },
  async runApplication(appKey: string, { rootTag }: { rootTag: string }): Promise<void> {
    const Component = registry.get(appKey);
    if (!Component) throw new Error(`AppRegistry: "${appKey}" not registered`);
    const React = await import("react");
    await render(React.createElement(Component), rootTag);
  },
};

export type { UiStyle };
