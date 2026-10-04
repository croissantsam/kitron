import type { AppInfo, AppPathName } from "@kitron/types";
import { getPeer } from "./transport.js";

/** App lifecycle API — §15. Usable from the Node backend and the renderer. */
export const app = {
  ready(): Promise<void> {
    return getPeer().invoke("app.ready");
  },

  quit(): Promise<void> {
    return getPeer().invoke("app.quit");
  },

  exit(code = 0): Promise<void> {
    return getPeer().invoke("app.exit", { code });
  },

  isReady(): Promise<boolean> {
    return getPeer().invoke("app.isReady");
  },

  getName(): Promise<string> {
    return getPeer().invoke("app.getName");
  },

  getVersion(): Promise<string> {
    return getPeer().invoke("app.getVersion");
  },

  getPath(name: AppPathName): Promise<string> {
    return getPeer().invoke("app.getPath", { name });
  },

  getInfo(): Promise<AppInfo> {
    return getPeer().invoke("app.getInfo");
  },

  onQuit(listener: () => void): () => void {
    return getPeer().subscribe("app.quit-requested", listener);
  },
};
