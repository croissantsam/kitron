//! macOS implementations (AppKit / Foundation / WebKit / UserNotifications).
//!
//! Native dialogs, notifications and screenshots go through `osascript` +
//! `screencapture`/`sips` (always present on macOS); clipboard images via
//! AppleScriptObjC pasteboard access. No Chromium, no extra dependency.

use anyhow::Result;

use super::{run_cmd, temp_png, DialogMessageArgs, DialogOpenArgs, DialogSaveArgs};

pub fn platform_name() -> &'static str {
    "macos (WKWebView)"
}

/// WKWebView is provided by the OS WebKit — Kitron never bundles Chromium.
pub fn webview_backend() -> &'static str {
    "WKWebView"
}

pub fn notification_show(
    title: &str,
    body: &str,
    _silent: bool,
    sound: Option<&str>,
) -> Result<()> {
    let mut script = format!(
        "display notification {} with title {}",
        apple_quote(body),
        apple_quote(title)
    );
    if let Some(s) = sound {
        if !s.is_empty() {
            script.push_str(&format!(" sound name {}", apple_quote(s)));
        }
    }
    run_cmd("osascript", &["-e", &script])?;
    Ok(())
}

fn apple_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn choose_filters(filters: &[(String, Vec<String>)]) -> String {
    let mut exts: Vec<String> = Vec::new();
    for (_, e) in filters {
        exts.extend(e.iter().cloned());
    }
    if exts.is_empty() {
        return String::new();
    }
    let list = exts
        .iter()
        .map(|e| format!("\"{e}\""))
        .collect::<Vec<_>>()
        .join(", ");
    format!(" of type {{{list}}}")
}

/// Returns (canceled, paths). A dismissed panel surfaces as osascript error.
pub fn dialog_open(args: &DialogOpenArgs) -> Result<(bool, Vec<String>)> {
    let script = open_script(args);
    // Single (non-list) results fail the repeat — retry unwrapped.
    match run_cmd("osascript", &["-e", &script]) {
        Ok(out) => Ok((
            false,
            out.lines().map(|l| l.to_string()).filter(|l| !l.is_empty()).collect(),
        )),
        Err(_) => {
            let single = script.replace(
                "\nset _out to \"\"\nrepeat with _f in _sel\nset _out to _out & (POSIX path of _f) & \"\\n\"\nend repeat\n_out",
                "\nPOSIX path of _sel",
            );
            match run_cmd("osascript", &["-e", &single]) {
                Ok(out) => {
                    let p = out.trim().to_string();
                    Ok((p.is_empty(), if p.is_empty() { vec![] } else { vec![p] }))
                }
                Err(e) if is_cancel(&e) => Ok((true, vec![])),
                Err(e) => {
                    if is_cancel(&e) {
                        Ok((true, vec![]))
                    } else {
                        Err(e)
                    }
                }
            }
        }
    }
}

fn open_script(args: &DialogOpenArgs) -> String {
    let prompt = args.title.as_deref().unwrap_or("Choose");
    let mut script = if args.directory {
        format!(
            "set _sel to choose folder with prompt {}",
            apple_quote(prompt)
        )
    } else {
        format!("set _sel to choose file with prompt {}", apple_quote(prompt))
    };
    if let Some(p) = args.default_path.as_deref() {
        script.push_str(&format!(" default location {}", apple_quote(p)));
    }
    script.push_str(&choose_filters(&args.filters));
    if args.multiple && !args.directory {
        script.push_str(" with multiple selections allowed");
    }
    script.push_str(
        "\nset _out to \"\"\nrepeat with _f in _sel\nset _out to _out & (POSIX path of _f) & \"\\n\"\nend repeat\n_out",
    );
    script
}

fn is_cancel(e: &anyhow::Error) -> bool {
    let m = e.to_string().to_lowercase();
    m.contains("cancel") || m.contains("(-128)")
}

pub fn dialog_save(args: &DialogSaveArgs) -> Result<(bool, Vec<String>)> {
    match run_cmd("osascript", &["-e", &save_script(args)]) {
        Ok(out) => {
            let p = out.trim().to_string();
            Ok((p.is_empty(), if p.is_empty() { vec![] } else { vec![p] }))
        }
        Err(e) if is_cancel(&e) => Ok((true, vec![])),
        Err(e) => Err(e),
    }
}

fn save_script(args: &DialogSaveArgs) -> String {
    let prompt = args.title.as_deref().unwrap_or("Save");
    let mut script = format!(
        "set _f to choose file name with prompt {}",
        apple_quote(prompt)
    );
    if let Some(p) = args.default_path.as_deref() {
        script.push_str(&format!(" default location {}", apple_quote(p)));
    }
    if let Some(n) = args.default_name.as_deref() {
        script.push_str(&format!(" default name {}", apple_quote(n)));
    }
    script.push_str("\nPOSIX path of _f");
    script
}

pub fn dialog_message(args: &DialogMessageArgs) -> Result<(bool, usize)> {
    let buttons = if args.buttons.is_empty() {
        vec!["OK".to_string()]
    } else {
        args.buttons.clone()
    };
    match run_cmd("osascript", &["-e", &message_script(args, &buttons)]) {
        Ok(out) => {
            let label = out.trim();
            let idx = buttons.iter().position(|b| b == label).unwrap_or(0);
            Ok((false, idx))
        }
        Err(e) if is_cancel(&e) => Ok((true, 0)),
        Err(e) => Err(e),
    }
}

fn message_script(args: &DialogMessageArgs, buttons: &[String]) -> String {
    let list = buttons
        .iter()
        .map(|b| apple_quote(b))
        .collect::<Vec<_>>()
        .join(", ");
    let icon = match args.kind.as_str() {
        "error" => " with icon stop",
        "warning" => " with icon caution",
        _ => " with icon note",
    };
    let text = if let Some(d) = args.detail.as_deref() {
        format!("{}\n\n{}", args.message, d)
    } else {
        args.message.clone()
    };
    let title = args.title.as_deref().unwrap_or("Kitron");
    format!(
        "button returned of (display dialog {} with title {} buttons {{{list}}} default button 1{icon})",
        apple_quote(&text),
        apple_quote(title)
    )
}

/// PNG bytes from the clipboard, or None when no image is present.
pub fn clipboard_read_image() -> Result<Option<Vec<u8>>> {
    let probe = r#"use framework "AppKit"
use framework "Foundation"
set pb to current application's NSPasteboard's generalPasteboard()
set tys to (pb's types()) as list
if tys contains (current application's NSPasteboardTypePNG) then
  return "png"
else if tys contains (current application's NSPasteboardTypeTIFF) then
  return "tiff"
else
  return "none"
end if"#;
    let kind = run_cmd("osascript", &["-e", probe])?;
    if kind == "none" {
        return Ok(None);
    }
    let tmp = temp_png("clip");
    let tmp_str = tmp.to_string_lossy().to_string();
    if kind == "png" {
        let script = format!(
            "use framework \"AppKit\"\nuse framework \"Foundation\"\nset pb to current application's NSPasteboard's generalPasteboard()\nset d to pb's dataForType:(current application's NSPasteboardTypePNG)\n(d's writeToFile:{} atomically:true)",
            apple_quote(&tmp_str)
        );
        run_cmd("osascript", &["-e", &script])?;
        let bytes = std::fs::read(&tmp)?;
        let _ = std::fs::remove_file(&tmp);
        return Ok(Some(bytes));
    }
    // TIFF → convert with sips (builtin).
    let tiff = tmp.with_extension("tiff");
    let tiff_str = tiff.to_string_lossy().to_string();
    let script = format!(
        "use framework \"AppKit\"\nuse framework \"Foundation\"\nset pb to current application's NSPasteboard's generalPasteboard()\nset d to pb's dataForType:(current application's NSPasteboardTypeTIFF)\n(d's writeToFile:{} atomically:true)",
        apple_quote(&tiff_str)
    );
    run_cmd("osascript", &["-e", &script])?;
    run_cmd("sips", &["-s", "format", "png", &tiff_str, "--out", &tmp_str])?;
    let bytes = std::fs::read(&tmp)?;
    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(&tiff);
    Ok(Some(bytes))
}

pub fn clipboard_write_image(png: &[u8]) -> Result<()> {
    let tmp = temp_png("clipw");
    std::fs::write(&tmp, png)?;
    let tmp_str = tmp.to_string_lossy().to_string();
    let script = format!(
        "use framework \"AppKit\"\nuse framework \"Foundation\"\nset pb to current application's NSPasteboard's generalPasteboard()\n(pb's clearContents())\nset d to (current application's NSData's dataWithContentsOfFile:{})\n(pb's setData:d forType:(current application's NSPasteboardTypePNG))",
        apple_quote(&tmp_str)
    );
    let r = run_cmd("osascript", &["-e", &script]);
    let _ = std::fs::remove_file(&tmp);
    r?;
    Ok(())
}

/// Screenshot of a screen rect (physical px) via screencapture.
/// Requires the Screen Recording permission on the packaged app —
/// without it the OS refuses the capture (typed NATIVE_API_FAILED).
pub fn capture_window_rect(x: i32, y: i32, w: u32, h: u32) -> Result<Vec<u8>> {
    let tmp = temp_png("cap");
    let tmp_str = tmp.to_string_lossy().to_string();
    let mut argv = screencapture_args(x, y, w, h);
    argv.push(tmp_str.clone());
    let refs: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
    run_cmd("screencapture", &refs)?;
    let bytes = std::fs::read(&tmp)?;
    let _ = std::fs::remove_file(&tmp);
    Ok(bytes)
}

fn screencapture_args(x: i32, y: i32, w: u32, h: u32) -> Vec<String> {
    vec![
        "-R".to_string(),
        format!("{x},{y},{w},{h}"),
        "-x".to_string(),
        "-t".to_string(),
        "png".to_string(),
    ]
}

pub fn clipboard_read() -> Result<String> {
    run_cmd("pbpaste", &[])
}

pub fn clipboard_write(text: &str) -> Result<()> {
    use std::io::Write;
    let mut child = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("pbcopy failed")
    }
}

fn pasteboard_string(pasteboard_type: &str) -> Result<String> {
    let script = format!(
        "use framework \"AppKit\"\nuse framework \"Foundation\"\nset pb to current application's NSPasteboard's generalPasteboard()\nset s to (pb's stringForType:(current application's {}))\nif s is missing value then return \"\"\nreturn s as text",
        pasteboard_type
    );
    run_cmd("osascript", &["-e", &script])
}

fn pasteboard_set_string(pasteboard_type: &str, text: &str) -> Result<()> {
    let script = format!(
        "use framework \"AppKit\"\nuse framework \"Foundation\"\nset pb to current application's NSPasteboard's generalPasteboard()\n(pb's clearContents())\n(pb's setString:{} forType:(current application's {}))",
        apple_quote(text),
        pasteboard_type
    );
    run_cmd("osascript", &["-e", &script])?;
    Ok(())
}

pub fn clipboard_read_format(format: &str) -> Result<String> {
    match format {
        "html" => pasteboard_string("NSPasteboardTypeHTML"),
        "rtf" => pasteboard_string("NSPasteboardTypeRTF"),
        _ => clipboard_read(),
    }
}

pub fn clipboard_write_format(format: &str, text: &str) -> Result<()> {
    match format {
        "html" => pasteboard_set_string("NSPasteboardTypeHTML", text),
        "rtf" => pasteboard_set_string("NSPasteboardTypeRTF", text),
        _ => clipboard_write(text),
    }
}

/// Render the given NSWindow's WKWebView to PDF bytes (vector, full page).
/// The completion runs later on the main thread, so the result is delivered
/// through `on_done` instead of being returned — never block the event loop.
pub fn print_webview_to_pdf(
    ns_window: *mut std::ffi::c_void,
    on_done: Box<dyn FnOnce(Result<Vec<u8>, String>)>,
) -> Result<()> {
    use block2::ConcreteBlock;
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use objc2::msg_send;
    use std::ffi::CStr;
    use std::os::raw::c_char;

    if ns_window.is_null() {
        anyhow::bail!("null window");
    }
    let wk_class = AnyClass::get(c"WKWebView").ok_or_else(|| anyhow::anyhow!("WKWebView missing"))?;

    unsafe fn find_webview(view: *mut AnyObject, wk: &AnyClass) -> *mut AnyObject {
        if view.is_null() {
            return std::ptr::null_mut();
        }
        let is_kind: Bool = msg_send![view, isKindOfClass: wk];
        if is_kind.as_bool() {
            return view;
        }
        let subs: *mut AnyObject = msg_send![view, subviews];
        if subs.is_null() {
            return std::ptr::null_mut();
        }
        let count: usize = msg_send![subs, count];
        for i in 0..count {
            let sub: *mut AnyObject = msg_send![subs, objectAtIndex: i];
            let found = find_webview(sub, wk);
            if !found.is_null() {
                return found;
            }
        }
        std::ptr::null_mut()
    }

    // WKPDFConfiguration with defaults (full page).
    let pdf_class =
        AnyClass::get(c"WKPDFConfiguration").ok_or_else(|| anyhow::anyhow!("WKPDFConfiguration missing (macOS 14.2+)"))?;
    let webview = unsafe {
        let content: *mut AnyObject = msg_send![ns_window as *mut AnyObject, contentView];
        find_webview(content, wk_class)
    };
    if webview.is_null() {
        anyhow::bail!("no WKWebView in window");
    }

    struct Responder(Box<dyn FnOnce(Result<Vec<u8>, String>)>);
    // Block2 needs Fn + Clone; funnel through a shared one-shot cell.
    let cell = std::sync::Arc::new(std::sync::Mutex::new(Some(Responder(on_done))));
    let cell2 = cell.clone();
    let block = ConcreteBlock::new(
        move |data: *mut AnyObject, error: *mut AnyObject| {
            // SAFETY: WebKit guarantees non-null valid objects per branch.
            let result = unsafe {
                if !data.is_null() {
                    // NOTE: `bytes` must be declared as `*const c_void`
                    // (type code `^v`); `*const u8` fails verification.
                    let bytes: *const std::ffi::c_void = msg_send![data, bytes];
                    let len: usize = msg_send![data, length];
                    let slice = std::slice::from_raw_parts(bytes as *const u8, len);
                    Ok(slice.to_vec())
                } else if !error.is_null() {
                    let desc: *mut AnyObject = msg_send![error, localizedDescription];
                    let msg = if desc.is_null() {
                        "pdf failed".to_string()
                    } else {
                        let c: *const c_char = msg_send![desc, UTF8String];
                        if c.is_null() {
                            "pdf failed".to_string()
                        } else {
                            CStr::from_ptr(c).to_string_lossy().into_owned()
                        }
                    };
                    Err(msg)
                } else {
                    Err("empty pdf result".to_string())
                }
            };
            if let Ok(mut guard) = cell2.lock() {
                if let Some(Responder(f)) = guard.take() {
                    f(result);
                }
            }
        },
    );
    let block = block.copy();
    unsafe {
        let config: *mut AnyObject = msg_send![pdf_class, new];
        let _: () = msg_send![webview, createPDFWithConfiguration: config, completionHandler: &*block];
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{DialogMessageArgs, DialogOpenArgs, DialogSaveArgs};

    #[test]
    fn capture_args_shape() {
        let argv = screencapture_args(10, 20, 800, 600);
        assert_eq!(argv[0], "-R");
        assert_eq!(argv[1], "10,20,800,600");
        assert!(argv.contains(&"-x".to_string()));
    }

    /// Compile-check generated AppleScripts without executing them
    /// (execution would pop native dialogs and block the test).
    #[cfg(target_os = "macos")]
    fn compiles(script: &str) {
        let out = std::process::Command::new("osacompile")
            .args(["-o", "/dev/null", "-e", script])
            .output()
            .expect("osacompile runs");
        assert!(
            out.status.success(),
            "AppleScript compile failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn dialog_scripts_compile() {
        compiles(&open_script(&DialogOpenArgs {
            title: Some("Pick".into()),
            default_path: Some("/tmp".into()),
            filters: vec![("Images".into(), vec!["png".into(), "jpg".into()])],
            multiple: true,
            directory: false,
        }));
        compiles(&open_script(&DialogOpenArgs {
            title: None,
            default_path: None,
            filters: vec![],
            multiple: false,
            directory: true,
        }));
        compiles(&save_script(&DialogSaveArgs {
            title: Some("Save".into()),
            default_path: Some("/tmp".into()),
            default_name: Some("out.txt".into()),
        }));
        compiles(&message_script(
            &DialogMessageArgs {
                title: Some("T".into()),
                message: "Sure?".into(),
                detail: Some("Think".into()),
                buttons: vec!["Yes".into(), "No".into()],
                kind: "question".into(),
            },
            &["Yes".to_string(), "No".to_string()],
        ));
        compiles("display notification \"hi\" with title \"T\" sound name \"Glass\"");
    }
}
