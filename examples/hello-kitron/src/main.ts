import { window } from "kitron";
import { Text, View, render } from "@kitron/ui";

export async function main(): Promise<string> {
  const mainWindow = await window.create({ width: 1200, height: 800, title: "Hello Kitron" });
  await render(
    mainWindow.id,
    View({ style: { flex: 1, align: "center", justify: "center" } },
      Text({ style: { fontSize: 28 } }, "Hello Kitron")),
  );
  return mainWindow.id;
}
