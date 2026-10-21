//! Application menu bootstrap (spec §18).
//!
//! The MVP installs a standard macOS application menu (About / Hide / Quit,
//! Edit, Window). The editable `menu.create([...])` API is phase 2 — this
//! gives apps a correct default (Quit, Copy/Paste, Minimize) for free.

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2::runtime::Sel;
use objc2::sel;
use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
use objc2_foundation::NSString;

/// Installs a standard application menu if none exists yet.
/// Menu shape mirrors the default macOS template app.
pub(crate) fn install_application_menu(app_name: &str) {
    let Some(mtm) = MainThreadMarker::new() else {
        core_debug("install_application_menu called off the main thread; skipping");
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
    add_item(
        mtm,
        &app_menu,
        &format!("Hide {app_name}"),
        Some(sel!(hide:)),
        "h",
    );
    add_separator(mtm, &app_menu);
    add_item(
        mtm,
        &app_menu,
        &format!("Quit {app_name}"),
        Some(sel!(terminate:)),
        "q",
    );
    let app_item = new_item(
        mtm,
        app_name,
        None,
        "",
    );
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

    fn core_debug(message: &str) {
        eprintln!("[warn] [kitron:menu] {message}");
    }
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
