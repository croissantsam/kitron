/**
 * `@kitron/notification` — native user notifications (spec §42).
 *
 * macOS uses `UNUserNotificationCenter`. Gated by the `notification` permission
 * (spec §23).
 */

import {
  KitronError,
  getNativeModule,
  parseEnvelope,
} from "@kitron/core";

function requireNative() {
  const native = getNativeModule();
  if (!native) {
    throw new KitronError({
      code: "KITRON_UNAVAILABLE",
      message: "notification APIs require the Kitron native runtime",
    });
  }
  return native;
}

export interface NotificationOptions {
  title?: string;
  body?: string;
  /** Play the default sound (default true). */
  sound?: boolean;
}

export const notification = {
  /** Shows a native notification. */
  async show(options: NotificationOptions = {}): Promise<void> {
    await requireNative().notificationShow(JSON.stringify(options ?? {}));
  },
};
