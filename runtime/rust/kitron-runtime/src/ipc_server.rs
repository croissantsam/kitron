//! IPC dispatcher — routes `{domain, method, params}` to managers and
//! returns typed `{ok, result} | {ok:false, error}` envelopes.
//! Transport (MVP): newline-delimited JSON over stdio; Unix socket /
//! named pipe and JSI transports reuse this dispatch table unchanged.

use crate::app::AppManager;
use crate::clipboard::Clipboard;
use crate::menu::MenuItem;
use crate::menu::MenuManager;
use crate::node_sidecar::NodeSidecar;
use crate::process_mgr::ProcessManager;
use crate::shortcuts::ShortcutManager;
use crate::tray::TrayManager;
use crate::windows::WindowManager;
use kitron_core::{IpcRequest, IpcResponse, WindowCreateOptions};
use kitron_core::permissions::Permissions;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Runtime {
    pub windows: Arc<WindowManager>,
    pub app: Arc<AppManager>,
    pub node: Arc<NodeSidecar>,
    pub procs: Arc<ProcessManager>,
    pub trays: Arc<TrayManager>,
    pub menus: Arc<MenuManager>,
    pub shortcuts: Arc<ShortcutManager>,
    pub permissions: Arc<Permissions>,
    pub updater_state: Arc<crate::updater::UpdaterState>,
    pub crashes: Arc<crate::crash::CrashStore>,
    pub ui: Arc<crate::ui::UiManager>,
}

impl Runtime {
    pub fn new(app: Arc<AppManager>) -> Self {
        Self::new_with_permissions(app, Permissions::empty())
    }

    pub fn new_with_permissions(app: Arc<AppManager>, permissions: Permissions) -> Self {
        let rt = Self {
            windows: Arc::new(WindowManager::new()),
            app,
            node: Arc::new(NodeSidecar::new()),
            procs: Arc::new(ProcessManager::new()),
            trays: Arc::new(TrayManager::new()),
            menus: Arc::new(MenuManager::new()),
            shortcuts: Arc::new(ShortcutManager::new()),
            permissions: Arc::new(permissions),
            updater_state: Arc::new(crate::updater::UpdaterState::new()),
            crashes: Arc::new(crate::crash::CrashStore::new()),
            ui: Arc::new(crate::ui::UiManager::new()),
        };
        // Async UI completions (image loads) repaint without a dispatch round-trip.
        rt.ui.set_redraw_hook(Arc::new({
            let windows = rt.windows.clone();
            move || windows.redraw_all()
        }));
        rt
    }

    /// Build OS tray icons / app menu / hotkeys on the main thread.
    /// Idempotent (managers carry once-guards); `hwnd` is the Win32 window
    /// handle for `init_for_hwnd`, `None` elsewhere.
    pub fn bind_os(&self, hwnd: Option<isize>) {
        crate::tray::bind_os(&self.trays, &self.app);
        crate::shortcuts::bind_os(&self.shortcuts, &self.app);
        crate::menu::bind_os(&self.menus, &self.app, hwnd);
    }

    pub async fn dispatch(&self, req: IpcRequest) -> IpcResponse {
        let id = req.id;
        // §23: sensitive domains require an explicit grant. No
        // `native.executeAnything` escape hatch exists.
        if !self.permissions.is_allowed(&req.domain) {
            return IpcResponse::err(
                id,
                "PERMISSION_DENIED",
                format!(
                    "`{}` requires an explicit permission: add it to `permissions` in kitron.config or pass `--permissions {}` to the runtime",
                    req.domain, req.domain
                ),
            );
        }
        let p = req.params.clone();
        let get_str = |k: &str| p.get(k).and_then(|v| v.as_str()).map(|s| s.to_string());
        let get_u32 = |k: &str| p.get(k).and_then(|v| v.as_u64()).map(|v| v as u32);
        let get_i32 = |k: &str| p.get(k).and_then(|v| v.as_i64()).map(|v| v as i32);

        match (req.domain.as_str(), req.method.as_str()) {
            // ---- app (§11, §20-§21) ----
            ("app", "ready") => {
                self.app.ready();
                IpcResponse::ok(id, serde_json::Value::Null)
            }
            ("app", "quit") => {
                self.app.quit();
                IpcResponse::ok(id, serde_json::Value::Null)
            }
            ("app", "hide") => {
                self.app.hide();
                IpcResponse::ok(id, serde_json::Value::Null)
            }
            ("app", "show") => {
                self.app.show();
                IpcResponse::ok(id, serde_json::Value::Null)
            }
            // OS entry points (also used by tests/CLI to inject links).
            ("app", "deepLink") => match get_str("url") {
                Some(url) => {
                    self.app.emit("app.deepLink", serde_json::json!({ "url": url }));
                    IpcResponse::ok(id, serde_json::Value::Null)
                }
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "app.deepLink: need url"),
            },
            ("app", "fileOpen") => match get_str("path") {
                Some(path) => {
                    self.app.emit("app.fileOpen", serde_json::json!({ "path": path }));
                    IpcResponse::ok(id, serde_json::Value::Null)
                }
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "app.fileOpen: need path"),
            },
            // ---- window (§10) ----
            ("window", "create") => {
                let opts: WindowCreateOptions = serde_json::from_value(p).unwrap_or_default();
                // Headless registration; the winit event loop upgrades to a live
                // window when running with a display (see kitron binary).
                let info = self.windows.create_headless(opts);
                crate::os_events::request_sync_windows();
                IpcResponse::ok(id, serde_json::json!({ "id": info.id }))
            }
            ("window", "list") => {
                let all = self.windows.all();
                IpcResponse::ok(id, serde_json::to_value(all).unwrap())
            }
            ("window", "get") => match get_str("id") {
                Some(wid) => match self.windows.get(&wid) {
                    Ok(info) => IpcResponse::ok(id, serde_json::to_value(info).unwrap()),
                    Err(e) => IpcResponse::err(id, "WINDOW_NOT_FOUND", e.to_string()),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "window.get: missing id"),
            },
            ("window", "close") => match get_str("id") {
                Some(wid) => match self.windows.close(&wid) {
                    Ok(()) => {
                        crate::os_events::request_sync_windows();
                        IpcResponse::ok(id, serde_json::Value::Null)
                    }
                    Err(e) => IpcResponse::err(id, "WINDOW_NOT_FOUND", e.to_string()),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "window.close: missing id"),
            },
            ("window", m @ ("minimize" | "maximize" | "restore" | "focus" | "show" | "hide")) => {
                match get_str("id") {
                    Some(wid) => {
                        let r = match m {
                            "minimize" => self.windows.minimize(&wid),
                            "maximize" => self.windows.maximize(&wid),
                            "restore" => self.windows.restore(&wid),
                            "focus" => self.windows.focus(&wid),
                            "show" => self.windows.show(&wid),
                            _ => self.windows.hide(&wid),
                        };
                        match r {
                            Ok(info) => IpcResponse::ok(id, serde_json::to_value(info).unwrap()),
                            Err(e) => IpcResponse::err(id, "WINDOW_OPERATION_FAILED", e.to_string()),
                        }
                    }
                    None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "missing id"),
                }
            }
            ("window", "setSize") => {
                let (wid, w, h) = (get_str("id"), get_u32("width"), get_u32("height"));
                match (wid, w, h) {
                    (Some(wid), Some(w), Some(h)) => match self.windows.set_size(&wid, w, h) {
                        Ok(info) => IpcResponse::ok(id, serde_json::to_value(info).unwrap()),
                        Err(e) => IpcResponse::err(id, "WINDOW_OPERATION_FAILED", e.to_string()),
                    },
                    _ => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "window.setSize: need id,width,height"),
                }
            }
            ("window", "setPosition") => {
                let (wid, x, y) = (get_str("id"), get_i32("x"), get_i32("y"));
                match (wid, x, y) {
                    (Some(wid), Some(x), Some(y)) => match self.windows.set_position(&wid, x, y) {
                        Ok(info) => IpcResponse::ok(id, serde_json::to_value(info).unwrap()),
                        Err(e) => IpcResponse::err(id, "WINDOW_OPERATION_FAILED", e.to_string()),
                    },
                    _ => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "window.setPosition: need id,x,y"),
                }
            }
            ("window", "setTitle") => match (get_str("id"), get_str("title")) {
                (Some(wid), Some(t)) => match self.windows.set_title(&wid, t) {
                    Ok(info) => IpcResponse::ok(id, serde_json::to_value(info).unwrap()),
                    Err(e) => IpcResponse::err(id, "WINDOW_OPERATION_FAILED", e.to_string()),
                },
                _ => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "window.setTitle: need id,title"),
            },
            // ---- dialogs (§12) ----
            ("dialog", "openFile") => {
                let multiple = p.get("multiple").and_then(|v| v.as_bool()).unwrap_or(false);
                let filters = crate::dialogs::parse_filters(&p);
                let default_path = get_str("defaultPath");
                match crate::dialogs::open_file(multiple, filters, default_path).await {
                    Some(files) => IpcResponse::ok(id, serde_json::to_value(files).unwrap()),
                    None => IpcResponse::err(id, "DIALOG_CANCELLED", "dialog cancelled"),
                }
            }
            ("dialog", "saveFile") => {
                let filters = crate::dialogs::parse_filters(&p);
                let default_path = get_str("defaultPath");
                match crate::dialogs::save_file(default_path, filters).await {
                    Some(path) => IpcResponse::ok(id, serde_json::Value::String(path)),
                    None => IpcResponse::err(id, "DIALOG_CANCELLED", "dialog cancelled"),
                }
            }
            ("dialog", "selectDirectory") => match crate::dialogs::select_directory().await {
                Some(path) => IpcResponse::ok(id, serde_json::Value::String(path)),
                None => IpcResponse::err(id, "DIALOG_CANCELLED", "dialog cancelled"),
            },
            ("dialog", "message") => {
                let title = get_str("title");
                let message = get_str("message").unwrap_or_default();
                let kind = get_str("kind");
                let buttons: Vec<String> = p
                    .get("buttons")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                let button = crate::dialogs::message(title, message, kind, buttons).await;
                IpcResponse::ok(id, serde_json::json!({ "button": button }))
            }
            // ---- clipboard (§13) ----
            ("clipboard", "writeText") => match get_str("text") {
                Some(text) => match Clipboard::new().and_then(|c| c.write_text(text).map_err(|e| e)) {
                    Ok(()) => IpcResponse::ok(id, serde_json::Value::Null),
                    Err(e) => IpcResponse::err(id, "CLIPBOARD_WRITE_FAILED", e),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "clipboard.writeText: need text"),
            },
            ("clipboard", "readText") => match Clipboard::new().and_then(|c| c.read_text()) {
                Ok(t) => IpcResponse::ok(id, serde_json::Value::String(t)),
                Err(e) => IpcResponse::err(id, "CLIPBOARD_READ_FAILED", e),
            },
            // ---- notifications (§14) ----
            ("notification", "show") => {
                let title = get_str("title").unwrap_or_else(|| "Kitron".into());
                let body = get_str("body").unwrap_or_default();
                match crate::notification::show(title, body) {
                    Ok(()) => IpcResponse::ok(id, serde_json::Value::Null),
                    Err(e) => IpcResponse::err(id, "NOTIFICATION_FAILED", e),
                }
            }
            // ---- shell (§15) ----
            ("shell", "openExternal") => match get_str("url") {
                Some(url) => match crate::shell::open_external(&url) {
                    Ok(()) => IpcResponse::ok(id, serde_json::Value::Null),
                    Err(e) => IpcResponse::err(id, "SHELL_OPEN_FAILED", e),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "shell.openExternal: need url"),
            },
            ("shell", "openPath") => match get_str("path") {
                Some(path) => match crate::shell::open_path(&path) {
                    Ok(()) => IpcResponse::ok(id, serde_json::Value::Null),
                    Err(e) => IpcResponse::err(id, "SHELL_OPEN_FAILED", e),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "shell.openPath: need path"),
            },
            ("shell", "showItemInFolder") => match get_str("path") {
                Some(path) => match crate::shell::show_item_in_folder(&path) {
                    Ok(()) => IpcResponse::ok(id, serde_json::Value::Null),
                    Err(e) => IpcResponse::err(id, "SHELL_OPEN_FAILED", e),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "need path"),
            },
            // ---- process (§16) ----
            ("process", "spawn") => {
                let command = get_str("command").unwrap_or_default();
                if command.is_empty() {
                    return IpcResponse::err(id, "KITRON_INVALID_PARAMS", "process.spawn: need command");
                }
                let args: Vec<String> = p
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                match self.procs.spawn(command, args, get_str("cwd"), None).await {
                    Ok(pid) => IpcResponse::ok(id, serde_json::json!({ "pid": pid })),
                    Err(e) => IpcResponse::err(id, "PROCESS_SPAWN_FAILED", e),
                }
            }
            ("process", "kill") => {
                let pid = p.get("pid").and_then(|v| v.as_u64()).map(|v| v as u32);
                match pid {
                    Some(pid) => match self.procs.kill(pid).await {
                        Ok(()) => IpcResponse::ok(id, serde_json::Value::Null),
                        Err(e) => IpcResponse::err(id, "PROCESS_NOT_FOUND", e),
                    },
                    None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "process.kill: need pid"),
                }
            }
            // ---- node sidecar (§28-§30) ----
            ("node", "start") => {
                let entry = get_str("entry").unwrap_or_else(|| "./backend/index.js".into());
                let args: Vec<String> = p
                    .get("args")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
                    .unwrap_or_default();
                let env: HashMap<String, String> = p
                    .get("env")
                    .and_then(|v| v.as_object())
                    .map(|o| {
                        o.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    })
                    .unwrap_or_default();
                match self.node.start(entry, args, env, None).await {
                    Ok(()) => IpcResponse::ok(id, serde_json::Value::Null),
                    Err(e) => IpcResponse::err(id, "NODE_START_FAILED", e),
                }
            }
            ("node", "stop") => match self.node.stop().await {
                Ok(()) => IpcResponse::ok(id, serde_json::Value::Null),
                Err(e) => IpcResponse::err(id, "NODE_STOP_FAILED", e),
            },
            ("node", "restart") => {
                let entry = get_str("entry");
                let args: Option<Vec<String>> = p.get("args").and_then(|v| v.as_array()).map(|a| {
                    a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()
                });
                let env: Option<HashMap<String, String>> =
                    p.get("env").and_then(|v| v.as_object()).map(|o| {
                        o.iter()
                            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                            .collect()
                    });
                match self.node.restart(entry, args, env).await {
                    Ok(()) => IpcResponse::ok(id, serde_json::Value::Null),
                    Err(e) => IpcResponse::err(id, "NODE_START_FAILED", e),
                }
            }
            ("node", "status") => {
                let (s, pid) = self.node.status().await;
                IpcResponse::ok(id, serde_json::json!({ "status": format!("{s:?}").to_lowercase(), "pid": pid }))
            }
            ("backend", "invoke") => {
                let procedure = get_str("procedure").unwrap_or_default();
                let params = p.get("params").cloned().unwrap_or(serde_json::Value::Null);
                // Rust-side ceiling; the TS wrapper applies its own timeout first.
                match self.node.invoke(procedure.clone(), params, 60_000).await {
                    Ok(result) => IpcResponse::ok(id, result),
                    Err((code, message)) => IpcResponse::err(id, &code, message),
                }
            }
            // ---- tray (§17) ----
            ("tray", "create") => {
                let icon = get_str("icon").unwrap_or_default();
                let tooltip = get_str("tooltip");
                match self.trays.create(icon, tooltip) {
                    Ok(entry) => {
                        crate::os_events::request_os_bind();
                        IpcResponse::ok(
                            id,
                            serde_json::json!({ "id": entry.id }),
                        )
                    }
                    Err(e) => IpcResponse::err(id, "TRAY_CREATION_FAILED", e.to_string()),
                }
            }
            ("tray", "destroy") => match get_str("id") {
                Some(tid) => match self.trays.destroy(&tid) {
                    Ok(()) => IpcResponse::ok(id, serde_json::Value::Null),
                    Err(e) => IpcResponse::err(id, "TRAY_CREATION_FAILED", e.to_string()),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "tray.destroy: missing id"),
            },
            ("tray", "list") => {
                let list: Vec<serde_json::Value> = self
                    .trays
                    .list()
                    .into_iter()
                    .map(|e| serde_json::json!({ "id": e.id, "tooltip": e.tooltip }))
                    .collect();
                IpcResponse::ok(id, serde_json::to_value(list).unwrap())
            }
            ("tray", "click") => match get_str("id") {
                Some(tid) => {
                    if self.trays.get(&tid).is_none() {
                        IpcResponse::err(id, "TRAY_CREATION_FAILED", format!("tray not found: {tid}"))
                    } else {
                        self.app.emit("tray.clicked", serde_json::json!({ "id": tid }));
                        IpcResponse::ok(id, serde_json::Value::Null)
                    }
                }
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "tray.click: missing id"),
            },
            // ---- menus (§18) ----
            ("menu", "create") => {
                let items: Result<Vec<MenuItem>, _> = p
                    .get("items")
                    .cloned()
                    .map(serde_json::from_value)
                    .unwrap_or(Ok(vec![]));
                match items {
                    Ok(items) => match self.menus.create(items) {
                        Ok(menu_id) => {
                            crate::os_events::request_os_bind();
                            IpcResponse::ok(id, serde_json::json!({ "id": menu_id }))
                        }
                        Err(e) => IpcResponse::err(id, "MENU_CREATION_FAILED", e.to_string()),
                    },
                    Err(e) => IpcResponse::err(id, "KITRON_INVALID_PARAMS", format!("menu.create: bad items: {e}")),
                }
            }
            ("menu", "setAppMenu") => match get_str("id") {
                Some(mid) => match self.menus.set_app_menu(&mid) {
                    Ok(()) => {
                        crate::os_events::request_os_bind();
                        IpcResponse::ok(id, serde_json::Value::Null)
                    }
                    Err(e) => IpcResponse::err(id, "MENU_CREATION_FAILED", e.to_string()),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "menu.setAppMenu: missing id"),
            },
            ("menu", "click") => {
                let mid = get_str("id").or_else(|| get_str("menuId")).unwrap_or_default();
                let action = get_str("action").unwrap_or_default();
                if mid.is_empty() || action.is_empty() {
                    IpcResponse::err(id, "KITRON_INVALID_PARAMS", "menu.click: need id and action")
                } else if !self.menus.has_action(&mid, &action) {
                    IpcResponse::err(id, "MENU_CREATION_FAILED", format!("unknown action {action} for menu {mid}"))
                } else {
                    self.app.emit("menu.action", serde_json::json!({ "menuId": mid, "action": action }));
                    IpcResponse::ok(id, serde_json::Value::Null)
                }
            }
            // ---- shortcuts (§19) ----
            ("shortcut", "register") => match get_str("accelerator") {
                Some(acc) => match self.shortcuts.register(&acc) {
                    Ok(key) => {
                        crate::os_events::request_os_bind();
                        // Canonical key so JS `shortcut.CMD+K` events match.
                        IpcResponse::ok(id, serde_json::json!({ "accelerator": key }))
                    }
                    Err(e) => IpcResponse::err(id, "SHORTCUT_REGISTER_FAILED", e.to_string()),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "shortcut.register: need accelerator"),
            },
            ("shortcut", "unregister") => match get_str("accelerator") {
                Some(acc) => match self.shortcuts.unregister(&acc) {
                    Ok(()) => IpcResponse::ok(id, serde_json::Value::Null),
                    Err(e) => IpcResponse::err(id, "SHORTCUT_REGISTER_FAILED", e.to_string()),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "shortcut.unregister: need accelerator"),
            },
            ("shortcut", "list") => {
                IpcResponse::ok(id, serde_json::to_value(self.shortcuts.list()).unwrap())
            }
            ("shortcut", "press") => match get_str("accelerator") {
                Some(acc) => match crate::shortcuts::normalize(&acc) {
                    Ok(key) => {
                        if self.shortcuts.is_registered(&key) {
                            self.app.emit(&format!("shortcut.{key}"), serde_json::Value::Null);
                            IpcResponse::ok(id, serde_json::Value::Null)
                        } else {
                            IpcResponse::err(id, "SHORTCUT_REGISTER_FAILED", format!("not registered: {key}"))
                        }
                    }
                    Err(e) => IpcResponse::err(id, "KITRON_INVALID_PARAMS", e.to_string()),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "shortcut.press: need accelerator"),
            },
            // ---- updater (§22) ----
            ("updater", "check") => match get_str("feedUrl").or_else(|| get_str("feed_url")) {
                Some(url) => match crate::updater::check(&url).await {
                    Ok(info) => IpcResponse::ok(id, serde_json::to_value(info).unwrap()),
                    Err(e) => IpcResponse::err(id, "UPDATE_CHECK_FAILED", e),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "updater.check: need feedUrl"),
            },
            ("updater", "verify") => {
                let payload = p.get("payload").and_then(|v| v.as_str()).unwrap_or("");
                let signature = p.get("signature").and_then(|v| v.as_str()).unwrap_or("");
                let pubkey = p.get("publicKey").and_then(|v| v.as_str()).or_else(|| p.get("pubkey").and_then(|v| v.as_str())).unwrap_or("");
                match crate::updater::verify_signature(payload.as_bytes(), signature, pubkey) {
                    Ok(()) => IpcResponse::ok(id, serde_json::json!({ "valid": true })),
                    Err(e) => IpcResponse::err(id, "UPDATE_SIGNATURE_INVALID", e),
                }
            }
            ("updater", "installAndRestart") => {
                match self.updater_state.take_for_restart().await {
                    Some(staged) => {
                        // Swap already happened at `install` time. Spawn the
                        // new image and exit — the OS delivers the restart.
                        // NOTE: never covered by tests (it exits the process).
                        let exe = std::env::current_exe()
                            .map_err(|e| format!("UPDATE_CHECK_FAILED: {e}"));
                        match exe {
                            Ok(exe) => {
                                match std::process::Command::new(exe).spawn() {
                                    Ok(_) => {
                                        tracing::info!(version = ?staged.version, "update installed, restarting");
                                        std::process::exit(0);
                                    }
                                    Err(e) => IpcResponse::err(id, "UPDATE_CHECK_FAILED", format!("restart failed: {e}")),
                                }
                            }
                            Err(e) => IpcResponse::err(id, "UPDATE_CHECK_FAILED", e),
                        }
                    }
                    None => IpcResponse::err(
                        id,
                        "UPDATE_NOT_STAGED",
                        "nothing installed: run updater.download + updater.install first",
                    ),
                }
            }
            ("updater", "download") => {
                let url = get_str("url").or_else(|| get_str("feedUrl")).unwrap_or_default();
                let version = get_str("version").unwrap_or_else(|| "unknown".into());
                if url.is_empty() {
                    IpcResponse::err(id, "KITRON_INVALID_PARAMS", "updater.download: need url")
                } else {
                    match crate::updater::download(&url, &version).await {
                        Ok(path) => {
                            self.updater_state.set_downloaded(path.clone(), version.clone()).await;
                            IpcResponse::ok(id, serde_json::json!({ "path": path, "version": version }))
                        }
                        Err(e) => IpcResponse::err(id, "UPDATE_DOWNLOAD_FAILED", e),
                    }
                }
            }
            ("updater", "install") => {
                let sig = get_str("signature").unwrap_or_default();
                let key = get_str("publicKey").or_else(|| get_str("pubkey")).unwrap_or_default();
                let path = get_str("path").map(std::path::PathBuf::from);
                let staged = self.updater_state.staged_path().await;
                let target = path.or(staged);
                match target {
                    Some(target) if !sig.is_empty() && !key.is_empty() => {
                        match crate::updater::install(&target, &sig, &key).await {
                            Ok(backup) => {
                                self.updater_state.set_verified(sig, key, backup.clone()).await;
                                IpcResponse::ok(id, serde_json::json!({ "backup": backup }))
                            }
                            Err(e) => {
                                // Signature failures keep their code; IO errors surface raw.
                                if e == "UPDATE_SIGNATURE_INVALID" || e.starts_with("UPDATE_SIGNATURE_INVALID") {
                                    IpcResponse::err(id, "UPDATE_SIGNATURE_INVALID", e)
                                } else {
                                    IpcResponse::err(id, "UPDATE_CHECK_FAILED", e)
                                }
                            }
                        }
                    }
                    Some(_) => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "updater.install: need signature + publicKey"),
                    None => IpcResponse::err(id, "UPDATE_NOT_STAGED", "nothing downloaded: run updater.download first"),
                }
            }
            // ---- own UI renderer (no RN, no webview) ----
            ("ui", "create") => {
                let nid = get_str("id").unwrap_or_default();
                let kind: Result<crate::ui::UiKind, _> =
                    serde_json::from_value(p.get("kind").cloned().unwrap_or(serde_json::Value::Null));
                let props: crate::ui::UiProps =
                    serde_json::from_value(p.get("props").cloned().unwrap_or(serde_json::json!({}))).unwrap_or_default();
                let style: crate::ui::UiStyle =
                    serde_json::from_value(p.get("style").cloned().unwrap_or(serde_json::json!({}))).unwrap_or_default();
                match (nid.is_empty(), kind) {
                    (false, Ok(kind)) => match self.ui.create(nid.clone(), kind, props, style) {
                        Ok(()) => {
                            // Images load async (file/http); completion repaints.
                            if matches!(kind, crate::ui::UiKind::Image) {
                                let ui = self.ui.clone();
                                tokio::spawn(async move {
                                    ui.load_image(nid).await;
                                });
                            }
                            self.windows.redraw_all();
                            IpcResponse::ok(id, serde_json::Value::Null)
                        }
                        Err(e) => IpcResponse::err(id, ui_code(&e), e.to_string()),
                    },
                    _ => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "ui.create: need id + kind(view|text|button|input|image)"),
                }
            }
            ("ui", "setProps") => match get_str("id") {
                Some(nid) => {
                    // Partial merge: text-only refreshes must not wipe style-borne props.
                    let patch = p.get("props").cloned().unwrap_or(serde_json::json!({}));
                    match self.ui.patch_props(&nid, &patch) {
                        Ok(()) => {
                            self.windows.redraw_all();
                            IpcResponse::ok(id, serde_json::Value::Null)
                        }
                        Err(e) => IpcResponse::err(id, ui_code(&e), e.to_string()),
                    }
                }
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "ui.setProps: need id"),
            },
            ("ui", "setStyle") => match get_str("id") {
                Some(nid) => {
                    let patch = p.get("style").cloned().unwrap_or(serde_json::json!({}));
                    match self.ui.patch_style(&nid, &patch) {
                        Ok(()) => {
                            self.windows.redraw_all();
                            IpcResponse::ok(id, serde_json::Value::Null)
                        }
                        Err(e) => IpcResponse::err(id, ui_code(&e), e.to_string()),
                    }
                }
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "ui.setStyle: need id"),
            },
            ("ui", "append") => {
                let parent = get_str("parent").unwrap_or_default();
                let child = get_str("child").unwrap_or_default();
                let index = p.get("index").and_then(|v| v.as_u64()).map(|v| v as usize);
                if parent.is_empty() || child.is_empty() {
                    IpcResponse::err(id, "KITRON_INVALID_PARAMS", "ui.append: need parent + child")
                } else {
                    match self.ui.append(&parent, &child, index) {
                        Ok(()) => {
                            self.windows.redraw_all();
                            IpcResponse::ok(id, serde_json::Value::Null)
                        }
                        Err(e) => IpcResponse::err(id, ui_code(&e), e.to_string()),
                    }
                }
            }
            ("ui", "remove") => match get_str("id") {
                Some(nid) => match self.ui.remove(&nid) {
                    Ok(()) => {
                        self.windows.redraw_all();
                        IpcResponse::ok(id, serde_json::Value::Null)
                    }
                    Err(e) => IpcResponse::err(id, ui_code(&e), e.to_string()),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "ui.remove: need id"),
            },
            ("ui", "setRoot") => {
                let win = get_str("window").unwrap_or_default();
                let root = get_str("root").unwrap_or_default();
                if win.is_empty() || root.is_empty() {
                    IpcResponse::err(id, "KITRON_INVALID_PARAMS", "ui.setRoot: need window + root")
                } else {
                    match self.ui.set_root(&win, &root) {
                        Ok(()) => {
                            self.windows.redraw(&win);
                            IpcResponse::ok(id, serde_json::Value::Null)
                        }
                        Err(e) => IpcResponse::err(id, ui_code(&e), e.to_string()),
                    }
                }
            }
            ("ui", "clearRoot") => {
                let win = get_str("window").unwrap_or_default();
                if win.is_empty() {
                    IpcResponse::err(id, "KITRON_INVALID_PARAMS", "ui.clearRoot: need window")
                } else {
                    self.ui.clear_root(&win);
                    self.windows.redraw(&win);
                    IpcResponse::ok(id, serde_json::Value::Null)
                }
            }
            ("ui", "layout") => {
                let win = get_str("window").unwrap_or_default();
                let w = p.get("width").and_then(|v| v.as_f64()).unwrap_or(800.0) as f32;
                let h = p.get("height").and_then(|v| v.as_f64()).unwrap_or(600.0) as f32;
                if win.is_empty() {
                    IpcResponse::err(id, "KITRON_INVALID_PARAMS", "ui.layout: need window")
                } else {
                    match self.ui.layout(&win, w, h) {
                        Ok(placed) => IpcResponse::ok(id, serde_json::to_value(placed).unwrap()),
                        Err(e) => IpcResponse::err(id, ui_code(&e), e.to_string()),
                    }
                }
            }
            ("ui", "scroll") => {
                // Test/OS hook: wheel delta at window coordinates.
                let win = get_str("window").unwrap_or_default();
                let (x, y) = (
                    p.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    p.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                );
                let (dx, dy) = (
                    p.get("dx").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    p.get("dy").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                );
                let (w, h) = (
                    p.get("width").and_then(|v| v.as_f64()).unwrap_or(800.0) as f32,
                    p.get("height").and_then(|v| v.as_f64()).unwrap_or(600.0) as f32,
                );
                match self.ui.scroll_by(&win, x, y, dx, dy, w, h) {
                    Ok(Some((node, (sx, sy)))) => {
                        self.app.emit("ui.scroll", serde_json::json!({ "windowId": win, "nodeId": node, "x": sx, "y": sy }));
                        self.windows.redraw(&win);
                        IpcResponse::ok(id, serde_json::json!({ "nodeId": node, "x": sx, "y": sy }))
                    }
                    Ok(None) => IpcResponse::ok(id, serde_json::Value::Null),
                    Err(e) => IpcResponse::err(id, ui_code(&e), e.to_string()),
                }
            }
            ("ui", "press") => {                // Test/OS hook: press at window coordinates (the event loop
                // calls the same path for real mouse input).
                let win = get_str("window").unwrap_or_default();
                let (x, y) = (
                    p.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                    p.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32,
                );
                let (w, h) = (
                    p.get("width").and_then(|v| v.as_f64()).unwrap_or(800.0) as f32,
                    p.get("height").and_then(|v| v.as_f64()).unwrap_or(600.0) as f32,
                );
                match self.ui.press_at(&win, x, y, w, h) {
                    Ok(Some(btn)) => {
                        self.app.emit("ui.press", serde_json::json!({ "windowId": win, "nodeId": btn }));
                        self.windows.redraw(&win);
                        IpcResponse::ok(id, serde_json::json!({ "nodeId": btn }))
                    }
                    Ok(None) => {
                        self.windows.redraw(&win);
                        IpcResponse::ok(id, serde_json::Value::Null)
                    }
                    Err(e) => IpcResponse::err(id, ui_code(&e), e.to_string()),
                }
            }
            // ---- crash reports ----
            ("crash", "list") => match self.crashes.list() {
                Ok(reports) => IpcResponse::ok(id, serde_json::to_value(reports).unwrap()),
                Err(e) => IpcResponse::err(id, "CRASH_NOT_FOUND", e),
            },
            ("crash", "get") => match get_str("id") {
                Some(cid) => match self.crashes.get(&cid) {
                    Ok(r) => IpcResponse::ok(id, serde_json::to_value(r).unwrap()),
                    Err(e) => IpcResponse::err(id, "CRASH_NOT_FOUND", e),
                },
                None => IpcResponse::err(id, "KITRON_INVALID_PARAMS", "crash.get: missing id"),
            },
            ("crash", "clear") => match self.crashes.clear() {
                Ok(n) => IpcResponse::ok(id, serde_json::json!({ "cleared": n })),
                Err(e) => IpcResponse::err(id, "CRASH_NOT_FOUND", e),
            },
            _ => IpcResponse::err(
                id,
                "KITRON_NOT_IMPLEMENTED",
                format!("unknown {}.{}", req.domain, req.method),
            ),
        }
    }
}

fn ui_code(e: &crate::ui::UiError) -> &'static str {
    match e {
        crate::ui::UiError::NotFound(_) | crate::ui::UiError::NoRoot(_) => "UI_NOT_FOUND",
        crate::ui::UiError::Invalid(_) => "KITRON_INVALID_PARAMS",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitron_core::IpcRequest;

    fn req(domain: &str, method: &str, params: serde_json::Value) -> IpcRequest {
        IpcRequest { id: 1, domain: domain.into(), method: method.into(), params }
    }

    fn runtime() -> Runtime {
        let (app, _rx) = AppManager::new();
        Runtime::new(Arc::new(app))
    }

    fn runtime_with(grants: &[&str]) -> Runtime {
        let (app, _rx) = AppManager::new();
        Runtime::new_with_permissions(
            Arc::new(app),
            kitron_core::permissions::Permissions::new(grants.iter().map(|s| s.to_string())),
        )
    }

    fn ok_value(r: IpcResponse) -> serde_json::Value {
        match r {
            IpcResponse::Ok { result, .. } => result,
            IpcResponse::Err { error, .. } => panic!("expected ok, got {}", error.code),
        }
    }

    fn err_code(r: IpcResponse) -> String {
        match r {
            IpcResponse::Err { error, .. } => error.code,
            _ => panic!("expected err"),
        }
    }

    #[tokio::test]
    async fn window_list_and_unknown_window() {
        let rt = runtime();
        let v = ok_value(rt.dispatch(req("window", "list", serde_json::json!({}))).await);
        assert_eq!(v, serde_json::json!([]));
        assert_eq!(
            err_code(rt.dispatch(req("window", "get", serde_json::json!({ "id": "nope" }))).await),
            "WINDOW_NOT_FOUND"
        );
    }

    #[tokio::test]
    async fn tray_create_click_destroy() {
        let rt = runtime();
        let mut rx = rt.app.subscribe();
        let v = ok_value(
            rt.dispatch(req("tray", "create", serde_json::json!({ "icon": "./icon.png" }))).await,
        );
        let id = v["id"].as_str().unwrap().to_string();
        rt.dispatch(req("tray", "click", serde_json::json!({ "id": id }))).await;
        let (name, payload) = rx.recv().await.unwrap();
        assert_eq!(name, "tray.clicked");
        assert_eq!(payload["id"], id);
        assert_eq!(
            err_code(rt.dispatch(req("tray", "click", serde_json::json!({ "id": "nope" }))).await),
            "TRAY_CREATION_FAILED"
        );
    }

    #[tokio::test]
    async fn menu_create_click_validation() {
        let rt = runtime();
        let mut rx = rt.app.subscribe();
        let v = ok_value(
            rt.dispatch(
                req("menu", "create", serde_json::json!({ "items": [{ "label": "File", "items": [{ "label": "New", "action": "new-file" }] }] })),
            )
            .await,
        );
        let id = v["id"].as_str().unwrap().to_string();
        // Unknown action rejected.
        assert_eq!(
            err_code(
                rt.dispatch(req("menu", "click", serde_json::json!({ "id": id, "action": "nope" }))).await
            ),
            "MENU_CREATION_FAILED"
        );
        rt.dispatch(req("menu", "click", serde_json::json!({ "id": id, "action": "new-file" }))).await;
        let (name, payload) = rx.recv().await.unwrap();
        assert_eq!(name, "menu.action");
        assert_eq!(payload["action"], "new-file");
        // Empty menu rejected.
        assert_eq!(
            err_code(rt.dispatch(req("menu", "create", serde_json::json!({ "items": [] }))).await),
            "MENU_CREATION_FAILED"
        );
    }

    #[tokio::test]
    async fn shortcut_register_press_unregister() {
        let rt = runtime();
        let mut rx = rt.app.subscribe();
        ok_value(
            rt.dispatch(req("shortcut", "register", serde_json::json!({ "accelerator": "Ctrl+K" }))).await,
        );
        // Duplicate rejected.
        assert_eq!(
            err_code(
                rt.dispatch(req("shortcut", "register", serde_json::json!({ "accelerator": "Ctrl+K" }))).await
            ),
            "SHORTCUT_REGISTER_FAILED"
        );
        rt.dispatch(req("shortcut", "press", serde_json::json!({ "accelerator": "Ctrl+K" }))).await;
        let (name, _) = rx.recv().await.unwrap();
        assert_eq!(name, "shortcut.Ctrl+K");
        ok_value(
            rt.dispatch(req("shortcut", "unregister", serde_json::json!({ "accelerator": "Ctrl+K" }))).await,
        );
        assert_eq!(
            err_code(
                rt.dispatch(req("shortcut", "press", serde_json::json!({ "accelerator": "Ctrl+K" }))).await
            ),
            "SHORTCUT_REGISTER_FAILED"
        );
    }

    #[tokio::test]
    async fn backend_invoke_without_sidecar() {
        let rt = runtime();
        assert_eq!(
            err_code(
                rt.dispatch(
                    req("backend", "invoke", serde_json::json!({ "procedure": "x", "params": {} }))
                )
                .await
            ),
            "BACKEND_INVOKE_FAILED"
        );
        // `node` spawn succeeds even for a missing entry (the child fails
        // later), so restart reports ok and status tracks the child.
        ok_value(rt.dispatch(req("node", "restart", serde_json::json!({ "entry": "__nope__.js" }))).await);
        let v = ok_value(rt.dispatch(req("node", "status", serde_json::json!({}))).await);
        assert_eq!(v["status"], "running");
        ok_value(rt.dispatch(req("node", "stop", serde_json::json!({}))).await);
    }

    #[tokio::test]
    async fn updater_verify_rejects_bad_sig() {
        let rt = runtime();
        assert_eq!(
            err_code(
                rt.dispatch(
                    req("updater", "verify", serde_json::json!({ "payload": "x", "signature": "AA", "publicKey": "00" }))
                )
                .await
            ),
            "UPDATE_SIGNATURE_INVALID"
        );
        assert_eq!(
            err_code(rt.dispatch(req("updater", "installAndRestart", serde_json::json!({}))).await),
            "UPDATE_NOT_STAGED"
        );
        // Install without credentials is a param error, not a signature error.
        assert_eq!(
            err_code(rt.dispatch(req("updater", "install", serde_json::json!({}))).await),
            "UPDATE_NOT_STAGED"
        );
    }

    #[tokio::test]
    async fn app_deeplink_fileopen_emit_events() {
        let rt = runtime();
        let mut rx = rt.app.subscribe();
        ok_value(
            rt.dispatch(req("app", "deepLink", serde_json::json!({ "url": "myapp://open/1" }))).await,
        );
        let (name, payload) = rx.recv().await.unwrap();
        assert_eq!(name, "app.deepLink");
        assert_eq!(payload["url"], "myapp://open/1");
        ok_value(rt.dispatch(req("app", "fileOpen", serde_json::json!({ "path": "/tmp/a.md" }))).await);
        let (name, _) = rx.recv().await.unwrap();
        assert_eq!(name, "app.fileOpen");
    }

    #[tokio::test]
    async fn crash_store_dispatch() {
        let rt = runtime();
        // list returns an array (contents depend on parallel tests sharing
        // the on-disk store — assert shape, not contents).
        let v = ok_value(rt.dispatch(req("crash", "list", serde_json::json!({}))).await);
        assert!(v.is_array());
        assert_eq!(
            err_code(rt.dispatch(req("crash", "get", serde_json::json!({ "id": "kitron-test-nope" }))).await),
            "CRASH_NOT_FOUND"
        );
        assert_eq!(
            err_code(rt.dispatch(req("crash", "get", serde_json::json!({ "id": "../evil" }))).await),
            "CRASH_NOT_FOUND"
        );
    }

    #[tokio::test]
    async fn unknown_domain_is_typed_not_implemented() {
        let rt = runtime();
        assert_eq!(err_code(rt.dispatch(req("nope", "nope", serde_json::json!({}))).await), "KITRON_NOT_IMPLEMENTED");
    }

    #[tokio::test]
    async fn sensitive_domains_require_grants() {
        let rt = runtime();
        // Secure default: shell + process denied.
        assert_eq!(
            err_code(rt.dispatch(req("shell", "openExternal", serde_json::json!({ "url": "https://x.com" }))).await),
            "PERMISSION_DENIED"
        );
        assert_eq!(
            err_code(rt.dispatch(req("process", "spawn", serde_json::json!({ "command": "true" }))).await),
            "PERMISSION_DENIED"
        );
        // Granted: request reaches the handler (bad scheme → handler error,
        // proving it passed the permission gate).
        let rt = runtime_with(&["shell"]);
        assert_eq!(
            err_code(rt.dispatch(req("shell", "openExternal", serde_json::json!({ "url": "notaurl" }))).await),
            "SHELL_OPEN_FAILED"
        );
        // Other sensitive domain still denied.
        assert_eq!(
            err_code(rt.dispatch(req("process", "spawn", serde_json::json!({ "command": "true" }))).await),
            "PERMISSION_DENIED"
        );
    }
}
