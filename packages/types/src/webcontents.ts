/**
 * `webContents`-equivalent surface — Tranche B.
 * Per-window web content control (execute JS, zoom, find, downloads…).
 * Backed by the Rust host driving the real WebView.
 */

export interface EvalResult<T = unknown> {
  ok: boolean;
  value?: T;
  error?: string;
}

export interface FindInPageOptions {
  forward?: boolean;
  matchCase?: boolean;
}

export interface ZoomOptions {
  factor: number;
}

export interface DownloadURLOptions {
  filename?: string;
}

export interface PrintToPDFOptions {
  /** Honored subset: full-page render. Margins/pageSize are accepted but
   * currently informational (native vector PDF, default page setup). */
  landscape?: boolean;
  printBackground?: boolean;
  pageSize?: "A4" | "Letter" | "Legal";
  marginTop?: number;
  marginBottom?: number;
  marginLeft?: number;
  marginRight?: number;
}

export interface WebContentsInfo {
  id: number;
  url: string;
  title: string;
}
