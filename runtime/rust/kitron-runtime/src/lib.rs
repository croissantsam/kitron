//! kitron-runtime — native operations behind the typed TS facade.
//!
//! Rules (§7, §32):
//! - no business logic here, only OS access
//! - never block the UI thread: heavy work is async / background
//! - macOS uses AppKit/Foundation/UserNotifications via winit + objc2
//! - Windows uses Win32/WinRT via winit + windows crate

pub mod app;
pub mod clipboard;
pub mod crash;
pub mod dialogs;
pub mod ipc_server;
pub mod menu;
pub mod node_sidecar;
pub mod notification;
pub mod os_events;
pub mod paint;
pub mod process_mgr;
pub mod shell;
pub mod shortcuts;
pub mod tray;
pub mod ui;
pub mod updater;
pub mod windows;

pub use kitron_core as core;
