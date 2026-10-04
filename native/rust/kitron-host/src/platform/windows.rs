//! Windows implementations (WebView2 / WinRT).
//!
//! WebView2 is consumed as the OS-provided Edge runtime — Kitron never
//! ships a second Chromium. Native dialogs, clipboard images and capture
//! go through PowerShell + WinForms (STA).

use anyhow::Result;

use super::{run_cmd, temp_png, DialogMessageArgs, DialogOpenArgs, DialogSaveArgs};

pub fn platform_name() -> &'static str {
    "windows (WebView2)"
}

pub fn webview_backend() -> &'static str {
    "WebView2"
}

fn ps(script: &str) -> Result<String> {
    run_cmd(
        "powershell",
        &["-NoProfile", "-STA", "-Command", script],
    )
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

pub fn notification_show(
    title: &str,
    body: &str,
    _silent: bool,
    _sound: Option<&str>,
) -> Result<()> {
    // TODO: Windows.UI.Notifications toast via `windows` crate.
    println!("[Kitron][Rust][windows] Notification: {title} — {body}");
    Ok(())
}

pub fn menu_set() -> Result<()> {
    Ok(())
}

pub fn tray_create(tooltip: &str) -> Result<String> {
    Ok(format!("tray-windows-{tooltip}"))
}

fn ext_filter(filters: &[(String, Vec<String>)]) -> String {
    if filters.is_empty() {
        return "All files (*.*)|*.*".to_string();
    }
    filters
        .iter()
        .map(|(name, exts)| {
            let pats = exts.iter().map(|e| format!("*.{e}")).collect::<Vec<_>>().join(";");
            format!("{name} ({pats})|{pats}")
        })
        .collect::<Vec<_>>()
        .join("|")
}

pub fn dialog_open(args: &DialogOpenArgs) -> Result<(bool, Vec<String>)> {
    if args.directory {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = {}; $d.SelectedPath = {}; if ($d.ShowDialog() -eq 'OK') {{ $d.SelectedPath }} else {{ exit 1 }}",
            ps_quote(args.title.as_deref().unwrap_or("Choose")),
            ps_quote(args.default_path.as_deref().unwrap_or("")),
        );
        return match ps(&script) {
            Ok(out) => Ok((false, vec![out])),
            Err(_) => Ok((true, vec![])),
        };
    }
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.OpenFileDialog; $d.Title = {}; $d.InitialDirectory = {}; $d.Filter = {}; $d.Multiselect = ${}; if ($d.ShowDialog() -eq 'OK') {{ $d.FileNames -join \"`n\" }} else {{ exit 1 }}",
        ps_quote(args.title.as_deref().unwrap_or("Choose")),
        ps_quote(args.default_path.as_deref().unwrap_or("")),
        ps_quote(&ext_filter(&args.filters)),
        if args.multiple { "true" } else { "false" },
    );
    match ps(&script) {
        Ok(out) => Ok((
            false,
            out.lines().map(|l| l.to_string()).filter(|l| !l.is_empty()).collect(),
        )),
        Err(_) => Ok((true, vec![])),
    }
}

pub fn dialog_save(args: &DialogSaveArgs) -> Result<(bool, Vec<String>)> {
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; $d = New-Object System.Windows.Forms.SaveFileDialog; $d.Title = {}; $d.InitialDirectory = {}; if ($d.ShowDialog() -eq 'OK') {{ $d.FileName }} else {{ exit 1 }}",
        ps_quote(args.title.as_deref().unwrap_or("Save")),
        ps_quote(args.default_path.as_deref().unwrap_or("")),
    );
    match ps(&script) {
        Ok(out) if !out.is_empty() => Ok((false, vec![out])),
        _ => Ok((true, vec![])),
    }
}

pub fn dialog_message(args: &DialogMessageArgs) -> Result<(bool, usize)> {
    // Map button counts onto MessageBoxButtons; results translate back
    // to indices below (OK/Yes → 0, Cancel/No → last).
    let mb: &str = match args.buttons.len() {
        0 | 1 => "OK",
        2 => "OKCancel",
        _ => "YesNoCancel",
    };
    let text = if let Some(d) = args.detail.as_deref() {
        format!("{}\n\n{}", args.message, d)
    } else {
        args.message.clone()
    };
    let icon = match args.kind.as_str() {
        "error" => "Error",
        "warning" => "Exclamation",
        "question" => "Question",
        _ => "Information",
    };
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.MessageBox]::Show({}, {}, [System.Windows.Forms.MessageBoxButtons]::{}, [System.Windows.Forms.MessageBoxIcon]::{})",
        ps_quote(&text),
        ps_quote(args.title.as_deref().unwrap_or("Kitron")),
        mb,
        icon,
    );
    match ps(&script) {
        Ok(out) => {
            // DialogResult → our first-button-first ordering approximation.
            let idx = match out.trim() {
                "OK" | "Yes" => 0,
                "Cancel" | "No" => 1.min(args.buttons.len().saturating_sub(1)),
                _ => 0,
            };
            Ok((false, idx))
        }
        Err(_) => Ok((true, 0)),
    }
}

pub fn clipboard_read_image() -> Result<Option<Vec<u8>>> {
    let tmp = temp_png("clip");
    let tmp_str = tmp.to_string_lossy().replace('\\', "\\\\");
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; $img = [System.Windows.Forms.Clipboard]::GetImage(); if ($null -eq $img) {{ exit 2 }}; $img.Save('{tmp_str}', [System.Drawing.Imaging.ImageFormat]::Png)",
    );
    match ps(&script) {
        Ok(_) => {
            let bytes = std::fs::read(&tmp)?;
            let _ = std::fs::remove_file(&tmp);
            Ok(Some(bytes))
        }
        Err(e) if e.to_string().contains("exit 2") || e.to_string().contains("exit code: 2") => {
            Ok(None)
        }
        Err(e) => {
            let m = e.to_string();
            if m.contains('2') {
                Ok(None)
            } else {
                Err(e)
            }
        }
    }
}

pub fn clipboard_write_image(png: &[u8]) -> Result<()> {
    let tmp = temp_png("clipw");
    std::fs::write(&tmp, png)?;
    let tmp_str = tmp.to_string_lossy().replace('\\', "\\\\");
    let script = format!(
        "Add-Type -AssemblyName System.Windows.Forms; Add-Type -AssemblyName System.Drawing; $img = [System.Drawing.Image]::FromFile('{tmp_str}'); [System.Windows.Forms.Clipboard]::SetImage($img)",
    );
    let r = ps(&script);
    let _ = std::fs::remove_file(&tmp);
    r?;
    Ok(())
}

pub fn capture_window_rect(x: i32, y: i32, w: u32, h: u32) -> Result<Vec<u8>> {
    let tmp = temp_png("cap");
    let tmp_str = tmp.to_string_lossy().replace('\\', "\\\\");
    let script = format!(
        "Add-Type -AssemblyName System.Drawing; $b = New-Object System.Drawing.Bitmap({w}, {h}); $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen({x}, {y}, 0, 0, $b.Size); $b.Save('{tmp_str}', [System.Drawing.Imaging.ImageFormat]::Png)",
    );
    ps(&script)?;
    let bytes = std::fs::read(&tmp)?;
    let _ = std::fs::remove_file(&tmp);
    Ok(bytes)
}

pub fn clipboard_read() -> Result<String> {
    ps("[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; Get-Clipboard -Raw")
}

pub fn clipboard_write(text: &str) -> Result<()> {
    let script = format!("Set-Clipboard -Value {}", ps_quote(text));
    ps(&script)?;
    Ok(())
}

pub fn clipboard_read_format(format: &str) -> Result<String> {
    match format {
        "html" | "rtf" => {
            let kind = if format == "html" { "Html" } else { "Rtf" };
            ps(&format!("[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; Get-Clipboard -Raw -TextFormatType {kind}"))
        }
        _ => clipboard_read(),
    }
}

pub fn clipboard_write_format(format: &str, text: &str) -> Result<()> {
    match format {
        "html" | "rtf" => {
            let kind = if format == "html" { "Html" } else { "Rtf" };
            ps(&format!("Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.Clipboard]::SetText({}, [System.Windows.Forms.TextDataFormat]::{kind})", ps_quote(text)))?;
            Ok(())
        }
        _ => clipboard_write(text),
    }
}
