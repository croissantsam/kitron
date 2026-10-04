/** Auto-updater API — §46. */
export interface UpdateInfo {
  available: boolean;
  version?: string;
  notes?: string;
  url?: string;
}

export interface UpdateProgress {
  percent: number;
  bytesTransferred: number;
  totalBytes: number;
}
