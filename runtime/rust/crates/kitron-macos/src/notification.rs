//! User notifications through `UNUserNotificationCenter` (spec §42).
//!
//! Gated by the `notification` permission (spec §23).

use kitron_protocol::KitronError;
use objc2_foundation::{NSError, NSString};
use objc2_user_notifications::{
    UNAuthorizationOptions, UNMutableNotificationContent, UNNotificationRequest,
    UNNotificationSound, UNUserNotificationCenter,
};
use serde_json::Value;

use crate::core;

pub(crate) fn show(args_json: &str) -> Result<(), KitronError> {
    core().require_permission("notification")?;
    let parsed: Value = serde_json::from_str(args_json).unwrap_or(Value::Null);
    let title = parsed.get("title").and_then(Value::as_str).unwrap_or("Kitron");
    let body = parsed.get("body").and_then(Value::as_str).unwrap_or("");
    let sound = parsed.get("sound").and_then(Value::as_bool).unwrap_or(true);

    let center = UNUserNotificationCenter::currentNotificationCenter();

    // Best-effort authorization request (fire and forget).
    let auth_block = block2::RcBlock::new(|_granted: objc2::runtime::Bool, _err: *mut NSError| {});
    center.requestAuthorizationWithOptions_completionHandler(
        UNAuthorizationOptions(7), // alert + sound + badge
        &auth_block,
    );

    let content = UNMutableNotificationContent::new();
    content.setTitle(&NSString::from_str(title));
    content.setBody(&NSString::from_str(body));
    if sound {
        content.setSound(Some(&UNNotificationSound::defaultSound()));
    }

    let id = NSString::from_str("kitron-notification");
    let request =
        UNNotificationRequest::requestWithIdentifier_content_trigger(&id, &content, None);
    let add_block = block2::RcBlock::new(|_err: *mut NSError| {});
    center.addNotificationRequest_withCompletionHandler(&request, Some(&add_block));
    Ok(())
}