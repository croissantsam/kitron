//! Kitron native host library — the minimal Rust bridge (§5, §35).
//!
//! Rule: Rust owns native windows, WebViews, OS APIs and IPC transport.
//! Everything else (filesystem, networking, business logic) belongs to Node.js.

pub mod ipc;
pub mod notification;
pub mod platform;
pub mod webview;
pub mod window;

use serde_json::{json, Value};

use crate::ipc::{IpcRequest, IpcResponse};
use crate::window::{WindowManager, WindowOptions};

/// Central dispatcher: IPC channel → native action.
pub struct Host {
    pub windows: WindowManager,
    pub app_name: String,
    pub app_version: String,
    ready: bool,
}

impl Host {
    pub fn new(app_name: String, app_version: String) -> Self {
        Self {
            windows: WindowManager::default(),
            app_name,
            app_version,
            ready: true,
        }
    }

    pub fn handle(&mut self, req: IpcRequest) -> IpcResponse {
        let ok = |v: Value| IpcResponse::ok(req.id.clone(), v);
        let fail = |code: &str, msg: String| IpcResponse::err(req.id.clone(), code, msg);
        let p = &req.payload;

        match req.channel.as_str() {
            "ping" => ok(json!("pong")),
            "app.ready" => ok(Value::Null),
            "app.isReady" => ok(json!(self.ready)),
            "app.getName" => ok(json!(self.app_name)),
            "app.getVersion" => ok(json!(self.app_version)),
            "app.getInfo" => ok(json!({
                "name": self.app_name,
                "version": self.app_version,
                "platform": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "ready": self.ready,
            })),
            "app.getPath" => {
                let name = p.get("name").and_then(|v| v.as_str()).unwrap_or("userData");
                ok(json!(app_path(name, &self.app_name)))
            }
            "app.quit" | "app.exit" => {
                self.ready = false;
                ok(Value::Null)
            }

            "window.create" => {
                let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if id.is_empty() {
                    return fail("WINDOW_FAILED", "window.create requires an id".into());
                }
                let options: WindowOptions = p
                    .get("options")
                    .map(|v| serde_json::from_value(v.clone()).unwrap_or_default())
                    .unwrap_or_default();
                self.windows.create(id.to_string(), options);
                eprintln!("[Kitron][Rust] WebView created (window {id})");
                ok(json!({ "id": id }))
            }
            c if c.starts_with("window.") => {
                let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
                if self.windows.get(id).is_none() {
                    return fail("WINDOW_FAILED", format!("Unknown window \"{id}\""));
                }
                match c {
                    "window.loadURL" | "window.loadFile" | "window.show" | "window.hide"
                    | "window.close" | "window.focus" | "window.minimize" | "window.maximize"
                    | "window.restore" | "window.reload" | "window.openDevTools"
                    | "window.closeDevTools" | "window.setSize" | "window.setPosition"
                    | "window.setAlwaysOnTop" => ok(Value::Null),
                    "window.setTitle" => {
                        if let Some(w) = self.windows.get_mut(id) {
                            if let Some(t) = p.get("title").and_then(|v| v.as_str()) {
                                w.options.title = Some(t.to_string());
                            }
                        }
                        ok(Value::Null)
                    }
                    "window.isMaximized" => {
                        ok(json!(self.windows.get(id).map(|w| w.maximized).unwrap_or(false)))
                    }
                    "window.setFullscreen" => {
                        let flag = p.get("flag").and_then(|v| v.as_bool()).unwrap_or(true);
                        if let Some(w) = self.windows.get_mut(id) {
                            w.fullscreen = flag;
                        }
                        ok(Value::Null)
                    }
                    "window.isFullscreen" => {
                        ok(json!(self.windows.get(id).map(|w| w.fullscreen).unwrap_or(false)))
                    }
                    "window.setMinimumSize" | "window.setMaximumSize" => ok(Value::Null),                    _ => fail("HANDLER_NOT_FOUND", format!("Unknown channel \"{c}\"")),
                }
            }

            "native.notification.show" => {
                let opts: crate::notification::ShowOptions = match serde_json::from_value(p.clone()) {
                    Ok(o) => o,
                    Err(e) => return fail("INVALID_PAYLOAD", e.to_string()),
                };
                match crate::notification::show(opts) {
                    Ok(()) => ok(Value::Null),
                    Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                }
            }
            "native.dialog.open" => {
                let multiple = p
                    .get("multiple")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let directory = p
                    .get("directory")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let args = crate::platform::DialogOpenArgs {
                    title: p.get("title").and_then(|v| v.as_str()).map(str::to_string),
                    default_path: p
                        .get("defaultPath")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    filters: p
                        .get("filters")
                        .and_then(|v| v.as_array())
                        .map(|a| {
                            a.iter()
                                .filter_map(|f| {
                                    Some((
                                        f.get("name")
                                            .and_then(|v| v.as_str())?
                                            .to_string(),
                                        f.get("extensions")
                                            .and_then(|v| v.as_array())
                                            .map(|e| {
                                                e.iter()
                                                    .filter_map(|x| {
                                                        x.as_str().map(str::to_string)
                                                    })
                                                    .collect()
                                            })
                                            .unwrap_or_default(),
                                    ))
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    multiple,
                    directory,
                };
                match crate::platform::dialog_open(&args) {
                    Ok((canceled, file_paths)) => {
                        ok(json!({ "canceled": canceled, "filePaths": file_paths }))
                    }
                    Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                }
            }
            "native.dialog.save" => {
                let args = crate::platform::DialogSaveArgs {
                    title: p.get("title").and_then(|v| v.as_str()).map(str::to_string),
                    default_path: p
                        .get("defaultPath")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    default_name: None,
                };
                match crate::platform::dialog_save(&args) {
                    Ok((canceled, file_paths)) => {
                        ok(json!({ "canceled": canceled, "filePaths": file_paths }))
                    }
                    Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                }
            }
            "native.dialog.message" => {
                let buttons = p
                    .get("buttons")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter().filter_map(|b| b.as_str().map(str::to_string)).collect()
                    })
                    .unwrap_or_else(|| vec!["OK".to_string()]);
                let args = crate::platform::DialogMessageArgs {
                    title: p.get("title").and_then(|v| v.as_str()).map(str::to_string),
                    message: p.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    detail: p.get("detail").and_then(|v| v.as_str()).map(str::to_string),
                    buttons,
                    kind: p.get("kind").and_then(|v| v.as_str()).unwrap_or("info").to_string(),
                };
                match crate::platform::dialog_message(&args) {
                    Ok((canceled, response)) => {
                        ok(json!({ "canceled": canceled, "response": response }))
                    }
                    Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                }
            }
            "native.menu.set" => {
                #[cfg(target_os = "macos")]
                {
                    let template = p.get("template").cloned().unwrap_or_else(|| p.clone());
                    match crate::platform::apply_menu_template(&template) {
                        Ok(()) => ok(Value::Null),
                        Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    ok(Value::Null)
                }
            }
            "native.menu.popup" => {
                #[cfg(target_os = "macos")]
                {
                    let items = p.get("items").cloned().unwrap_or(Value::Null);
                    let x = p.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let y = p.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    match crate::platform::popup_menu(&items, x, y) {
                        Ok(()) => ok(Value::Null),
                        Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    ok(Value::Null)
                }
            }
            "native.tray.create" => {
                let id = p
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| {
                        use std::time::{SystemTime, UNIX_EPOCH};
                        format!(
                            "tray-{}",
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map(|d| d.as_millis())
                                .unwrap_or(0)
                        )
                    });
                #[cfg(target_os = "macos")]
                {
                    let title = p.get("title").and_then(|v| v.as_str());
                    let tooltip = p.get("tooltip").and_then(|v| v.as_str());
                    let menu = p.get("menu");
                    match crate::platform::tray_create(&id, title, tooltip, menu) {
                        Ok(()) => ok(json!(id)),
                        Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    ok(json!(id))
                }
            }
            "native.tray.destroy" => {
                #[cfg(target_os = "macos")]
                {
                    let id = p.get("id").and_then(|v| v.as_str()).unwrap_or("");
                    match crate::platform::tray_destroy(id) {
                        Ok(()) => ok(Value::Null),
                        Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                    }
                }
                #[cfg(not(target_os = "macos"))]
                {
                    ok(Value::Null)
                }
            }
            "native.desktop.getSources" => {
                // Real list needs the event loop (screens) — the feature
                // build intercepts before reaching the router.
                fail(
                    "PLATFORM_UNSUPPORTED",
                    "desktopCapturer requires the real native host".into(),
                )
            }
            "native.clipboard.readText" => {
                match crate::platform::clipboard_read() {
                    Ok(t) => ok(json!(t)),
                    Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                }
            }
            "native.clipboard.read" => {
                let format = p.get("format").and_then(|v| v.as_str()).unwrap_or("text");
                match crate::platform::clipboard_read_format(format) {
                    Ok(t) => ok(json!(t)),
                    Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                }
            }
            "native.clipboard.writeText" => {
                let text = p.get("text").and_then(|v| v.as_str()).unwrap_or("");
                match crate::platform::clipboard_write(text) {
                    Ok(()) => ok(Value::Null),
                    Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                }
            }
            "native.clipboard.writeFormat" => {
                let format = p.get("format").and_then(|v| v.as_str()).unwrap_or("text");
                let text = p.get("text").and_then(|v| v.as_str()).unwrap_or("");
                match crate::platform::clipboard_write_format(format, text) {
                    Ok(()) => ok(Value::Null),
                    Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                }
            }
            "native.clipboard.readImage" => {
                match crate::platform::clipboard_read_image() {
                    Ok(Some(bytes)) => ok(json!({
                        "available": true,
                        "pngBase64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes),
                    })),
                    Ok(None) => ok(json!({ "available": false, "pngBase64": "" })),
                    Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                }
            }
            "native.clipboard.writeImage" => {
                let b64 = p.get("pngBase64").and_then(|v| v.as_str()).unwrap_or("");
                match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64) {
                    Ok(bytes) => match crate::platform::clipboard_write_image(&bytes) {
                        Ok(()) => ok(Value::Null),
                        Err(e) => fail("NATIVE_API_FAILED", e.to_string()),
                    },
                    Err(e) => fail("INVALID_PAYLOAD", e.to_string()),
                }
            }
            "native.clipboard.clear" => ok(Value::Null),
            "native.screen.getDisplays" => ok(json!([])),
            "native.screen.getCursor" => ok(json!({ "x": 0, "y": 0 })),
            "native.shortcut.register" | "native.shortcut.unregister" => ok(Value::Null),
            "native.window.setAlwaysOnTop" | "native.window.setTitle" => ok(Value::Null),
            "native.power.saveBlockerStart" => ok(json!(0)),
            "native.power.saveBlockerStop" => ok(Value::Null),
            // Tranche B: needs a JS engine — the feature build intercepts
            // these before reaching the router; headless answers explicitly.
            "webcontents.evalJS"
            | "webcontents.getURL"
            | "webcontents.getTitle"
            | "webcontents.setZoomFactor"
            | "webcontents.capturePage" => fail(
                "PLATFORM_UNSUPPORTED",
                "webContents evaluation requires the real native host".into(),
            ),
            "webcontents.getZoomFactor" => ok(json!(1)),
            "webcontents.reload" => ok(Value::Null),
            "webcontents.printToPDF" => fail(
                "PLATFORM_UNSUPPORTED",
                "printToPDF requires the real native host (macOS supported)".into(),
            ),
            "updater.check" => ok(json!({ "available": false })),
            "updater.download" | "updater.install" => ok(Value::Null),
            other => fail("HANDLER_NOT_FOUND", format!("No handler for channel \"{other}\"")),
        }
    }
}

fn app_path(name: &str, app_name: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    match name {
        "home" => home.clone(),
        "temp" => std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into()),
        "userData" => format!("{home}/Library/Application Support/{app_name}"),
        "appData" => format!("{home}/Library/Application Support"),
        "documents" => format!("{home}/Documents"),
        "downloads" => format!("{home}/Downloads"),
        "desktop" => format!("{home}/Desktop"),
        "logs" => format!("{home}/Library/Logs/{app_name}"),
        _ => format!("{home}/Library/Application Support/{app_name}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(channel: &str) -> IpcRequest {
        IpcRequest {
            id: "1".into(),
            channel: channel.into(),
            payload: Value::Null,
        }
    }

    #[test]
    fn ping_pong() {
        let mut h = Host::new("App".into(), "0.1.0".into());
        let res = h.handle(req("ping"));
        assert!(res.success);
        assert_eq!(res.payload, Some(json!("pong")));
    }

    #[test]
    fn window_lifecycle() {
        let mut h = Host::new("App".into(), "0.1.0".into());
        let res = h.handle(IpcRequest {
            id: "1".into(),
            channel: "window.create".into(),
            payload: json!({ "id": "w1", "options": { "title": "Hi" } }),
        });
        assert!(res.success);
        let res = h.handle(IpcRequest {
            id: "2".into(),
            channel: "window.setTitle".into(),
            payload: json!({ "id": "w1", "title": "Yo" }),
        });
        assert!(res.success);
        let res = h.handle(IpcRequest {
            id: "3".into(),
            channel: "window.show".into(),
            payload: json!({ "id": "nope" }),
        });
        assert!(!res.success);
    }

    #[test]
    fn fullscreen_state() {
        let mut h = Host::new("App".into(), "0.1.0".into());
        let res = h.handle(IpcRequest {
            id: "1".into(),
            channel: "window.create".into(),
            payload: json!({ "id": "w1" }),
        });
        assert!(res.success);
        let res = h.handle(IpcRequest {
            id: "2".into(),
            channel: "window.setFullscreen".into(),
            payload: json!({ "id": "w1", "flag": true }),
        });
        assert!(res.success);
        let res = h.handle(IpcRequest {
            id: "3".into(),
            channel: "window.isFullscreen".into(),
            payload: json!({ "id": "w1" }),
        });
        assert_eq!(res.payload, Some(json!(true)));
    }

    #[test]
    fn webcontents_headless_answers_explicitly() {
        let mut h = Host::new("App".into(), "0.1.0".into());
        let res = h.handle(IpcRequest {
            id: "1".into(),
            channel: "webcontents.evalJS".into(),
            payload: json!({ "id": "w1", "code": "1" }),
        });
        assert!(!res.success);
        let res = h.handle(IpcRequest {
            id: "2".into(),
            channel: "webcontents.getZoomFactor".into(),
            payload: json!({ "id": "w1" }),
        });
        assert_eq!(res.payload, Some(json!(1)));
    }

    #[test]
    fn unknown_channel_errors() {
        let mut h = Host::new("App".into(), "0.1.0".into());
        let res = h.handle(req("nope.nothing"));
        assert!(!res.success);
    }
}
