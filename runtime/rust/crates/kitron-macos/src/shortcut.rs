//! Global shortcuts through Carbon `RegisterEventHotKey` (spec §42).
//!
//! Gated by the `shortcut` permission (spec §23). Hot keys are global: they
//! fire even when the app is in the background.

use kitron_protocol::KitronError;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use crate::core;

type OSStatus = i32;

#[repr(C)]
#[derive(Copy, Clone)]
struct EventHotKeyID {
    signature: u32,
    id: u32,
}

type EventHotKeyRef = *mut std::ffi::c_void;
type EventTargetRef = *mut std::ffi::c_void;
type EventHandlerRef = *mut std::ffi::c_void;
type EventRef = *mut std::ffi::c_void;
type EventHandlerCallRef = *mut std::ffi::c_void;

#[repr(C)]
struct EventTypeSpec {
    event_class: u32,
    event_kind: u32,
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    fn GetApplicationEventTarget() -> EventTargetRef;
    fn RegisterEventHotKey(
        inHotKeyCode: u32,
        inHotKeyModifiers: u32,
        inHotKeyID: EventHotKeyID,
        inTarget: EventTargetRef,
        inOptions: u32,
        outRef: *mut EventHotKeyRef,
    ) -> OSStatus;
    fn UnregisterEventHotKey(inHotKeyRef: EventHotKeyRef) -> OSStatus;
    fn InstallEventHandler(
        inTarget: EventTargetRef,
        inHandler: extern "C" fn(EventHandlerCallRef, EventRef, *mut std::ffi::c_void) -> OSStatus,
        inNumTypes: usize,
        inList: *const EventTypeSpec,
        inUserData: *mut std::ffi::c_void,
        outRef: *mut EventHandlerRef,
    ) -> OSStatus;
    fn GetEventParameter(
        inEvent: EventRef,
        inName: u32,
        inDesiredType: u32,
        outActualType: *mut u32,
        inBufferSize: usize,
        outActualSize: *mut usize,
        outData: *mut std::ffi::c_void,
    ) -> OSStatus;
}

const K_EVENT_CLASS_KEYBOARD: u32 = 0x6B657962; // 'keyb'
const K_EVENT_HOT_KEY_PRESSED: u32 = 5;
const K_EVENT_PARAM_DIRECT_OBJECT: u32 = 0x2D2D2D2D; // '----'
const TYPE_EVENT_HOT_KEY_ID: u32 = 0x686B6964; // 'hkid'

// Carbon modifier masks.
const CMD_KEY: u32 = 1 << 8;
const SHIFT_KEY: u32 = 1 << 9;
const OPTION_KEY: u32 = 1 << 11;
const CONTROL_KEY: u32 = 1 << 12;

static HANDLER_INSTALLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
static REGISTRY: OnceLock<Mutex<BTreeMap<u32, String>>> = OnceLock::new();
static REFS: OnceLock<Mutex<BTreeMap<u32, usize>>> = OnceLock::new();

fn registry() -> &'static Mutex<BTreeMap<u32, String>> {
    REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}
fn refs() -> &'static Mutex<BTreeMap<u32, usize>> {
    REFS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

extern "C" fn handler(
    _call: EventHandlerCallRef,
    event: EventRef,
    _user: *mut std::ffi::c_void,
) -> OSStatus {
    let mut hot_key_id = EventHotKeyID {
        signature: 0,
        id: 0,
    };
    unsafe {
        GetEventParameter(
            event,
            K_EVENT_PARAM_DIRECT_OBJECT,
            TYPE_EVENT_HOT_KEY_ID,
            std::ptr::null_mut(),
            std::mem::size_of::<EventHotKeyID>(),
            std::ptr::null_mut(),
            &mut hot_key_id as *mut _ as *mut std::ffi::c_void,
        );
    }
    if let Ok(reg) = registry().lock() {
        if let Some(acc) = reg.get(&hot_key_id.id) {
            crate::core().emit(
                kitron_core::EventName::ShortcutTriggered,
                serde_json::json!({ "accelerator": acc }),
            );
        }
    }
    0
}

fn install_handler() {
    if HANDLER_INSTALLED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let spec = EventTypeSpec {
        event_class: K_EVENT_CLASS_KEYBOARD,
        event_kind: K_EVENT_HOT_KEY_PRESSED,
    };
    unsafe {
        InstallEventHandler(
            GetApplicationEventTarget(),
            handler,
            1,
            &spec,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
    }
}

fn key_code_for(name: &str) -> Option<u32> {
    // macOS virtual key codes.
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "a" => Some(0),
        "s" => Some(1),
        "d" => Some(2),
        "f" => Some(3),
        "h" => Some(4),
        "g" => Some(5),
        "z" => Some(6),
        "x" => Some(7),
        "c" => Some(8),
        "v" => Some(9),
        "b" => Some(11),
        "q" => Some(12),
        "w" => Some(13),
        "e" => Some(14),
        "r" => Some(15),
        "y" => Some(16),
        "t" => Some(17),
        "1" => Some(18),
        "2" => Some(19),
        "3" => Some(20),
        "4" => Some(21),
        "6" => Some(22),
        "5" => Some(23),
        "=" => Some(24),
        "9" => Some(25),
        "7" => Some(26),
        "-" => Some(27),
        "8" => Some(28),
        "0" => Some(29),
        "]" => Some(30),
        "o" => Some(31),
        "u" => Some(32),
        "[" => Some(33),
        "i" => Some(34),
        "p" => Some(35),
        "l" => Some(37),
        "j" => Some(38),
        "k" => Some(39),
        ";" => Some(41),
        "n" => Some(45),
        "m" => Some(46),
        "," => Some(43),
        "." => Some(47),
        "/" => Some(44),
        "space" => Some(49),
        "return" | "enter" => Some(36),
        "tab" => Some(48),
        "escape" | "esc" => Some(53),
        "delete" | "backspace" => Some(51),
        "up" => Some(126),
        "down" => Some(125),
        "left" => Some(123),
        "right" => Some(124),
        "f1" => Some(122),
        "f2" => Some(120),
        "f3" => Some(99),
        "f4" => Some(118),
        "f5" => Some(96),
        "f6" => Some(97),
        "f7" => Some(98),
        "f8" => Some(100),
        "f9" => Some(101),
        "f10" => Some(109),
        "f11" => Some(103),
        "f12" => Some(111),
        _ => None,
    }
}

pub(crate) fn register(accelerator: &str) -> Result<(), KitronError> {
    core().require_permission("shortcut")?;
    install_handler();
    // Parse modifiers + key.
    let mut modifiers: u32 = 0;
    let mut key_name = String::new();
    for part in accelerator.split('+') {
        match part.trim().to_ascii_lowercase().as_str() {
            "cmd" | "command" | "mod" | "cmdorctrl" => modifiers |= CMD_KEY,
            "ctrl" | "control" => modifiers |= CONTROL_KEY,
            "alt" | "option" => modifiers |= OPTION_KEY,
            "shift" => modifiers |= SHIFT_KEY,
            "" => {}
            other => key_name = other.to_string(),
        }
    }
    let key_code = key_code_for(&key_name).ok_or_else(|| {
        KitronError::code("SHORTCUT_INVALID_KEY", format!("unknown key: {key_name}"))
    })?;

    let id = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let hot_key_id = EventHotKeyID {
        signature: 0x6B74726E, // 'ktrn'
        id,
    };
    let mut out_ref: EventHotKeyRef = std::ptr::null_mut();
    let status = unsafe {
        RegisterEventHotKey(
            key_code,
            modifiers,
            hot_key_id,
            GetApplicationEventTarget(),
            0,
            &mut out_ref,
        )
    };
    if status != 0 {
        return Err(KitronError::code(
            "SHORTCUT_REGISTER_FAILED",
            format!("RegisterEventHotKey failed: {status}"),
        ));
    }
    registry().lock().unwrap().insert(id, accelerator.to_string());
    refs().lock().unwrap().insert(id, out_ref as usize);
    Ok(())
}

pub(crate) fn unregister(accelerator: &str) -> Result<(), KitronError> {
    core().require_permission("shortcut")?;
    // Find id by accelerator.
    let mut target_id: Option<u32> = None;
    if let Ok(reg) = registry().lock() {
        for (id, acc) in reg.iter() {
            if acc == accelerator {
                target_id = Some(*id);
                break;
            }
        }
    }
    let Some(id) = target_id else {
        return Ok(());
    };
    if let Some(r) = refs().lock().unwrap().remove(&id) {
        unsafe { UnregisterEventHotKey(r as EventHotKeyRef) };
    }
    registry().lock().unwrap().remove(&id);
    Ok(())
}