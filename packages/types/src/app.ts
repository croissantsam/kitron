/** App API types — §15. */

export type AppPathName =
  | "home"
  | "appData"
  | "userData"
  | "temp"
  | "exe"
  | "documents"
  | "downloads"
  | "desktop"
  | "pictures"
  | "music"
  | "videos"
  | "logs";

export interface AppInfo {
  name: string;
  version: string;
  locale: string;
  platform: NodeJS.Platform;
  arch: string;
  ready: boolean;
}
