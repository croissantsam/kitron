/**
 * Root component. Demonstrates the three runtime surfaces available to a
 * Kitron app (spec §9):
 *   1. `window`  — native multi-window management through the Rust core
 *   2. `backend` — typed RPC to the embedded Node.js backend
 *   3. `app`     — process identity + lifecycle events
 */

import React, { useEffect, useState } from "react";
import { StyleSheet, Text, View } from "react-native";

import { app, backend, KitronError, logger, window } from "kitron";

let primaryWindowId: number | null = null;
void primaryWindowId;

function HelloKitron() {
  const [identity, setIdentity] = useState("…");
  const [sum, setSum] = useState("…");
  const [windows, setWindows] = useState(1);

  useEffect(() => {
    logger.info("ui mounted");

    let disposed = false;

    const identityFallback = (message: string): void => {
      logger.warn(message);
      setIdentity(message);
    };

    try {
      const info = app.info();
      setIdentity(`${info.name} v${info.version}`);
      logger.info("app.info", info);
    } catch (error) {
      const typed = KitronError.from(error);
      identityFallback(`${typed.code}: ${typed.message}`);
    }

    backend
      .invoke<{ sum: number }>("math.add", { a: 19, b: 23 })
      .then((result) => {
        logger.info("backend.invoke result", result);
        if (!disposed) {
          setSum(`19 + 23 = ${result.sum}`);
        }
      })
      .catch((error: unknown) => {
        const typed = KitronError.from(error);
        logger.error("backend invoke failed", typed);
        if (!disposed) {
          setSum(`${typed.code}: ${typed.message}`);
        }
      });

    // Two-way events: the Node backend can push events to the UI.
    const off = app.on("backend.event", (event) => {
      logger.info("backend.event", event.payload);
    });

    app.onReady().then(() => logger.info("app.ready received"));

    return () => {
      disposed = true;
      off();
    };
  }, []);

  async function openMarkdownWindow() {
    const snapshot = await window.create({
      title: "Markdown Preview",
      width: 640,
      height: 480,
    });
    setWindows(window.list().length);
    logger.info("created secondary window", snapshot.id);
  }

  return (
    <View style={styles.container}>
      <Text style={styles.title}>Hello Kitron</Text>
      <Text style={styles.item}>{identity}</Text>
      <Text style={styles.item}>backend → {sum}</Text>
      <Text style={styles.item}>windows: {windows}</Text>
      <Text style={styles.link} onPress={() => void openMarkdownWindow()}>
        Open a second window →
      </Text>
      <Text style={styles.hint}>
        Quit with ⌘Q — the native menu is installed by the Rust core (spec §18).
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    alignItems: "center",
    justifyContent: "center",
    gap: 12,
    backgroundColor: "#f6f1e7",
  },
  title: { fontSize: 28, fontWeight: "700", color: "#1c1c1e" },
  item: { fontSize: 15, color: "#3c3c43" },
  link: { fontSize: 15, color: "#3860d4", padding: 8 },
  hint: { fontSize: 12, opacity: 0.6, textAlign: "center", color: "#3c3c43" },
});

export default HelloKitron;
