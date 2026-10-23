//! System tray / status-bar items through `NSStatusBar` (spec §42).
//!
//! Main-thread only (AppKit). Gated by the `tray` permission (spec §23).

use kitron_protocol::KitronError;
use objc2::rc::Retained;
use objc2_app_kit::{NSStatusBar, NSStatusItem};
use objc2_foundation::{NSString, MainThreadMarker};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use crate::core;
use crate::menu;

static ITEMS: OnceLock<Mutex<BTreeMap<u64, usize>>> = OnceLock::new();
static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn items() -> &'static Mutex<BTreeMap<u64, usize>> {
    ITEMS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(crate) fn create(args_json: &str) -> Result<Value, KitronError> {
    core().require_permission("tray")?;
    let mtm = MainThreadMarker::new().ok_or_else(|| {
        KitronError::code("THREAD_MAIN_REQUIRED", "tray access must run on the main thread")
    })?;
    let parsed: Value = serde_json::from_str(args_json).unwrap_or(Value::Null);
    let title = parsed.get("title").and_then(Value::as_str).unwrap_or("");
    let tooltip = parsed.get("tooltip").and_then(Value::as_str);

    let status_bar = NSStatusBar::systemStatusBar();
    let item = status_bar.statusItemWithLength(-1.0);
    if !title.is_empty() {
        if let Some(button) = item.button(mtm) {
            button.setTitle(&NSString::from_str(title));
        }
    }
    if let Some(t) = tooltip {
        if let Some(button) = item.button(mtm) {
            button.setToolTip(Some(&NSString::from_str(t)));
        }
    }
    if let Some(menu_json) = parsed.get("menu") {
        let serialized = menu_json.to_string();
        if let Ok(built) = menu::build_menu(&serialized) {
            item.setMenu(Some(&built));
        }
    }

    let raw = Retained::into_raw(item) as usize;
    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    items().lock().unwrap().insert(id, raw);
    Ok(serde_json::json!({ "id": id }))
}

pub(crate) fn set_title(id: u64, title: &str) -> Result<(), KitronError> {
    core().require_permission("tray")?;
    let mtm = MainThreadMarker::new().ok_or_else(|| {
        KitronError::code("THREAD_MAIN_REQUIRED", "tray access must run on the main thread")
    })?;
    let guard = items().lock().unwrap();
    let raw = *guard.get(&id).ok_or_else(|| {
        KitronError::code("TRAY_NOT_FOUND", format!("no tray item with id {id}"))
    })?;
    let item: Retained<NSStatusItem> = unsafe { Retained::retain(raw as *mut NSStatusItem).unwrap() };
    if let Some(button) = item.button(mtm) {
        button.setTitle(&NSString::from_str(title));
    }
    Ok(())
}

pub(crate) fn remove(id: u64) -> Result<(), KitronError> {
    core().require_permission("tray")?;
    let _mtm = MainThreadMarker::new().ok_or_else(|| {
        KitronError::code("THREAD_MAIN_REQUIRED", "tray access must run on the main thread")
    })?;
    let mut guard = items().lock().unwrap();
    if let Some(raw) = guard.remove(&id) {
        // Drop the retained item.
        let _ = unsafe { Retained::<NSStatusItem>::from_raw(raw as *mut NSStatusItem) };
        Ok(())
    } else {
        Err(KitronError::code("TRAY_NOT_FOUND", format!("no tray item with id {id}")))
    }
}