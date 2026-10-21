/**
 * Example UI — the three Kitron runtime surfaces in one screen (spec §9):
 *
 *   1. `app`     — process identity + lifecycle events (JSI snapshot)
 *   2. `backend` — typed RPC to the embedded Node.js backend
 *   3. `window`  — native multi-window management through the Rust core
 *
 * Everything here is plain React Native: no webview, no HTML.
 */

import React, { useCallback, useEffect, useState } from "react";
import { Button, ScrollView, StyleSheet, Text, View } from "react-native";

import { app, backend, clipboard, KitronError, logger, menu, shell, window } from "kitron";

interface ProgressEvent {
  step: number;
  of: number;
}

export default function HelloKitronExample() {
  const [identity, setIdentity] = useState("…");
  const [sum, setSum] = useState("…");
  const [windows, setWindows] = useState<number>(0);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [log, setLog] = useState<string[]>([]);

  const append = useCallback((line: string) => {
    setLog((previous) => [line, ...previous].slice(0, 8));
  }, []);

  useEffect(() => {
    // 1. RN ⇄ Rust: synchronous JSI snapshot.
    try {
      const info = app.info();
      setIdentity(`${info.name} ${info.version} · pid ${info.pid}`);
      logger.info("app.info", info);
    } catch (error) {
      setIdentity(KitronError.from(error).message);
    }
    setWindows(window.list().length);

    // 2. RN ⇄ Node: typed RPC over the Kitron IPC transport.
    backend
      .invoke<{ sum: number }>("math.add", { a: 19, b: 23 })
      .then((result) => {
        setSum(`19 + 23 = ${result.sum}`);
        append(`backend.invoke → {sum: ${result.sum}}`);
      })
      .catch((error: unknown) => {
        const typed = KitronError.from(error);
        setSum(`${typed.code}: ${typed.message}`);
      });

    // 3. Node → RN push events.
    const offProgress = app.on("report.progress", (event) => {
      const payload = event.payload as ProgressEvent;
      setProgress(payload);
      append(`report.progress → step ${payload.step + 1}/${payload.of}`);
    });

    const offBackend = app.on("backend.event", (event) => {
      append(`backend.event → ${JSON.stringify(event.payload)}`);
    });

    // Native menu (§18): the items are declared in TypeScript, rendered by
    // AppKit, and their `action` ids come back as events.
    const offMenu = menu.onAction((action) => {
      append(`menu.action → ${action}`);
    });
    void menu
      .setApplicationMenu([
        {
          label: "Kitron",
          items: [
            { label: "Run report", action: "run-report", accelerator: "CmdOrCtrl+R" },
            { type: "separator" },
            { label: "Copy greeting", action: "copy-greeting" },
          ],
        },
        {
          label: "Edit",
          items: [
            { label: "Cut", action: "edit-cut" },
            { label: "Copy", action: "edit-copy" },
            { label: "Paste", action: "edit-paste" },
          ],
        },
      ])
      .catch((error: unknown) => {
        append(`menu → ${KitronError.from(error).message}`);
      });

    void app.onReady().then(() => append("app.ready"));

    return () => {
      offProgress();
      offBackend();
      offMenu();
    };
  }, [append]);

  const openWindow = useCallback(async () => {
    try {
      const created = await window.create({
        title: "Kitron — secondary window",
        width: 480,
        height: 320,
      });
      setWindows(window.list().length);
      append(`window.create → id ${created.id}`);
    } catch (error) {
      append(`window.create failed → ${KitronError.from(error).message}`);
    }
  }, [append]);

  const runReport = useCallback(async () => {
    await backend.invoke("report.run", { steps: 3 });
  }, []);

  // Phase-2 native APIs (§42) — clipboard + shell, gated by the permissions
  // declared in kitron.config.ts (§23).
  const copyGreeting = useCallback(async () => {
    try {
      await clipboard.write("Hello from Kitron");
      append(`clipboard → ${await clipboard.read()}`);
    } catch (error) {
      append(`clipboard failed → ${KitronError.from(error).message}`);
    }
  }, [append]);

  const openReadme = useCallback(async () => {
    try {
      await shell.openUrl("https://github.com/croissantlabs/kitron");
      append("shell.openUrl → handed to the system handler");
    } catch (error) {
      append(`shell.openUrl failed → ${KitronError.from(error).message}`);
    }
  }, [append]);

  return (
    <ScrollView style={styles.root} contentContainerStyle={styles.content}>
      <Text style={styles.title}>Kitron</Text>
      <Text style={styles.subtitle}>React Native · Node.js · Rust — no Chromium</Text>

      <View style={styles.card}>
        <Text style={styles.label}>app (JSI → Rust)</Text>
        <Text style={styles.value}>{identity}</Text>
      </View>

      <View style={styles.card}>
        <Text style={styles.label}>backend (IPC → Node)</Text>
        <Text style={styles.value}>{sum}</Text>
      </View>

      <View style={styles.card}>
        <Text style={styles.label}>window (Rust → AppKit)</Text>
        <Text style={styles.value}>{windows} window(s) open</Text>
      </View>

      <View style={styles.row}>
        <Button title="Open a window" onPress={() => void openWindow()} />
        <View style={styles.spacer} />
        <Button title="Run report" onPress={() => void runReport()} />
      </View>

      <View style={styles.row}>
        <Button title="Copy greeting" onPress={() => void copyGreeting()} />
        <View style={styles.spacer} />
        <Button title="Open the docs" onPress={() => void openReadme()} />
      </View>

      {progress ? (
        <Text style={styles.progress}>
          progress {progress.step + 1}/{progress.of}
        </Text>
      ) : null}

      <View style={styles.log}>
        {log.map((line, index) => (
          <Text key={`${index}-${line}`} style={styles.logLine}>
            {line}
          </Text>
        ))}
      </View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, backgroundColor: "#f6f1e7" },
  content: { padding: 24, gap: 12 },
  title: { fontSize: 32, fontWeight: "700", color: "#1c1c1e" },
  subtitle: { fontSize: 13, color: "#6c6c70", marginBottom: 8 },
  card: {
    backgroundColor: "#ffffff",
    borderRadius: 10,
    padding: 14,
    borderWidth: 1,
    borderColor: "#e4ded2",
  },
  label: { fontSize: 11, letterSpacing: 1, textTransform: "uppercase", color: "#8a8377" },
  value: { fontSize: 16, color: "#1c1c1e", marginTop: 4 },
  row: { flexDirection: "row", alignItems: "center", marginTop: 4 },
  spacer: { width: 12 },
  progress: { fontSize: 13, color: "#3860d4" },
  log: { marginTop: 12, gap: 4 },
  logLine: { fontSize: 12, color: "#3c3c43", fontFamily: "Menlo" },
});
