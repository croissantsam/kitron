/**
 * @kitron/ui — Kitron's own UI system.
 *
 * No React, no React Native, no webview. Components are plain data;
 * `render()` reconciles them against the previous tree and ships minimal
 * ops (`ui.create/append/setProps/remove/…`) to the Rust view store, which
 * lays out with flexbox (taffy) and paints natively (tiny-skia).
 *
 * ```ts
 * import { Button, Text, View, render } from "@kitron/ui";
 *
 * render(win.id, View({ style: { flex: 1 } },
 *   Text({ style: { fontSize: 28 } }, "Hello Kitron"),
 *   Button({ title: "Go", onPress: () => console.log("pressed") }),
 * ));
 * ```
 */

import { invoke, onNativeEvent } from "@kitron/core";

export type Dimension = number | `${number}%` | "auto";

export interface UiStyle {
  flex?: number;
  direction?: "column" | "row" | "column-reverse" | "row-reverse";
  align?: "center" | "start" | "end" | "stretch" | "baseline";
  justify?: "center" | "start" | "end" | "between" | "around" | "evenly";
  width?: Dimension;
  height?: Dimension;
  padding?: number;
  gap?: number;
  background?: string;
  color?: string;
  fontSize?: number;
  borderRadius?: number;
}

export type UiKind = "view" | "text" | "button" | "input";

export interface Element {
  kind: UiKind;
  props: Record<string, unknown>;
  style?: UiStyle;
  children: Element[];
}

export interface ViewProps {
  style?: UiStyle;
  children?: Element[];
}
export interface TextProps {
  style?: Omit<UiStyle, "background"> & { background?: string };
  children?: string;
}
export interface ButtonProps {
  title: string;
  onPress?: () => void;
  style?: UiStyle;
}
export interface TextInputProps {
  value?: string;
  placeholder?: string;
  onChange?: (value: string) => void;
  style?: UiStyle;
}

export function View(props: ViewProps = {}, ...children: Element[]): Element {
  return { kind: "view", props: {}, style: props.style, children: props.children ?? children };
}

export function Text(props: TextProps, text?: string): Element {
  return {
    kind: "text",
    props: { text: text ?? props.children ?? "" },
    style: props.style,
    children: [],
  };
}

export function Button(props: ButtonProps): Element {
  const el: Element = {
    kind: "button",
    props: { text: props.title },
    style: props.style,
    children: [],
  };
  if (props.onPress) pressHandlers.set(el as object, props.onPress);
  return el;
}

export function TextInput(props: TextInputProps = {}): Element {
  const el: Element = {
    kind: "input",
    props: { text: props.value ?? "", placeholder: props.placeholder ?? "" },
    style: props.style,
    children: [],
  };
  if (props.onChange) changeHandlers.set(el as object, props.onChange);
  return el;
}

// Handler registries (element identity → callback). Rebuilt every render.
const pressHandlers = new WeakMap<object, () => void>();
const changeHandlers = new WeakMap<object, (value: string) => void>();
const nodeHandlers = new Map<string, { press?: () => void; change?: (value: string) => void }>();

let listenerInstalled = false;
function ensureListener(): void {
  if (listenerInstalled) return;
  listenerInstalled = true;
  onNativeEvent("ui.press", (p) => {
    const { nodeId } = p as { nodeId?: string };
    if (nodeId) nodeHandlers.get(nodeId)?.press?.();
  });
  onNativeEvent("ui.change", (p) => {
    const { nodeId, value } = p as { nodeId?: string; value?: string };
    if (nodeId) nodeHandlers.get(nodeId)?.change?.(value ?? "");
  });
}

// ---- reconciler ----

interface LiveNode {
  id: string;
  kind: UiKind;
  el: object;
}

const trees = new Map<string, LiveNode>();
let nextId = 1;

/** Render (or re-render) a tree into a window. Keyed by child index. */
export async function render(windowId: string, root: Element): Promise<void> {
  ensureListener();
  const prev = trees.get(windowId);
  if (!prev) {
    const id = `ui-${nextId++}`;
    await mount(windowId, root, id);
    trees.set(windowId, { id, kind: root.kind, el: root });
    await invoke("ui", "setRoot", { window: windowId, root: id });
    return;
  }
  await patch(windowId, prev.id, prev, root);
  trees.set(windowId, { id: prev.id, kind: root.kind, el: root });
}

async function mount(windowId: string, el: Element, id: string): Promise<void> {
  void windowId;
  const { props, style } = split(el);
  await invoke("ui", "create", { id, kind: el.kind, props, style });
  nodeHandlers.set(id, {
    press: pressHandlers.get(el as object),
    change: changeHandlers.get(el as object),
  });
  let i = 0;
  for (const child of el.children) {
    const cid = `${id}.${i++}`;
    await mount(windowId, child, cid);
    await invoke("ui", "append", { parent: id, child: cid });
  }
}

async function patch(windowId: string, id: string, prev: LiveNode, next: Element): Promise<void> {
  if (prev.kind !== next.kind) {
    await invoke("ui", "remove", { id });
    nodeHandlers.delete(id);
    await mount(windowId, next, id);
    return;
  }
  const { props, style } = split(next);
  await invoke("ui", "setProps", { id, props });
  await invoke("ui", "setStyle", { id, style });
  nodeHandlers.set(id, {
    press: pressHandlers.get(next as object),
    change: changeHandlers.get(next as object),
  });
  // Reconcile children by index (documented limitation: no keyed moves yet).
  const prevKids = (prev.el as Element).children;
  const max = Math.max(prevKids.length, next.children.length);
  for (let i = 0; i < max; i++) {
    const cid = `${id}.${i}`;
    const p = prevKids[i];
    const n = next.children[i];
    if (p && n) {
      await patch(windowId, cid, { id: cid, kind: p.kind, el: p }, n);
    } else if (n) {
      await mount(windowId, n, cid);
      await invoke("ui", "append", { parent: id, child: cid, index: i });
    } else if (p) {
      await invoke("ui", "remove", { id: cid });
      nodeHandlers.delete(cid);
    }
  }
}

function split(el: Element): { props: Record<string, unknown>; style: UiStyle } {
  const style: UiStyle = { ...(el.style ?? {}) };
  const props: Record<string, unknown> = { ...(el.props as object) };
  // Paint attributes travel in props (Rust UiProps); layout stays in style.
  for (const key of ["background", "color", "fontSize", "borderRadius"] as const) {
    const v = (style as Record<string, unknown>)[key];
    if (v !== undefined) {
      props[key] = v;
      delete (style as Record<string, unknown>)[key];
    }
  }
  return { props, style };
}

/** Remove every node of a window's tree (also clears the native root). */
export async function unmount(windowId: string): Promise<void> {
  const prev = trees.get(windowId);
  if (prev) {
    await invoke("ui", "remove", { id: prev.id }).catch(() => undefined);
    trees.delete(windowId);
  }
  await invoke("ui", "clearRoot", { window: windowId }).catch(() => undefined);
}

/** @internal — reset reconciler state (tests). */
export function __resetUiForTests(): void {
  trees.clear();
  nodeHandlers.clear();
  nextId = 1;
}
