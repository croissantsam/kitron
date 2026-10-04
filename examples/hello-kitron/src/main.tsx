import React, { useState } from "react";
import { Button, Text, View, render } from "@kitron/react";
import { window } from "kitron";

export async function main(): Promise<string> {
  const mainWindow = await window.create({ width: 1200, height: 800, title: "Hello Kitron" });

  function App() {
    const [count, setCount] = useState(0);
    return (
      <View style={{ flex: 1, alignItems: "center", justifyContent: "center" }}>
        <Text style={{ fontSize: 28 }}>Hello Kitron</Text>
        <Button title={`Clicked ${count} times`} onPress={() => setCount((c) => c + 1)} />
      </View>
    );
  }

  await render(<App />, mainWindow.id);
  return mainWindow.id;
}
