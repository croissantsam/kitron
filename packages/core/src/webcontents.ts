import type { FindInPageOptions, PrintToPDFOptions } from "@kitron/types";
import { getPeer } from "./transport.js";

/**
 * `webContents` equivalent — Tranche B.
 * Per-window content control; executed by the Rust host inside the real
 * WebView (JS evaluation round-trip, CSS zoom, downloads…).
 */
export class WebContents {
  constructor(readonly windowId: string) {}

  private call<T = unknown>(action: string, payload?: unknown): Promise<T> {
    return getPeer().invoke<T>(`webcontents.${action}`, {
      id: this.windowId,
      ...((payload ?? {}) as object),
    });
  }

  /** Evaluate JS in the page; resolves with the JSON value (or rejects). */
  async executeJavaScript<T = unknown>(code: string): Promise<T> {
    const res = await this.call<{ ok: boolean; value?: T; error?: string }>("evalJS", {
      code,
    });
    if (!res.ok) throw new Error(res.error ?? "evaluateJavaScript failed");
    return res.value as T;
  }

  getURL(): Promise<string> {
    return this.call("getURL");
  }

  getTitle(): Promise<string> {
    return this.call("getTitle");
  }

  async setZoomFactor(factor: number): Promise<void> {
    await this.call("setZoomFactor", { factor });
  }

  getZoomFactor(): Promise<number> {
    return this.call("getZoomFactor");
  }

  /** In-page search via the page's own find (match count unknown → 1/0). */
  async findInPage(text: string, options: FindInPageOptions = {}): Promise<number> {
    const caseArg = options.matchCase === true ? "false,true" : "false,false";
    const dirArg = options.forward === false ? "true" : "false";
    const code = `(window.find(${JSON.stringify(text)}, ${caseArg}, ${dirArg}) === true)`;
    const found = await this.executeJavaScript<boolean>(code).catch(() => false);
    return found ? 1 : 0;
  }

  stopFindInPage(): Promise<void> {
    return this.executeJavaScript("void 0").then(() => undefined);
  }

  /** Trigger a download through the page (anchor + download attr). */
  async downloadURL(url: string, filename?: string): Promise<void> {
    const dl = filename ? `a.download = ${JSON.stringify(filename)};` : "";
    const code = `(() => { const a = document.createElement('a'); a.href = ${JSON.stringify(url)}; ${dl} document.body.appendChild(a); a.click(); a.remove(); })()`;
    await this.executeJavaScript(code);
  }

  /** Print via the page dialog (printToPDF needs native capture — planned). */
  async print(): Promise<void> {
    await this.executeJavaScript("window.print()");
  }

  capturePage(): Promise<string> {
    return this.call("capturePage");
  }

  /** Render the page to a PDF data URL (native vector on macOS). */
  printToPDF(_options: PrintToPDFOptions = {}): Promise<string> {
    return this.call("printToPDF", {});
  }

  reload(): Promise<void> {
    return this.call("reload");
  }
}
