//! macOS application menu, context popup and status-bar tray (Tranche menus).
//!
//! Built with objc2 over AppKit — no extra windowing crate. Menu/tray
//! actions funnel through one `MenuTarget` object whose `representedObject`
//! carries `menu:<id>` / `tray:<id>`; clicks are written to stdout as IPC
//! events for the hub (`native.menu.click`, `native.tray.click`).

use anyhow::Result;
use objc2::runtime::{AnyObject, Bool};
use objc2::{class, define_class, msg_send, sel, ClassType};
use objc2_foundation::NSObject;
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::{Mutex, OnceLock};

define_class!(
    #[unsafe(super(NSObject))]
    struct MenuTarget;

    impl MenuTarget {
        #[unsafe(method(onKitronAction:))]
        fn __on_action(&self, sender: *mut AnyObject) {
            unsafe {
                if sender.is_null() {
                    return;
                }
                let obj: *mut AnyObject = msg_send![sender, representedObject];
                if obj.is_null() {
                    return;
                }
                let c: *const c_char = msg_send![obj, UTF8String];
                if c.is_null() {
                    return;
                }
                let tag = CStr::from_ptr(c).to_string_lossy().into_owned();
                if let Some(id) = tag.strip_prefix("menu:") {
                    let payload = serde_json::json!({"id": id}).to_string();
                    write_event("native.menu.click", &payload);
                } else if let Some(id) = tag.strip_prefix("tray:") {
                    let payload = serde_json::to_string(id).unwrap_or_default();
                    write_event("native.tray.click", &payload);
                }
            }
        }
    }
);

fn shared_target() -> &'static MenuTarget {
    static TARGET: OnceLock<usize> = OnceLock::new();
    let ptr = TARGET.get_or_init(|| unsafe {
        let obj: *mut MenuTarget = msg_send![MenuTarget::class(), new];
        obj as usize
    });
    unsafe { &*(*ptr as *const MenuTarget) }
}

fn write_event(channel: &str, payload_json: &str) {
    use std::io::Write;
    let mut out = std::io::stdout();
    let _ = writeln!(
        out,
        "{{\"channel\":\"{channel}\",\"payload\":{payload_json}}}"
    );
    let _ = out.flush();
}

fn nsstring(s: &str) -> *mut AnyObject {
    use objc2::rc::Retained;
    use objc2_foundation::NSString;
    let ns = NSString::from_str(s);
    let ptr = Retained::as_ptr(&ns) as *mut AnyObject;
    std::mem::forget(ns);
    ptr
}

fn rust_string(ns: *mut AnyObject) -> String {
    if ns.is_null() {
        return String::new();
    }
    unsafe {
        let c: *const c_char = msg_send![ns, UTF8String];
        if c.is_null() {
            return String::new();
        }
        CStr::from_ptr(c).to_string_lossy().into_owned()
    }
}

/// Accelerator like "CmdOrCtrl+Shift+X" → (keyEquivalent, modifierMask).
/// Exported for tests.
pub fn parse_accelerator(acc: &str) -> (String, u64) {
    const CMD: u64 = 1 << 20;
    const SHIFT: u64 = 1 << 17;
    const OPT: u64 = 1 << 19;
    const CTRL: u64 = 1 << 18;
    let mut mask = 0u64;
    let mut key = String::new();
    for part in acc.split('+') {
        match part.to_lowercase().as_str() {
            "cmd" | "cmdorctrl" | "command" | "ctrl" | "control" => {
                mask |= if cfg!(target_os = "macos") { CMD } else { CTRL }
            }
            "shift" => mask |= SHIFT,
            "alt" | "option" => mask |= OPT,
            _ => key = part.to_lowercase(),
        }
    }
    (key, mask)
}

fn role_selector(role: &str) -> Option<objc2::runtime::Sel> {
    Some(match role {
        "quit" => sel!(terminate:),
        "hide" => sel!(hide:),
        "hideOthers" => sel!(hideOtherApplications:),
        "unhide" => sel!(unhideAllApplications:),
        "close" => sel!(performClose:),
        "minimize" => sel!(performMiniaturize:),
        "zoom" => sel!(performZoom:),
        "copy" => sel!(copy:),
        "cut" => sel!(cut:),
        "paste" => sel!(paste:),
        "selectAll" => sel!(selectAll:),
        "undo" => sel!(undo:),
        "redo" => sel!(redo:),
        _ => return None,
    })
}

fn build_item(item: &serde_json::Value) -> Result<*mut AnyObject> {
    let kind = item.get("type").and_then(|v| v.as_str()).unwrap_or("normal");
    if kind == "separator" {
        let sep: *mut AnyObject = unsafe { msg_send![class!(NSMenuItem), separatorItem] };
        return Ok(sep);
    }
    let label = item.get("label").and_then(|v| v.as_str()).unwrap_or("");
    let target = shared_target() as *const MenuTarget as *mut AnyObject;
    unsafe {
        let ns_item: *mut AnyObject = msg_send![
            class!(NSMenuItem),
            alloc
        ];
        let ns_item: *mut AnyObject = msg_send![
            ns_item,
            initWithTitle: nsstring(label),
            action: sel!(onKitronAction:),
            keyEquivalent: nsstring("")
        ];
        let _: () = msg_send![ns_item, setTarget: target];
        if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
            let _: () = msg_send![ns_item, setRepresentedObject: nsstring(&format!("menu:{id}"))];
        }
        if let Some(role) = item.get("role").and_then(|v| v.as_str()) {
            if let Some(action) = role_selector(role) {
                let _: () = msg_send![ns_item, setAction: action];
                let null: *mut AnyObject = std::ptr::null_mut();
                let _: () = msg_send![ns_item, setTarget: null];
            }
        }
        if let Some(acc) = item.get("accelerator").and_then(|v| v.as_str()) {
            let (key, mask) = parse_accelerator(acc);
            if !key.is_empty() {
                let _: () = msg_send![ns_item, setKeyEquivalent: nsstring(&key)];
                let _: () = msg_send![ns_item, setKeyEquivalentModifierMask: mask];
            }
        }
        if item.get("enabled").and_then(|v| v.as_bool()) == Some(false) {
            let no: Bool = Bool::NO;
            let _: () = msg_send![ns_item, setEnabled: no];
        }
        if item.get("checked").and_then(|v| v.as_bool()) == Some(true) {
            let _: () = msg_send![ns_item, setState: 1i64];
        }
        if let Some(sub) = item.get("submenu").and_then(|v| v.as_array()) {
            let menu = build_menu(sub)?;
            let _: () = msg_send![ns_item, setSubmenu: menu];
        }
        Ok(ns_item)
    }
}

fn build_menu(items: &[serde_json::Value]) -> Result<*mut AnyObject> {
    unsafe {
        let menu: *mut AnyObject = msg_send![class!(NSMenu), alloc];
        let menu: *mut AnyObject = msg_send![menu, initWithTitle: nsstring("")];
        let _: () = msg_send![menu, setAutoenablesItems: Bool::NO];
        for item in items {
            let ns_item = build_item(item)?;
            let _: () = msg_send![menu, addItem: ns_item];
        }
        Ok(menu)
    }
}

/// Install the application main menu from a `MenuTemplate` JSON value.
pub fn apply_menu_template(template: &serde_json::Value) -> Result<()> {
    let items: Vec<serde_json::Value> = template
        .get("appMenu")
        .and_then(|v| v.as_array())
        .cloned()
        .or_else(|| template.as_array().cloned())
        .unwrap_or_default();
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let menu = build_menu(&items)?;
        let _: () = msg_send![app, setMainMenu: menu];
    }
    Ok(())
}

/// Popup a context menu built from a JSON array at window coords.
pub fn popup_menu(items: &serde_json::Value, x: f64, y: f64) -> Result<()> {
    let arr = items.as_array().cloned().unwrap_or_default();
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let key: *mut AnyObject = msg_send![app, keyWindow];
        if key.is_null() {
            anyhow::bail!("no key window for menu popup");
        }
        let view: *mut AnyObject = msg_send![key, contentView];
        let menu = build_menu(&arr)?;
        let pt = objc2_foundation::NSPoint::new(x, y);
        let _: () = msg_send![menu, popUpMenuPositioningItem: std::ptr::null_mut::<AnyObject>(), atLocation: pt, inView: view];
    }
    Ok(())
}

fn trays() -> std::sync::MutexGuard<'static, HashMap<String, usize>> {
    static TRAYS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    TRAYS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|e| e.into_inner())
}

/// Create (or update) a status-bar item. Returns nothing; errors are typed.
pub fn tray_create(
    id: &str,
    title: Option<&str>,
    tooltip: Option<&str>,
    menu: Option<&serde_json::Value>,
) -> Result<()> {
    unsafe {
        let bar: *mut AnyObject = msg_send![class!(NSStatusBar), systemStatusBar];
        // NSSquareStatusItemLength == -2.0
        let item: *mut AnyObject = msg_send![bar, statusItemWithLength: -2.0f64];
        let button: *mut AnyObject = msg_send![item, button];
        if let Some(t) = title {
            let _: () = msg_send![button, setTitle: nsstring(t)];
        }
        if let Some(tt) = tooltip {
            let _: () = msg_send![item, setToolTip: nsstring(tt)];
        }
        if let Some(m) = menu {
            if let Some(arr) = m.as_array() {
                let ns_menu = build_menu(arr)?;
                let _: () = msg_send![item, setMenu: ns_menu];
            }
        } else {
            let target = shared_target() as *const MenuTarget as *mut AnyObject;
            let _: () = msg_send![
                button,
                setTarget: target
            ];
            let _: () = msg_send![button, setAction: sel!(onKitronAction:)];
            let _: () = msg_send![
                button,
                setRepresentedObject: nsstring(&format!("tray:{id}"))
            ];
        }
        trays().insert(id.to_string(), item as usize);
        // Retained by the status bar; the map keeps a weak record only.
        let _: *mut AnyObject = msg_send![item, retain];
    }
    Ok(())
}

pub fn tray_destroy(id: &str) -> Result<()> {
    let item = trays().remove(id);
    if let Some(addr) = item {
        let ptr = addr as *mut AnyObject;
        unsafe {
            let bar: *mut AnyObject = msg_send![class!(NSStatusBar), systemStatusBar];
            let _: () = msg_send![bar, removeStatusItem: ptr];
            let _: () = msg_send![ptr, release];
        }
    }
    Ok(())
}

/// Top-level window list for `desktopCapturer` (no event loop needed).
pub fn window_list() -> Result<Vec<serde_json::Value>> {
    use core_foundation::array::CFArray;
    use core_foundation::base::TCFType;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::number::{CFNumber, CFNumberRef};
    use core_foundation::string::{CFString, CFStringRef};
    use core_graphics::window::{
        kCGNullWindowID, kCGWindowListExcludeDesktopElements, kCGWindowListOptionOnScreenOnly,
        CGWindowListCopyWindowInfo,
    };

    let list: CFArray<*const std::ffi::c_void> = unsafe {
        CFArray::wrap_under_create_rule(CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
            kCGNullWindowID,
        ) as _)
    };
    let get_str = |dict: &CFDictionary, key: &str| -> String {
        dict.find(CFString::new(key).as_CFTypeRef())
            .and_then(|v| {
                let ptr = *v as CFStringRef;
                Some(unsafe { CFString::wrap_under_get_rule(ptr) }.to_string())
            })
            .unwrap_or_default()
    };
    let get_num = |dict: &CFDictionary, key: &str| -> i64 {
        dict.find(CFString::new(key).as_CFTypeRef())
            .and_then(|v| {
                let ptr = *v as CFNumberRef;
                unsafe { CFNumber::wrap_under_get_rule(ptr) }.to_i64()
            })
            .unwrap_or(0)
    };
    let mut out = Vec::new();
    for i in 0..list.len() {
        let Some(item) = list.get(i) else {
            continue;
        };
        let dict = unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
        let name = get_str(&dict, "kCGWindowName");
        let owner = get_str(&dict, "kCGWindowOwnerName");
        if name.is_empty() && owner.is_empty() {
            continue;
        }
        out.push(serde_json::json!({
            "id": format!("window:{}", get_num(&dict, "kCGWindowNumber")),
            "name": if name.is_empty() { owner.clone() } else { format!("{owner} — {name}") },
            "kind": "window",
        }));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accelerator_parsing() {
        assert_eq!(parse_accelerator("CmdOrCtrl+Shift+X"), ("x".into(), (1 << 20) | (1 << 17)));
        assert_eq!(parse_accelerator("Alt+F4"), ("f4".into(), 1 << 19));
        assert_eq!(parse_accelerator("Ctrl+C"), ("c".into(), 1 << 20));
    }

    #[test]
    fn roles_known() {
        assert!(role_selector("quit").is_some());
        assert!(role_selector("copy").is_some());
        assert!(role_selector("bogus").is_none());
    }
}
