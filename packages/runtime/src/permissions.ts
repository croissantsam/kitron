import { KitronError } from "@kitron/types";
import type { KitronPermissions } from "@kitron/types";

/** Build the permission gate installed into @kitron/core (§21). */
export function createPermissionChecker(
  permissions: KitronPermissions,
): (capability: string) => void {
  return (capability: string) => {
    const key = capability as keyof KitronPermissions;
    if (permissions[key] === false) {
      throw new KitronError(
        "PERMISSION_DENIED",
        `Permission "${capability}" is disabled in kitron.config.ts`,
      );
    }
  };
}
