//! Native notifications (§17, §36).
//! TypeScript `native.notification.show()` → `notification.show` IPC →
//! `NotificationService` → per-OS implementation.

use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ShowOptions {
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub silent: bool,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub sound: Option<String>,
}

pub fn show(opts: ShowOptions) -> Result<()> {
    crate::platform::notification_show(&opts.title, &opts.body, opts.silent, opts.sound.as_deref())
}
