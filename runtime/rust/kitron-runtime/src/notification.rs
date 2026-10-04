//! Notifications: UserNotifications (macOS) / Windows notifications.
pub fn show(title: String, body: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // MVP: route through `notify-rust` (which uses NSUserNotificationCenter
        // fallback). A future pass binds UNUserNotificationCenter directly
        // via objc2-user-notifications for action buttons.
        notify_rust::Notification::new()
            .summary(&title)
            .body(&body)
            .show()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(target_os = "macos"))]
    {
        notify_rust::Notification::new()
            .summary(&title)
            .body(&body)
            .show()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
