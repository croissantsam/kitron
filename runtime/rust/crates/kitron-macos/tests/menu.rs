//! Integration test for the native menu (spec §18).
//!
//! Builds a menu with an action item, then triggers the item the way AppKit
//! does on a click (`NSApplication.sendAction:to:from:`) and asserts the
//! `menu.action` event reaches the runtime event bus.
//!
//! The test runs with `harness = false` (see Cargo.toml): AppKit is
//! main-thread only, and only the process' main thread can carry a
//! `MainThreadMarker`.

use std::time::Duration;

use kitron_macos::menu::test_support::{install_test_menu, trigger_first_action_item};

fn main() {
    let handle = install_test_menu("probe-action");
    assert!(handle.item_found, "the menu item was not installed");

    trigger_first_action_item();

    // The event is queued by the runtime; give the bus a moment to deliver it.
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    let mut found = false;
    while std::time::Instant::now() < deadline {
        let events = kitron_macos::take_test_events();
        if events.iter().any(|event| event.name == "menu.action") {
            found = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(found, "no menu.action event was emitted");

    println!("menu test: PASS");
}
