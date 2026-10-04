import { useState } from "react";
import { Button, Text, View, render } from "@kitron/react";
import { window } from "kitron";

// Mainline React, rendered by Kitron's own reconciler host —
// no react-native fork, no webview, no Chromium (see docs/UI.md).
const main = await window.create({
  width: 1200,
  height: 800,
  title: "My App",
});

function App() {
  const [count, setCount] = useState(0);
  return (
    <View style={{ flex: 1, alignItems: "center", justifyContent: "center", background: "#ffffff" }}>
      <Text style={{ fontSize: 28, color: "#111111" }}>Hello Kitron</Text>
      <Text style={{ fontSize: 14, color: "#666666" }}>Clicked {count} times</Text>
      <Button title="Click me" onPress={() => setCount((c) => c + 1)} />
    </View>
  );
}

await render(<App />, main.id);
