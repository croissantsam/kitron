//! Portable bindings: app bits, backend supervision control, and the events
//! the bridge drains. The supervisor and protocol are shared with the macOS
//! shell — these helpers only adapt the call shapes.

use std::ffi::{c_char, CStr};

use kitron_core::EventName;
use kitron_protocol::KitronError;

use crate::core;

/// The fields of `app.bits()` — exactly what the macOS twin answers
/// (`kitron_app_bits_json`); the JSI surface reads them directly.
pub fn app_bits() -> serde_json::Value {
    let core = core();
    serde_json::json!({
        "pid": std::process::id(),
        "name": core.config.app.name,
        "identifier": core.config.app.identifier,
        "version": core.config.app.version,
        "platform": kitron_protocol::platform_name(),
        "dev": core.dev_mode,
        "rootDir": core.root_dir,
    })
}

/// Backend lifecycle control (JSI/`kitron_node`): start | stop.
pub fn node_control(mode: &str) -> Result<serde_json::Value, KitronError> {
    let core = core();
    match mode {
        "start" => {
            let supervisor = core.start_node_from_config()?;
            core.logger.info("node", "backend starting");
            Ok(serde_json::json!({ "pid": supervisor.status().pid }))
        }
        "stop" => {
            core.stop_node("requested");
            Ok(serde_json::json!({}))
        }
        other => Err(KitronError::code(
            "NODE_MODE_UNKNOWN",
            format!("unknown node mode {other}"),
        )),
    }
}

/// Blocking invoke into the backend (bridge worker threads only).
///
/// # Safety
///
/// String arguments must be null or valid NUL-terminated C strings.
pub unsafe fn node_invoke(
    method: *const c_char,
    payload: *const c_char,
    timeout_ms: u32,
) -> Result<serde_json::Value, KitronError> {
    let method_name = if method.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(method) }.to_string_lossy().into_owned()
    };
    let payload = if payload.is_null() {
        None
    } else {
        let raw = unsafe { CStr::from_ptr(payload) }.to_string_lossy().into_owned();
        let parsed = serde_json::from_str::<serde_json::Value>(raw.as_ref());
        match parsed {
            Ok(value) => Some(value),
            Err(err) => {
                return Err(KitronError::code(
                    "NODE_PAYLOAD_INVALID",
                    format!("invalid payload JSON: {err}"),
                ))
            }
        }
    };

    kitron_core::node::invoke_supervisor(
        core(),
        &method_name,
        payload,
        std::time::Duration::from_millis(timeout_ms as u64),
    )
}

/// The events the bridge pump drains — envelopes keep the macOS shape.
pub fn drain_events() -> Vec<serde_json::Value> {
    core()
        .events()
        .drain()
        .into_iter()
        .map(|event| {
            serde_json::json!({
                "name": event.name,
                "windowId": event.window_id,
                "payload": event.payload,
            })
        })
        .collect()
}

/// `app.ready` marker — like macOS, the first window pushes it (spec §11).
pub fn push_app_ready_on_first_window(id: u64) {
    if id == 1 {
        core().emit(EventName::AppReady, serde_json::json!({}));
    }
}

// ---------------------------------------------------------------------------
// Clipboard (spec §13) — CF_UNICODETEXT only, permission-gated
// ---------------------------------------------------------------------------

/// Reads the clipboard as text (empty string when it holds no text).
pub fn clipboard_read() -> Result<String, KitronError> {
    core().require_permission("clipboard")?;
    use windows::Win32::Foundation::HGLOBAL;
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, OpenClipboard,
    };
    use windows::Win32::System::Memory::GlobalUnlock;
    use windows::Win32::System::Ole::CF_UNICODETEXT;
    use windows::Win32::System::Memory::GlobalLock;

    // SAFETY: clipboard round-trip, all handles released; text copied out.
    unsafe {
        OpenClipboard(None).map_err(|e| KitronError::code("CLIPBOARD_OPEN_FAILED", e.to_string()))?;
        let result = (|| -> Result<String, KitronError> {
            let handle = GetClipboardData(CF_UNICODETEXT.0 as u32)
                .map_err(|_| KitronError::code("CLIPBOARD_EMPTY", "no text on the clipboard"))?;
            let pointer = GlobalLock(HGLOBAL(handle.0));
            if pointer.is_null() {
                let _ = GlobalUnlock(HGLOBAL(handle.0));
                return Err(KitronError::code(
                    "CLIPBOARD_LOCK_FAILED",
                    "GlobalLock returned null",
                ));
            }
            let text = {
                let wide = pointer as *const u16;
                let mut len = 0usize;
                while *wide.add(len) != 0 {
                    len += 1;
                }
                let bytes = std::slice::from_raw_parts(wide, len);
                String::from_utf16_lossy(bytes)
            };
            let _ = GlobalUnlock(HGLOBAL(handle.0));
            Ok(text)
        })();
        let _ = CloseClipboard();
        result
    }
}

/// Writes text to the clipboard, replacing its current content.
pub fn clipboard_write(text: &str) -> Result<(), KitronError> {
    core().require_permission("clipboard")?;
    use windows::Win32::Foundation::{HANDLE, HGLOBAL};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
    };
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows::Win32::System::Ole::CF_UNICODETEXT;

    // SAFETY: allocation + clipboard ownership transfer per the Win32 rules:
    // after SetClipboardData the system owns the block (never freed here).
    unsafe {
        OpenClipboard(None).map_err(|e| KitronError::code("CLIPBOARD_OPEN_FAILED", e.to_string()))?;
        let result = (|| -> Result<(), KitronError> {
            EmptyClipboard()
                .map_err(|e| KitronError::code("CLIPBOARD_EMPTY_FAILED", e.to_string()))?;
            let mut wide: Vec<u16> = text.encode_utf16().collect();
            wide.push(0);
            let handle: HGLOBAL =
                GlobalAlloc(GMEM_MOVEABLE, wide.len() * std::mem::size_of::<u16>())
                    .map_err(|e| KitronError::code("CLIPBOARD_ALLOC_FAILED", e.to_string()))?;
            let pointer = GlobalLock(handle);
            if pointer.is_null() {
                let _ = GlobalUnlock(handle);
                return Err(KitronError::code(
                    "CLIPBOARD_LOCK_FAILED",
                    "GlobalLock returned null",
                ));
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), pointer as *mut u16, wide.len());
            let _ = GlobalUnlock(handle);
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(handle.0)))
                .map_err(|e| KitronError::code("CLIPBOARD_WRITE_FAILED", e.to_string()))?;
            Ok(())
        })();
        let _ = CloseClipboard();
        result
    }
}

// ---------------------------------------------------------------------------
// Shell (spec §15) — default handlers, permission-gated
// ---------------------------------------------------------------------------

/// Opens a URL or path with the default Windows handler.
pub fn shell_open(target: &str) -> Result<(), KitronError> {
    core().require_permission("shell")?;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let verb = HSTRING::from("open");
    let file = HSTRING::from(target);
    // SAFETY: main-thread call per the bridge gate; arguments owned. The
    // return is an HINSTANCE: pointer values ≤ 32 signal failure.
    let instance = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if instance.0 as isize <= 32 {
        return Err(KitronError::code(
            "SHELL_OPEN_FAILED",
            format!("ShellExecuteW refused {target}"),
        ));
    }
    Ok(())
}

/// Reveals a file in Explorer (Finder's `showItemInFolder` twin).
pub fn shell_show_item_in_folder(path: &str) -> Result<(), KitronError> {
    core().require_permission("shell")?;
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};
    use windows::Win32::UI::Shell::Common::ITEMIDLIST;
    let wide = HSTRING::from(path);
    // SAFETY: pidl created, consumed and freed in this scope.
    unsafe {
        let pidl = ILCreateFromPathW(PCWSTR(wide.as_ptr()));
        if pidl.is_null() {
            return Err(KitronError::code(
                "SHELL_PATH_INVALID",
                format!("ILCreateFromPathW refused {path}"),
            ));
        }
        let pidl = pidl as *const ITEMIDLIST;
        let result = SHOpenFolderAndSelectItems(pidl, None, 0)
            .map_err(|e| KitronError::code("SHELL_OPEN_FAILED", e.to_string()));
        ILFree(Some(pidl));
        result
    }
}
