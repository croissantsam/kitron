//! Application menus (spec §18).
//!
//! Two entry points:
//!
//! - `install_application_menu` — the standard macOS menu the runtime
//!   installs when the embedder has none (About / Hide / Quit, Edit, Window).
//! - `set_application_menu` — the editable menu an app installs from
//!   JavaScript. Items carrying an `action` id dispatch that id back to JS
//!   through the runtime event bus (`menu.action`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{define_class, msg_send, sel, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSEventModifierFlags, NSMenu,
    NSMenuItem,
};
use objc2_foundation::{NSInteger, NSObject, NSObjectProtocol, NSString};
use serde::Deserialize;

/// Action id per menu-item tag (the tag is the only thing the target object
/// receives when a menu item is clicked).
fn action_registry() -> &'static Mutex<HashMap<NSInteger, String>> {
    static REGISTRY: OnceLock<Mutex<HashMap<NSInteger, String>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_tag() -> NSInteger {
    static NEXT: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst) as NSInteger
}

define_class!(
    /// Target of every Kitron menu item: turns a click into a `menu.action`
    /// event carrying the action id the app registered.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    pub(super) struct MenuTarget;

    unsafe impl NSObjectProtocol for MenuTarget {}

    impl MenuTarget {
        #[unsafe(method(kitronMenuItemClicked:))]
        fn kitron_menu_item_clicked(&self, sender: &NSMenuItem) {
            let tag = sender.tag();
            let action = action_registry()
                .lock()
                .ok()
                .and_then(|registry| registry.get(&tag).cloned());
            let Some(action) = action else {
                return;
            };
            crate::menu_emit_action(&action);
        }
    }
);

/// The shared target object (kept alive for the process lifetime).
///
/// The class is `MainThreadOnly`, so it cannot be `Send`/`Sync` on its own —
/// the handle asserts the invariant (the target is only ever touched from the
/// main thread).
struct MenuTargetHandle(Retained<MenuTarget>);

// SAFETY: the target is only created and used on the main thread; the handle
// is a process-lifetime singleton that never leaves it.
unsafe impl Send for MenuTargetHandle {}
unsafe impl Sync for MenuTargetHandle {}

fn menu_target() -> Retained<MenuTarget> {
    static TARGET: OnceLock<MenuTargetHandle> = OnceLock::new();
    let handle = TARGET.get_or_init(|| {
        let mtm = MainThreadMarker::new().expect("menu target needs the main thread");
        let allocated = MenuTarget::alloc(mtm);
        let initialized: Option<Retained<MenuTarget>> = unsafe { msg_send![allocated, init] };
        MenuTargetHandle(initialized.expect("menu target init"))
    });
    handle.0.clone()
}

// ---------------------------------------------------------------------------
// Definition (JSON from JS)
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct MenuDefinition {
    pub label: String,
    pub items: Vec<MenuItemDefinition>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct MenuItemDefinition {
    /// "separator" for a divider.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub label: Option<String>,
    /// Action id dispatched to JS when the item is clicked.
    pub action: Option<String>,
    /// Accelerator, e.g. "CmdOrCtrl+N".
    pub accelerator: Option<String>,
    /// Nested items → a submenu.
    pub items: Option<Vec<MenuItemDefinition>>,
    pub enabled: Option<bool>,
    pub checked: Option<bool>,
}

// ---------------------------------------------------------------------------
// Installation
// ---------------------------------------------------------------------------

/// Installs a standard application menu if none exists yet.
pub(crate) fn install_application_menu(app_name: &str) {
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("[warn] [kitron:menu] install_application_menu called off the main thread; skipping");
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    if app.mainMenu().is_some() {
        return; // Never override an embedder's menu.
    }

    let main = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("MainMenu"));

    // -- App menu ------------------------------------------------------------
    let app_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(app_name));
    add_item(
        mtm,
        &app_menu,
        &format!("About {app_name}"),
        Some(sel!(orderFrontStandardAboutPanel:)),
        "",
    );
    add_separator(mtm, &app_menu);
    add_item(mtm, &app_menu, &format!("Hide {app_name}"), Some(sel!(hide:)), "h");
    add_separator(mtm, &app_menu);
    add_item(
        mtm,
        &app_menu,
        &format!("Quit {app_name}"),
        Some(sel!(terminate:)),
        "q",
    );
    let app_item = new_item(mtm, app_name, None, "");
    app_item.setSubmenu(Some(&app_menu));

    // -- Edit menu ------------------------------------------------------------
    let edit_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("Edit"));
    add_item(mtm, &edit_menu, "Undo", Some(sel!(undo:)), "z");
    add_item(mtm, &edit_menu, "Redo", Some(sel!(redo:)), "Z");
    add_separator(mtm, &edit_menu);
    add_item(mtm, &edit_menu, "Cut", Some(sel!(cut:)), "x");
    add_item(mtm, &edit_menu, "Copy", Some(sel!(copy:)), "c");
    add_item(mtm, &edit_menu, "Paste", Some(sel!(paste:)), "v");
    add_item(mtm, &edit_menu, "Select All", Some(sel!(selectAll:)), "a");
    let edit_item = new_item(mtm, "Edit", None, "");
    edit_item.setSubmenu(Some(&edit_menu));

    // -- Window menu -----------------------------------------------------------
    let window_menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("Window"));
    add_item(
        mtm,
        &window_menu,
        "Minimize",
        Some(sel!(performMiniaturize:)),
        "m",
    );
    add_item(mtm, &window_menu, "Zoom", Some(sel!(performZoom:)), "");
    let window_item = new_item(mtm, "Window", None, "");
    window_item.setSubmenu(Some(&window_menu));

    main.addItem(&app_item);
    main.addItem(&edit_item);
    main.addItem(&window_item);

    app.setMainMenu(Some(&main));
}

/// Builds and installs the application menu described by the app (spec §18).
pub(crate) fn set_application_menu(definition_json: &str) -> Result<(), String> {
    let Some(mtm) = MainThreadMarker::new() else {
        return Err("menu.setApplicationMenu must run on the main thread".to_string());
    };
    let definition: Vec<MenuDefinition> =
        serde_json::from_str(definition_json).map_err(|error| error.to_string())?;

    let target = menu_target();
    let main = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str("MainMenu"));
    for menu in &definition {
        let submenu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(&menu.label));
        for item in &menu.items {
            if let Some(item) = build_item(mtm, item, &target) {
                submenu.addItem(&item);
            }
        }
        let menu_item = new_item(mtm, &menu.label, None, "");
        menu_item.setSubmenu(Some(&submenu));
        main.addItem(&menu_item);
    }

    NSApplication::sharedApplication(mtm).setMainMenu(Some(&main));
    Ok(())
}

/// Builds one menu item (recursively for submenus).
fn build_item(
    mtm: MainThreadMarker,
    definition: &MenuItemDefinition,
    target: &Retained<MenuTarget>,
) -> Option<Retained<NSMenuItem>> {
    if definition.kind.as_deref() == Some("separator") {
        return Some(NSMenuItem::separatorItem(mtm));
    }

    let label = definition.label.clone().unwrap_or_default();
    let (key, modifiers) = parse_accelerator(definition.accelerator.as_deref());

    let has_action = definition.action.is_some();
    let action = if has_action {
        Some(sel!(kitronMenuItemClicked:))
    } else {
        None
    };
    let item = new_item(mtm, &label, action, &key);
    if !modifiers.is_empty() {
        item.setKeyEquivalentModifierMask(modifiers);
    }
    if let Some(enabled) = definition.enabled {
        item.setEnabled(enabled);
    }
    if let Some(checked) = definition.checked {
        item.setState(if checked {
            NSControlStateValueOn
        } else {
            NSControlStateValueOff
        });
    }
    if let Some(action_id) = &definition.action {
        let tag = next_tag();
        if let Ok(mut registry) = action_registry().lock() {
            registry.insert(tag, action_id.clone());
        }
        unsafe { item.setTarget(Some(target.as_ref() as &AnyObject)) };
        item.setTag(tag);
    }
    if let Some(children) = &definition.items {
        let submenu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(&label));
        for child in children {
            if let Some(child) = build_item(mtm, child, target) {
                submenu.addItem(&child);
            }
        }
        item.setSubmenu(Some(&submenu));
    }
    Some(item)
}

/// Parses `"CmdOrCtrl+N"` into the key character and the modifier flags.
fn parse_accelerator(accelerator: Option<&str>) -> (String, NSEventModifierFlags) {
    let Some(accelerator) = accelerator else {
        return (String::new(), NSEventModifierFlags::empty());
    };
    let mut modifiers = NSEventModifierFlags::empty();
    let mut key = String::new();
    for part in accelerator.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "cmd" | "command" | "cmdorctrl" | "mod" => {
                modifiers |= NSEventModifierFlags::Command;
            }
            "ctrl" | "control" => modifiers |= NSEventModifierFlags::Control,
            "alt" | "option" => modifiers |= NSEventModifierFlags::Option,
            "shift" => modifiers |= NSEventModifierFlags::Shift,
            "" => {}
            other => key = other.to_string(),
        }
    }
    (key, modifiers)
}

fn new_item(
    mtm: MainThreadMarker,
    title: &str,
    action: Option<Sel>,
    key_equivalent: &str,
) -> Retained<NSMenuItem> {
    unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            action,
            &NSString::from_str(key_equivalent),
        )
    }
}

fn add_item(mtm: MainThreadMarker, menu: &NSMenu, title: &str, action: Option<Sel>, key: &str) {
    let item = new_item(mtm, title, action, key);
    menu.addItem(&item);
}

fn add_separator(mtm: MainThreadMarker, menu: &NSMenu) {
    menu.addItem(&NSMenuItem::separatorItem(mtm));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_accelerators() {
        let (key, modifiers) = parse_accelerator(Some("CmdOrCtrl+N"));
        assert_eq!(key, "n");
        assert!(modifiers.contains(NSEventModifierFlags::Command));

        let (key, modifiers) = parse_accelerator(Some("Ctrl+Shift+Alt+p"));
        assert_eq!(key, "p");
        assert!(modifiers.contains(NSEventModifierFlags::Control));
        assert!(modifiers.contains(NSEventModifierFlags::Shift));
        assert!(modifiers.contains(NSEventModifierFlags::Option));

        let (key, modifiers) = parse_accelerator(None);
        assert_eq!(key, "");
        assert!(modifiers.is_empty());
    }

    #[test]
    fn parses_a_menu_definition() {
        let definition = r#"[
            {"label":"File","items":[
                {"label":"New","action":"new-file","accelerator":"CmdOrCtrl+N"},
                {"type":"separator"},
                {"label":"Sub","items":[{"label":"Deep","action":"deep"}]}
            ]}
        ]"#;
        let parsed: Vec<MenuDefinition> = serde_json::from_str(definition).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].label, "File");
        assert_eq!(parsed[0].items.len(), 3);
        assert_eq!(parsed[0].items[0].action.as_deref(), Some("new-file"));
        assert_eq!(parsed[0].items[1].kind.as_deref(), Some("separator"));
        assert_eq!(
            parsed[0].items[2].items.as_ref().unwrap()[0].action.as_deref(),
            Some("deep")
        );
    }
}

// ---------------------------------------------------------------------------
// Test support
// ---------------------------------------------------------------------------

/// Helpers used by the crate's integration tests to exercise the menu
/// wiring without a human clicking the item.
#[doc(hidden)]
pub mod test_support {
    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2::{sel, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};

    /// Whether the action item was found in the installed menu.
    #[doc(hidden)]
    pub struct TestMenuHandle {
        pub item_found: bool,
    }

    /// Installs a one-item menu carrying `action` and reports whether the item
    /// ended up in the application menu.
    pub fn install_test_menu(action: &str) -> TestMenuHandle {
        let definition = format!(
            r#"[{{"label":"Test","items":[{{"label":"Probe","action":"{action}"}}]}}]"#
        );
        super::set_application_menu(&definition).expect("menu installs");
        let item_found = first_action_item().is_some();
        TestMenuHandle { item_found }
    }

    /// The first menu item that carries an action.
    fn first_action_item() -> Option<Retained<NSMenuItem>> {
        let mtm = MainThreadMarker::new()?;
        let app = NSApplication::sharedApplication(mtm);
        let main = app.mainMenu()?;
        for index in 0..main.numberOfItems() {
            let Some(menu) = main.itemAtIndex(index).and_then(|item| item.submenu()) else {
                continue;
            };
            if let Some(found) = find_action_item(&menu) {
                return Some(found);
            }
        }
        None
    }

    fn find_action_item(menu: &NSMenu) -> Option<Retained<NSMenuItem>> {
        for index in 0..menu.numberOfItems() {
            let Some(item) = menu.itemAtIndex(index) else {
                continue;
            };
            if item.action().is_some() {
                return Some(item);
            }
            if let Some(submenu) = item.submenu() {
                if let Some(found) = find_action_item(&submenu) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Triggers the action item the way AppKit does on a click.
    pub fn trigger_first_action_item() {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        let Some(item) = first_action_item() else {
            return;
        };
        let Some(action) = item.action() else {
            return;
        };
        let target: Option<Retained<AnyObject>> = item.target();
        unsafe {
            app.sendAction_to_from(action, target.as_deref(), Some(&item));
        }
        let _ = sel!(kitronMenuItemClicked:);
    }
}
