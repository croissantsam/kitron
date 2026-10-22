//! Native menus on Windows — `HMENU` built from the portable `MenuDefinition`
//! plus an accelerator table (spec §18 on Windows).
//!
//! Wire parity with the macOS twin: the same JSON array (`MenuDefinition[]`)
//! arrives from `menu.setApplicationMenu`, clicks answer a `menu.action`
//! event carrying the item's `action` id, and accelerators parse from the
//! same "CmdOrCtrl+N" strings (Ctrl on Windows — the string is portable).
//!
//! Win32 menus are message-loop driven, so the built accelerator table is
//! handed to the RNW template through `kitron_windows_accelerators()` (its
//! loop calls `TranslateAcceleratorW` with it — integration point,
//! `docs/shell-contracts.md` §5).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateAcceleratorTableW, CreateMenu, CreatePopupMenu, DestroyMenu, DrawMenuBar,
    SetMenu, ACCEL, HMENU, MF_CHECKED, MF_DISABLED, MF_POPUP, MF_SEPARATOR, MF_STRING,
};

use kitron_core::shell::{MenuDefinition, MenuItemDefinition};
use kitron_core::EventName;
use kitron_protocol::KitronError;

use crate::core;

pub(crate) struct MenuState {
    /// Command id (the WM_COMMAND LOWORD) → the JS `action` string.
    actions: HashMap<u32, String>,
    /// Accels attached to the current menu, mirrored in `haccel`.
    accelerators: Vec<ACCEL>,
    /// The live accelerator table as a raw address (`HACCEL` is not `Send`).
    haccel: usize,
    /// The menu bar currently attached to windows (`HMENU` address, 0 = none).
    menubar: usize,
}

pub(crate) fn state() -> &'static Mutex<MenuState> {
    static STATE: OnceLock<Mutex<MenuState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(MenuState {
            actions: HashMap::new(),
            accelerators: Vec::new(),
            haccel: 0,
            menubar: 0,
        })
    })
}

/// `menu.setApplicationMenu` — the JSON is `MenuDefinition[]`, exactly what
/// the macOS twin parses. Main thread only (like every window surface).
pub fn set_application_menu(definition_json: &str) -> Result<(), KitronError> {
    let menus: Vec<MenuDefinition> = serde_json::from_str(definition_json)
        .map_err(|e| KitronError::code("MENU_INVALID", format!("invalid menu JSON: {e}")))?;

    let mut actions: HashMap<u32, String> = HashMap::new();
    let mut accelerators: Vec<ACCEL> = Vec::new();
    let mut next_command = 1u32; // 0 would match WM_COMMAND frames with no id

    // SAFETY: main-thread menu construction per the bridge thread gate.
    let menubar: HMENU = unsafe {
        CreateMenu().map_err(|e| KitronError::code("MENU_CREATE_FAILED", e.to_string()))?
    };
    for menu in &menus {
        let submenu = unsafe {
            CreatePopupMenu().map_err(|e| KitronError::code("MENU_CREATE_FAILED", e.to_string()))?
        };
        build_items(&submenu, &menu.items, &mut actions, &mut accelerators, &mut next_command)?;
        // SAFETY: appending an owned submenu to the owned bar.
        unsafe {
            AppendMenuW(
                menubar,
                MF_POPUP,
                submenu.0 as usize,
                PCWSTR(HSTRING::from(menu.label.as_str()).as_ptr()),
            )
            .map_err(|e| KitronError::code("MENU_CREATE_FAILED", e.to_string()))?;
        }
    }

    // Attach to every live window, then hand the bar over to the state.
    let targets: Vec<HWND> = crate::window::registry()
        .lock()
        .unwrap()
        .values()
        .map(|entry| entry.hwnd)
        .collect();
    // SAFETY: main-thread window surface updates.
    unsafe {
        for hwnd in targets {
            if SetMenu(hwnd, Some(menubar)).is_ok() {
                let _ = DrawMenuBar(hwnd);
            }
        }
        let table: usize = if accelerators.is_empty() {
            0
        } else {
            CreateAcceleratorTableW(&accelerators)
                .map_err(|e| KitronError::code("MENU_CREATE_FAILED", e.to_string()))?
                .0 as usize
        };
        let old = {
            let mut state = state().lock().unwrap();
            state.actions = actions;
            state.accelerators = accelerators;
            state.haccel = table;
            std::mem::replace(&mut state.menubar, menubar.0 as usize)
        };
        if old != 0 {
            let _ = DestroyMenu(HMENU(old as *mut _));
        }
    }

    Ok(())
}

/// Appends items (recursively) to `menu`, allocating command ids for entries
/// that carry an `action`, and collecting accelerators.
fn build_items(
    menu: &HMENU,
    items: &[MenuItemDefinition],
    actions: &mut HashMap<u32, String>,
    accelerators: &mut Vec<ACCEL>,
    next_command: &mut u32,
) -> Result<(), KitronError> {
    for item in items {
        if item.kind.as_deref() == Some("separator") {
            // SAFETY: plain menu mutation on the owning main thread.
            unsafe {
                AppendMenuW(*menu, MF_SEPARATOR, 0, PCWSTR::null())
                    .map_err(|e| KitronError::code("MENU_CREATE_FAILED", e.to_string()))?;
            }
            continue;
        }
        let label = item.label.as_deref().unwrap_or("");

        if let Some(nested) = item.items.as_ref() {
            let submenu = unsafe {
                CreatePopupMenu().map_err(|e| KitronError::code("MENU_CREATE_FAILED", e.to_string()))?
            };
            build_items(&submenu, nested, actions, accelerators, next_command)?;
            // SAFETY: see above.
            unsafe {
                AppendMenuW(
                    *menu,
                    MF_POPUP,
                    submenu.0 as usize,
                    PCWSTR(HSTRING::from(label).as_ptr()),
                )
                .map_err(|e| KitronError::code("MENU_CREATE_FAILED", e.to_string()))?;
            }
            continue;
        }

        let Some(action) = item.action.as_deref() else {
            // Action-less items cannot be dispatched — skip like the macOS
            // twin skips items without selectors.
            continue;
        };
        let command = *next_command;
        *next_command += 1;
        actions.insert(command, action.to_string());
        if let Some(spec) = item.accelerator.as_deref() {
            if let Some(accel) = parse_accelerator(spec, command) {
                accelerators.push(accel);
            }
        }

        let mut flags = MF_STRING;
        if item.enabled == Some(false) {
            flags |= MF_DISABLED;
        }
        if item.checked == Some(true) {
            flags |= MF_CHECKED;
        }
        // SAFETY: see above.
        unsafe {
            AppendMenuW(
                *menu,
                flags,
                command as usize,
                PCWSTR(HSTRING::from(label).as_ptr()),
            )
            .map_err(|e| KitronError::code("MENU_CREATE_FAILED", e.to_string()))?;
        }
    }
    Ok(())
}

/// "CmdOrCtrl+N" — FVIRTKEY entries for the accelerator string is portable
/// across shells.
///
/// The Win32 constants for the modifier flags live in `WindowsAndMessaging`
/// next to `ACCEL` in this crate version.
fn parse_accelerator(spec: &str, command: u32) -> Option<ACCEL> {
    use windows::Win32::UI::WindowsAndMessaging::{FALT, FCONTROL, FSHIFT, FVIRTKEY};

    let mut mods = FVIRTKEY;
    let mut key_part: Option<String> = None;
    for part in spec.split('+') {
        let lowered = part.to_ascii_lowercase();
        match lowered.as_str() {
            "cmdorctrl" | "cmd" | "ctrl" | "control" => mods = mods | FCONTROL,
            "alt" | "option" => mods = mods | FALT,
            "shift" => mods = mods | FSHIFT,
            other => key_part = Some(other.to_ascii_uppercase()),
        }
    }
    let key_part = key_part?;

    let key: u16 = match key_part.as_str() {
        "ENTER" | "RETURN" => 0x0D,
        "ESC" | "ESCAPE" => 0x1B,
        "TAB" => 0x09,
        "SPACE" => 0x20,
        "BACKSPACE" => 0x08,
        "DELETE" | "DEL" => 0x2E,
        "INSERT" | "INS" => 0x2D,
        "HOME" => 0x24,
        "END" => 0x23,
        "PAGEUP" => 0x21,
        "PAGEDOWN" => 0x22,
        "LEFT" => 0x25,
        "UP" => 0x26,
        "RIGHT" => 0x27,
        "DOWN" => 0x28,
        single if single.chars().count() == 1 => {
            let ch = single.chars().next().unwrap().to_ascii_uppercase();
            if ch.is_ascii_alphanumeric() {
                ch as u16
            } else {
                return None; // punctuation accelerators arrive later
            }
        }
        f if f.len() >= 2 && f.starts_with('F') => {
            let index: u8 = f[1..].parse().ok()?;
            if (1..=24).contains(&index) {
                0x6F + index as u16 // VK_F1 = 0x70
            } else {
                return None;
            }
        }
        _ => return None,
    };

    Some(ACCEL { fVirt: mods, key, cmd: command as u16 })
}

/// `WM_COMMAND` dispatch: the LOWORD of wParam carries the menu command id
/// (win proc calls this from its WM_COMMAND arm).
pub(crate) fn on_command(wparam: WPARAM) {
    let command = (wparam.0 & 0xFFFF) as u32;
    let action = state().lock().unwrap().actions.get(&command).cloned();
    if let Some(action) = action {
        core().emit(
            EventName::MenuAction,
            serde_json::json!({ "action": action }),
        );
    }
}

/// The current accelerator table for the template's message loop
/// (`TranslateAcceleratorW(hwnd, table, &msg)`), or 0 when unused.
///
/// # Safety
///
/// The returned pointer is a borrowed HACCEL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kitron_windows_accelerators() -> usize {
    state().lock().unwrap().haccel
}
