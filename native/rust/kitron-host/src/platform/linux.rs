//! Linux implementations (WebKitGTK, Wayland / X11 / GTK).
//!
//! Native dialogs go through `zenity`/`kdialog` when present; clipboard
//! images through `xclip`; capture through ImageMagick `import`. All are
//! optional system tools — absence yields a typed error, never a hang.

use anyhow::Result;

use super::{run_cmd, temp_png, DialogMessageArgs, DialogOpenArgs, DialogSaveArgs};

pub fn platform_name() -> &'static str {
    "linux (WebKitGTK)"
}

pub fn webview_backend() -> &'static str {
    "WebKitGTK"
}

pub fn notification_show(
    title: &str,
    body: &str,
    _silent: bool,
    _sound: Option<&str>,
) -> Result<()> {
    // TODO: org.freedesktop.Notifications over D-Bus.
    println!("[Kitron][Rust][linux] Notification: {title} — {body}");
    Ok(())
}

pub fn menu_set() -> Result<()> {
    Ok(())
}

pub fn tray_create(tooltip: &str) -> Result<String> {
    Ok(format!("tray-linux-{tooltip}"))
}

fn has(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn dialog_open(args: &DialogOpenArgs) -> Result<(bool, Vec<String>)> {
    if has("zenity") {
        let mut cmd: Vec<String> = vec![
            "--file-selection".to_string(),
            "--separator=\n".to_string(),
        ];
        if args.multiple {
            cmd.push("--multiple".to_string());
        }
        if args.directory {
            cmd.push("--directory".to_string());
        }
        if let Some(t) = args.title.as_deref() {
            cmd.push(format!("--title={t}"));
        }
        if let Some(p) = args.default_path.as_deref() {
            cmd.push(format!("--filename={p}/"));
        }
        for (name, exts) in &args.filters {
            let pats = exts.iter().map(|e| format!("*.{e}")).collect::<Vec<_>>().join(" ");
            cmd.push(format!("--file-filter={name} | {pats}"));
        }
        let refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        return match run_cmd("zenity", &refs) {
            Ok(out) => Ok((
                false,
                out.lines().map(|l| l.to_string()).filter(|l| !l.is_empty()).collect(),
            )),
            Err(_) => Ok((true, vec![])),
        };
    }
    if has("kdialog") {
        let mut cmd: Vec<String> = vec!["--getopenfilename".to_string()];
        if let Some(p) = args.default_path.as_deref() {
            cmd.push(p.to_string());
        }
        let refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        return match run_cmd("kdialog", &refs) {
            Ok(out) if !out.is_empty() => Ok((false, vec![out])),
            _ => Ok((true, vec![])),
        };
    }
    anyhow::bail!("no native file dialog available (install zenity)")
}

pub fn dialog_save(args: &DialogSaveArgs) -> Result<(bool, Vec<String>)> {
    if has("zenity") {
        let mut cmd: Vec<String> = vec!["--file-selection".to_string(), "--save".to_string()];
        if let Some(t) = args.title.as_deref() {
            cmd.push(format!("--title={t}"));
        }
        if let Some(p) = args.default_path.as_deref() {
            cmd.push(format!("--filename={p}/"));
        }
        let refs: Vec<&str> = cmd.iter().map(|s| s.as_str()).collect();
        return match run_cmd("zenity", &refs) {
            Ok(out) if !out.is_empty() => Ok((false, vec![out])),
            _ => Ok((true, vec![])),
        };
    }
    anyhow::bail!("no native file dialog available (install zenity)")
}

pub fn dialog_message(args: &DialogMessageArgs) -> Result<(bool, usize)> {
    if has("zenity") {
        let kind = match args.kind.as_str() {
            "error" => "--error",
            "warning" => "--warning",
            "question" => "--question",
            _ => "--info",
        };
        let text = if let Some(d) = args.detail.as_deref() {
            format!("{}\n\n{}", args.message, d)
        } else {
            args.message.clone()
        };
        let r = run_cmd(
            "zenity",
            &[
                kind,
                "--text",
                &text,
                "--title",
                args.title.as_deref().unwrap_or("Kitron"),
            ],
        );
        // zenity question: 0 = OK/Yes. Others have a single dismiss button.
        return match r {
            Ok(_) => Ok((false, 0)),
            Err(_) => Ok((args.kind == "question", 0)),
        };
    }
    anyhow::bail!("no native dialog available (install zenity)")
}

pub fn clipboard_read_image() -> Result<Option<Vec<u8>>> {
    for tool in ["xclip", "xsel"] {
        if !has(tool) {
            continue;
        }
        let args: &[&str] = if tool == "xclip" {
            &["-selection", "clipboard", "-t", "image/png", "-o"]
        } else {
            &["--clipboard", "--output"]
        };
        if let Ok(out) = std::process::Command::new(tool).args(args).output() {
            if out.status.success() && looks_like_png(&out.stdout) {
                return Ok(Some(out.stdout));
            }
            if out.status.success() && !out.stdout.is_empty() {
                return Ok(Some(out.stdout));
            }
        }
        return Ok(None);
    }
    anyhow::bail!("no clipboard image tool available (install xclip)")
}

fn looks_like_png(bytes: &[u8]) -> bool {
    bytes.len() > 8 && &bytes[..8] == b"\x89PNG\r\n\x1a\n"
}

pub fn clipboard_write_image(png: &[u8]) -> Result<()> {
    if has("xclip") {
        let mut child = std::process::Command::new("xclip")
            .args(["-selection", "clipboard", "-t", "image/png", "-i"])
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        use std::io::Write;
        child.stdin.take().map(|mut s| s.write_all(png));
        let status = child.wait()?;
        if status.success() {
            return Ok(());
        }
    }
    anyhow::bail!("no clipboard image tool available (install xclip)")
}

pub fn capture_window_rect(x: i32, y: i32, w: u32, h: u32) -> Result<Vec<u8>> {
    if has("import") {
        let tmp = temp_png("cap");
        let tmp_str = tmp.to_string_lossy().to_string();
        run_cmd(
            "import",
            &["-window", "root", "-crop", &format!("{w}x{h}+{x}+{y}"), &tmp_str],
        )?;
        let bytes = std::fs::read(&tmp)?;
        let _ = std::fs::remove_file(&tmp);
        return Ok(bytes);
    }
    anyhow::bail!("no screenshot tool available (install imagemagick)")
}

fn xclip_base(format: &str) -> Option<Vec<&'static str>> {
    if !has("xclip") {
        return None;
    }
    Some(match format {
        "html" => vec!["-selection", "clipboard", "-t", "text/html"],
        "rtf" => vec!["-selection", "clipboard", "-t", "text/rtf"],
        _ => vec!["-selection", "clipboard", "-t", "UTF8_STRING"],
    })
}

pub fn clipboard_read() -> Result<String> {
    clipboard_read_format("text")
}

pub fn clipboard_write(text: &str) -> Result<()> {
    clipboard_write_format("text", text)
}

pub fn clipboard_read_format(format: &str) -> Result<String> {
    let mut args = xclip_base(format).ok_or_else(|| anyhow::anyhow!("xclip missing"))?;
    args.push("-o");
    let out = std::process::Command::new("xclip").args(&args).output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        anyhow::bail!("clipboard empty or unavailable")
    }
}

pub fn clipboard_write_format(format: &str, text: &str) -> Result<()> {
    let mut args = xclip_base(format).ok_or_else(|| anyhow::anyhow!("xclip missing"))?;
    args.push("-i");
    let mut child = std::process::Command::new("xclip")
        .args(&args)
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    use std::io::Write;
    child.stdin.take().map(|mut s| s.write_all(text.as_bytes()));
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("xclip write failed")
    }
}
