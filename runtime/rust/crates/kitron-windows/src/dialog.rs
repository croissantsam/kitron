//! Native dialogs on Windows — `MessageBoxW` and the Common Item Dialog
//! (`IFileOpenDialog` / `IFileSaveDialog`), spec §12 on Windows.
//!
//! Presentation parity with the macOS twin: without `windowId` the message
//! box is app-modal; with one it is owned by that window (the Windows
//! equivalent of a sheet — parented, app-local). File panels are always
//! owned when `windowId` resolves. Results are built by the portable
//! `shell::dialog_results` helpers so the JSON keys never drift.

use kitron_core::shell::{
    dialog_results, DialogMessageArgs, DialogOpenFileArgs, DialogSaveFileArgs,
};
use kitron_protocol::KitronError;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, FOS_ALLOWMULTISELECT, FOS_FORCEFILESYSTEM, IFileOpenDialog,
    IFileSaveDialog, SIGDN_FILESYSPATH,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MESSAGEBOX_STYLE, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK,
    MB_OKCANCEL, MB_YESNOCANCEL,
};

use crate::core;

/// COM is apartment-initialised by the RNW host; guard just in case the call
/// arrives from a fresh thread.
fn ensure_com() -> Result<(), KitronError> {
    // SAFETY: plain COM init; S_FALSE (already initialised) maps to an error
    // value in some binders — both are success for our purposes.
    let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    Ok(())
}

/// The owner for an owned dialog (`windowId` contract), or `None` for the
/// app-modal presentation. An explicit id that does not resolve is an error.
fn owner_window(window_id: Option<u64>) -> Result<Option<HWND>, KitronError> {
    let Some(id) = window_id else {
        return Ok(None);
    };
    let guard = crate::window::registry().lock().unwrap();
    let entry = guard.get(&id).ok_or_else(|| {
        KitronError::code(
            "WINDOW_NOT_FOUND",
            format!("no window {id} to attach the dialog to"),
        )
    })?;
    Ok(Some(entry.hwnd))
}

fn args_parse(error: serde_json::Error) -> KitronError {
    KitronError::code("DIALOG_ARGS_INVALID", error.to_string())
}

/// The first button wins (`confirmed`), like AppKit's `NSAlertFirstButtonReturn`
/// arithmetic in `dialog_results::message(labels, 1000 + index)`.
fn button_style(count: usize) -> Result<MESSAGEBOX_STYLE, KitronError> {
    Ok(match count {
        1 => MB_OK,
        2 => MB_OKCANCEL,
        3 => MB_YESNOCANCEL,
        _ => {
            return Err(KitronError::code(
                "DIALOG_BUTTONS_UNSUPPORTED",
                format!(
                    "Windows message boxes support up to 3 buttons; \
                     got {count}. Trim `buttons` or use a menu action."
                ),
            ))
        }
    })
}

fn icon_style(name: Option<&str>) -> MESSAGEBOX_STYLE {
    match name {
        Some("warning") => MB_ICONWARNING,
        Some("critical") | Some("error") => MB_ICONERROR,
        _ => MB_ICONINFORMATION,
    }
}

/// The clicked-button index → the contract mapping, per the button set
/// MessageBox picked for the label count.
fn clicked_index(response: i32, count: usize) -> usize {
    use windows::Win32::UI::WindowsAndMessaging::{IDCANCEL, IDNO, IDOK, IDYES};
    // OK and YES lead (index 0); NO is second in YESNOCANCEL; CANCEL sits
    // last in both the two- and three-button sets.
    if response == IDOK.0 {
        return 0;
    }
    if count == 3 && response == IDYES.0 {
        return 0;
    }
    if count == 3 && response == IDNO.0 {
        return 1;
    }
    if response == IDCANCEL.0 {
        return count - 1;
    }
    0
}

/// `dialog.message` — app-modal by default, owned by `windowId` when given.
pub fn message(args_json: &str) -> Result<serde_json::Value, KitronError> {
    core().require_permission("dialog")?;
    let args: DialogMessageArgs = serde_json::from_str(args_json).map_err(args_parse)?;
    let labels = args.buttons.clone().unwrap_or_else(|| vec!["OK".into()]);
    let flags = button_style(labels.len())? | icon_style(args.style.as_deref());

    let owner = owner_window(args.window_id)?;
    let text = HSTRING::from(args.message.as_deref().unwrap_or(""));
    let title = HSTRING::from(args.title.as_deref().unwrap_or("Kitron"));

    // SAFETY: main-thread dialog (bridge gates the surface); strings owned.
    let clicked = unsafe {
        MessageBoxW(
            owner,
            PCWSTR(text.as_ptr()),
            PCWSTR(title.as_ptr()),
            flags,
        )
    };
    let index = clicked_index(clicked.0, labels.len());
    Ok(dialog_results::message(&labels, 1000 + index as isize))
}

/// `dialog.openFile` — Common Item Dialog, multi-select per the args.
pub fn open_file(args_json: &str) -> Result<serde_json::Value, KitronError> {
    core().require_permission("dialog")?;
    ensure_com()?;
    let args: DialogOpenFileArgs = serde_json::from_str(args_json).map_err(args_parse)?;
    let owner = owner_window(args.window_id)?;

    // SAFETY: COM dialog on the UI thread; all strings outlive the call.
    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_ALL).map_err(com_error)?;
        let mut options = dialog.GetOptions().map_err(com_error)?;
        options |= FOS_FORCEFILESYSTEM;
        if args.multiple == Some(true) {
            options |= FOS_ALLOWMULTISELECT;
        }
        dialog.SetOptions(options).map_err(com_error)?;
        if let Some(title) = args.title.as_deref() {
            dialog.SetTitle(&HSTRING::from(title)).map_err(com_error)?;
        }
        dialog_set_file_types(&dialog, &args.extensions)?;

        match dialog.Show(owner) {
            Ok(()) => {}
            Err(error) if is_cancel(&error) => return Ok(dialog_results::canceled_open()),
            Err(error) => return Err(com_error(error)),
        }

        let items = dialog.GetResults().map_err(com_error)?;
        let count = items.GetCount().map_err(com_error)?;
        let mut paths = Vec::new();
        for index in 0..count {
            let item = items.GetItemAt(index).map_err(com_error)?;
            let path = item
                .GetDisplayName(SIGDN_FILESYSPATH)
                .map_err(com_error)?
                .to_string()
                .map_err(|e| KitronError::code("DIALOG_PATH_INVALID", e.to_string()))?;
            paths.push(path);
        }
        Ok(dialog_results::chose_open(paths))
    }
}

/// `dialog.saveFile` — Common Item Dialog with the pre-filled file name.
pub fn save_file(args_json: &str) -> Result<serde_json::Value, KitronError> {
    core().require_permission("dialog")?;
    ensure_com()?;
    let args: DialogSaveFileArgs = serde_json::from_str(args_json).map_err(args_parse)?;
    let owner = owner_window(args.window_id)?;

    // SAFETY: same COM rules as `open_file`.
    unsafe {
        let dialog: IFileSaveDialog =
            CoCreateInstance(&FileSaveDialog, None, CLSCTX_ALL).map_err(com_error)?;
        dialog
            .SetOptions(FOS_FORCEFILESYSTEM)
            .map_err(com_error)?;
        if let Some(name) = args.default_name.as_deref() {
            dialog.SetFileName(&HSTRING::from(name)).map_err(com_error)?;
        }
        if let Some(title) = args.title.as_deref() {
            dialog.SetTitle(&HSTRING::from(title)).map_err(com_error)?;
        }
        match dialog.Show(owner) {
            Ok(()) => {}
            Err(error) if is_cancel(&error) => return Ok(dialog_results::canceled_save()),
            Err(error) => return Err(com_error(error)),
        }
        let item = dialog.GetResult().map_err(com_error)?;
        let path = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(com_error)?
            .to_string()
            .map_err(|e| KitronError::code("DIALOG_PATH_INVALID", e.to_string()))?;
        Ok(dialog_results::chose_save(Some(path)))
    }
}

fn is_cancel(error: &windows::core::Error) -> bool {
    // HRESULT_FROM_WIN32(ERROR_CANCELLED) — the Common Item Dialog's cancel.
    error.code().0 == 0x8007_04C7_u32 as i32
}

fn com_error(error: windows::core::Error) -> KitronError {
    KitronError::code("DIALOG_OPEN_FAILED", error.to_string())
}

/// Filters from the `extensions` contract: `["md", "txt"]` → one entry
/// `Kitron files | *.md;*.txt` (the macOS twin passes UTTypes to the panel).
fn dialog_set_file_types(
    dialog: &IFileOpenDialog,
    extensions: &Option<Vec<String>>,
) -> Result<(), KitronError> {
    let Some(list) = extensions else {
        return Ok(());
    };
    if list.is_empty() {
        return Ok(());
    }
    let pattern: String = list
        .iter()
        .map(|ext| format!("*.{ext}"))
        .collect::<Vec<_>>()
        .join(";");
    let name = HSTRING::from("Kitron files");
    let spec = HSTRING::from(pattern);
    let specs = [COMDLG_FILTERSPEC {
        pszName: PCWSTR(name.as_ptr()),
        pszSpec: PCWSTR(spec.as_ptr()),
    }];
    // SAFETY: the filter array outlives the call; strings are HSTRING-backed.
    unsafe { dialog.SetFileTypes(&specs).map_err(com_error) }
}
