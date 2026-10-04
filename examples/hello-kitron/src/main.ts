import { window } from "kitron";

export async function main(): Promise<string> {
  const mainWindow = await window.create({ width: 1200, height: 800, title: "Hello Kitron" });
  return mainWindow.id;
}
