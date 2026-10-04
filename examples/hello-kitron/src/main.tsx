import React, { useState } from "react";
import { Button, FlatList, Pressable, StyleSheet, Text, View, render } from "react-native";
import { window } from "kitron";

const styles = StyleSheet.create({
  root: { flex: 1, alignItems: "center", justifyContent: "center", background: "#ffffff" },
  title: { fontSize: 28, color: "#111111" },
  row: { padding: 8 },
});

export async function main(): Promise<string> {
  const mainWindow = await window.create({ width: 1200, height: 800, title: "Hello Kitron" });

  function App() {
    const [items, setItems] = useState(["Alpha", "Beta"]);
    const [count, setCount] = useState(0);
    return (
      <View style={styles.root}>
        <Text style={styles.title}>Hello Kitron</Text>
        <FlatList
          data={items}
          keyExtractor={(item) => item}
          renderItem={({ item }) => (
            <Pressable onPress={() => setCount((c) => c + 1)}>
              <Text style={styles.row}>{item} ({count})</Text>
            </Pressable>
          )}
        />
        <Button title="Add" onPress={() => setItems((xs) => [...xs, `Item ${xs.length + 1}`])} />
      </View>
    );
  }

  await render(<App />, mainWindow.id);
  return mainWindow.id;
}
