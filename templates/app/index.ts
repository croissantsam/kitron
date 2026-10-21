/**
 * Kitron app entry — registers the root component used by the Rust-created
 * native window (see the AppDelegate shipped in the macOS template).
 */

import { AppRegistry } from "react-native";

import App from "./App";

AppRegistry.registerComponent("KitronApp", () => App);
