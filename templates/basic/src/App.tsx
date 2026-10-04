import { Button, Text, View, render } from "@kitron/ui";
import { window } from "kitron";

// Kitron owns the window and every pixel in it — no webview, no Chromium,
// no external renderer. See docs/UI.md for the component model.
const main = await window.create({
  width: 1200,
  height: 800,
  title: "My App",
});

let count = 0;
function tree() {
  return View(
    { style: { flex: 1, alignItems: "center", justifyContent: "center", background: "#ffffff" } },
    Text({ style: { fontSize: 28, color: "#111111" } }, "Hello Kitron"),
    Text({ style: { fontSize: 14, color: "#666666" } }, `Clicked ${count} times`),
    Button({ title: "Click me", onPress: () => { count++; render(main.id, tree()); } }),
  );
}

render(main.id, tree());
