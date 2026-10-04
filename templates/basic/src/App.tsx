import { window } from "kitron";

// First vertical slice (§41): open one native window.
// No Rust/Swift/ObjC/C++/Win32 written by the app developer.
const mainWindow = await window.create({
  width: 1200,
  height: 800,
  title: "My App",
});

console.log("window:", mainWindow);
