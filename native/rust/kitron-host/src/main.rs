//! `kitron-host` binary — stdio NDJSON bridge + native window.
//!
//! ```text
//! kitron-host --url http://127.0.0.1:5173 --title "My App" --width 1200 --height 800 [--devtools]
//! ```
//!
//! Node.js (or the CLI) spawns this process and speaks NDJSON over stdin/stdout.
//! Without `--features native-webview` the host runs headless (IPC fully
//! functional — used for tests and web-only dev). With the feature, a real
//! OS WebView opens (WKWebView / WebView2 / WebKitGTK via wry): renderer
//! JS reaches the hub through the injected `window.kitron` bridge, and
//! `window.*` IPC channels drive the actual native window.

#[cfg(not(feature = "native-webview"))]
use kitron_host::ipc::{encode_frame, split_lines, IpcRequest};
#[cfg(not(feature = "native-webview"))]
use kitron_host::{platform, Host};

fn arg_value(args: &[String], name: &str, default: &str) -> String {
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().cloned().unwrap_or_else(|| default.to_string());
        }
        if let Some(v) = a.strip_prefix(&format!("{name}=")) {
            return v.to_string();
        }
    }
    default.to_string()
}

#[cfg(feature = "native-webview")]
mod ui {
    use std::io::{BufRead, Write};

    use kitron_host::ipc::{encode_frame, IpcRequest};
    use kitron_host::Host;
    use serde_json::Value;
    use winit::dpi::LogicalSize;
    use winit::event::{Event, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoop};
    use winit::window::WindowBuilder;

    #[derive(Debug)]
    enum UserEvent {
        StdinLine(String),
        SweepEvals,
    }

    /// Renderer bridge — mirrors `packages/native-api/src/preload.ts` (§20).
    /// Transport is `window.rpc.notify('kitron-ipc', line)` (wry injects the
    /// `window.rpc` shim when a handler is registered); the Rust RPC handler
    /// below unwraps it back to a raw NDJSON line for the hub.
    const BRIDGE_JS: &str = r#"
(() => {
  "use strict";
  const ALLOW = [/^ping$/, /^app\.(getName|getVersion|getPath|getInfo|isReady|ready|requestSingleInstanceLock|setProtocolClient|getProtocolURLs)$/, /^window\.[a-zA-Z]+$/, /^webcontents\.[a-zA-Z]+$/, /^shell\.(openExternal|openPath|showItemInFolder|trashItem|beep)$/, /^theme\.(getInfo|setSource)$/, /^power\.(isOnBattery|getIdleState|getIdleTime)$/, /^safe\.(encrypt|decrypt|available)$/, /^updater\.(check|download|install|setFeed|checkFeed)$/, /^native\.(notification|dialog|menu|tray|clipboard|screen|desktop|shortcut|power|window)\.[a-zA-Z]+$/, /^plugin\.[a-z0-9-]+\.[a-zA-Z0-9_-]+$/, /^log$/, /^(second-instance|open-url|theme\.updated)$/];
  const allowed = (ch) => ALLOW.some((re) => re.test(ch));
  function install(post) {
    if (window.kitron) return;
    const pending = new Map();
    const listeners = new Map();
    let seq = 0;
    window.__kitronDispatch = (text) => {
      let msg;
      try { msg = JSON.parse(text); } catch { return; }
      if (msg && typeof msg.id === "string" && typeof msg.success === "boolean") {
        const entry = pending.get(msg.id);
        if (!entry) return;
        pending.delete(msg.id);
        clearTimeout(entry.timer);
        if (msg.success) entry.resolve(msg.payload);
        else entry.reject(Object.assign(new Error((msg.error && msg.error.message) || "IPC failed"), { code: msg.error && msg.error.code }));
        return;
      }
      if (msg && typeof msg.channel === "string" && !("id" in msg)) {
        const set = listeners.get(msg.channel);
        if (set) for (const fn of [...set]) { try { fn(msg.payload); } catch {} }
      }
    };
    function invoke(channel, payload, opts) {
      if (!allowed(channel)) return Promise.reject(Object.assign(new Error('Channel "' + channel + '" is not allowed'), { code: "PERMISSION_DENIED" }));
      const id = "r-" + (++seq) + "-" + Date.now();
      const timeout = (opts && opts.timeout) || 30000;
      return new Promise((resolve, reject) => {
        const timer = setTimeout(() => { pending.delete(id); reject(Object.assign(new Error('Invoke "' + channel + '" timed out'), { code: "IPC_TIMEOUT" })); }, timeout);
        pending.set(id, { resolve, reject, timer });
        try { post(JSON.stringify({ id, channel, payload: payload ?? null })); }
        catch (e) { pending.delete(id); clearTimeout(timer); reject(e); }
      });
    }
    function emit(channel, payload) {
      if (!allowed(channel)) throw Object.assign(new Error('Channel "' + channel + '" is not allowed'), { code: "PERMISSION_DENIED" });
      post(JSON.stringify({ channel, payload: payload ?? null }));
    }
    function subscribe(channel, listener) {
      if (!allowed(channel)) throw Object.assign(new Error('Channel "' + channel + '" is not allowed'), { code: "PERMISSION_DENIED" });
      let set = listeners.get(channel);
      if (!set) { set = new Set(); listeners.set(channel, set); }
      set.add(listener);
      return () => { set.delete(listener); if (set.size === 0) listeners.delete(channel); };
    }
    const native = {
      notification: { show: (o) => invoke("native.notification.show", o) },
      window: {
        setAlwaysOnTop: (flag) => invoke("native.window.setAlwaysOnTop", { flag }),
        setTitle: (title) => invoke("native.window.setTitle", { title }),
      },
      dialog: {
        open: (o) => invoke("native.dialog.open", o ?? {}),
        save: (o) => invoke("native.dialog.save", o ?? {}),
      },
      clipboard: {
        readText: () => invoke("native.clipboard.readText"),
        writeText: (text) => invoke("native.clipboard.writeText", { text }),
      },
    };
    Object.defineProperty(window, "kitron", { value: Object.freeze({ invoke, emit, subscribe, native }), writable: false, configurable: false });
  }
  const timer = setInterval(() => {
    if (window.rpc && typeof window.rpc.notify === "function") {
      clearInterval(timer);
      install((text) => window.rpc.notify("kitron-ipc", text));
    }
  }, 20);
})();
"#;

    fn write_stdout(text: &str) {
        let mut out = std::io::stdout();
        let _ = out.write_all(text.as_bytes());
        let _ = out.flush();
    }

    /// Best-effort "Inspect Element": enables WebKit developer extras for
    /// this machine (macOS global default — dev only, no programmatic open
    /// in wry 0.6; right-click → Inspect Element, or Safari → Develop menu).
    fn enable_developer_extras() {
        #[cfg(target_os = "macos")]
        {
            match std::process::Command::new("defaults")
                .args([
                    "write",
                    "NSGlobalDomain",
                    "WebKitDeveloperExtras",
                    "-bool",
                    "true",
                ])
                .output()
            {
                Ok(o) if o.status.success() => {
                    eprintln!("[Kitron][Rust] WebKitDeveloperExtras enabled (right-click → Inspect Element)")
                }
                _ => eprintln!("[Kitron][Rust] Could not enable WebKitDeveloperExtras"),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            eprintln!("[Kitron][Rust] DevTools: use the platform inspector manually");
        }
    }

    /// Renderer → hub: unwrap `window.rpc.notify('kitron-ipc', line)` back
    /// to the raw NDJSON line. Anything else is ignored.
    fn handle_renderer_message(req: &wry::RpcRequest) {
        if req.method != "kitron-ipc" {
            return;
        }
        if let Some(line) = req
            .params
            .as_ref()
            .and_then(|v| v.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
        {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                write_stdout(trimmed);
                write_stdout("\n");
            }
        }
    }

    fn dispatch_to_js(webview: &mut wry::webview::WebView, json_line: &str) {
        let arg = serde_json::to_string(json_line).unwrap_or_default();
        let _ = webview.dispatch_script(&format!("window.__kitronDispatch({arg})"));
        let _ = webview.evaluate_script();
    }

    pub fn run(url: String, title: String, width: u32, height: u32, devtools: bool) {
        use std::collections::HashMap;
        use std::sync::{Arc, Mutex};

        eprintln!(
            "[Kitron][Rust] Native window ({} via {}) — {title} {width}x{height} → {url}{}",
            kitron_host::platform::platform_name(),
            kitron_host::platform::webview_backend(),
            if devtools { " (devtools)" } else { "" }
        );

        let event_loop = EventLoop::<UserEvent>::with_user_event();
        let proxy = event_loop.create_proxy();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(l) => {
                        if proxy.send_event(UserEvent::StdinLine(l)).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        // Watchdog: evals on pages without the bridge (about:blank…)
        // would otherwise hang forever — expire them loudly.
        let sweep_proxy = event_loop.create_proxy();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            if sweep_proxy.send_event(UserEvent::SweepEvals).is_err() {
                break;
            }
        });

        // Eval round-trip: evalId → (hub request id, started at).
        let eval_pending: Arc<Mutex<HashMap<String, (String, std::time::Instant)>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let eval_pending_rpc = eval_pending.clone();
        let mk_rpc = move || -> wry::webview::RpcHandler {
            let pending = eval_pending_rpc.clone();
            Box::new(move |req: wry::RpcRequest| {
                if req.method == "kitron-ipc" {
                    handle_renderer_message(&req);
                } else if req.method == "kitron-eval-result" {
                    let first = req
                        .params
                        .as_ref()
                        .and_then(|v| v.as_array())
                        .and_then(|a| a.first())
                        .cloned();
                    if let Some(obj) = first.as_ref().and_then(|v| v.as_object()) {
                        let eid = obj
                            .get("evalId")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let ok = obj.get("ok").and_then(|v| v.as_bool()).unwrap_or(false);
                        let entry = pending
                            .lock()
                            .ok()
                            .and_then(|mut m| m.remove(&eid));
                        if let Some((ipc_id, _)) = entry {
                            let frame = if ok {
                                let value = obj
                                    .get("value")
                                    .cloned()
                                    .unwrap_or(serde_json::Value::Null);
                                serde_json::json!({
                                    "id": ipc_id, "success": true, "payload": value
                                })
                            } else {
                                let msg = obj
                                    .get("error")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("eval failed");
                                serde_json::json!({
                                    "id": ipc_id,
                                    "success": false,
                                    "error": {"code": "EVAL_FAILED", "message": msg},
                                })
                            };
                            write_stdout(&frame.to_string());
                            write_stdout("\n");
                        }
                    }
                }
                None
            })
        };

        let boot_title = title.clone();
        let boot_url = url.clone();
        let window = WindowBuilder::new()
            .with_title(&title)
            .with_inner_size(LogicalSize::new(width, height))
            .build(&event_loop)
            .expect("native window");

        let boot_view = wry::webview::WebViewBuilder::new(window)
            .expect("webview builder")
            .load_url(&url)
            .expect("valid url")
            .initialize_script(BRIDGE_JS)
            .set_rpc_handler(mk_rpc())
            .build()
            .expect("webview");

        if devtools {
            eprintln!("[Kitron][Rust] DevTools inspector is not programmatically available in this wry version — use the Web Inspector manually");
            enable_developer_extras();
        }

        let mut host = Host::new(title, "0.1.0".to_string());

        // Tranche C: real multi-window state (backend id → WebView).
        let mut main_view: Option<wry::webview::WebView> = Some(boot_view);
        let mut views: HashMap<String, wry::webview::WebView> = HashMap::new();
        let mut win_ids: HashMap<winit::window::WindowId, String> = HashMap::new();
        let mut meta: HashMap<String, ViewMeta> = HashMap::new();
        let mut eval_seq: u64 = 0;
        let mut last_cursor: (f64, f64) = (0.0, 0.0);

        event_loop.run(move |event, target, control_flow| {
            *control_flow = ControlFlow::Wait;
            // Pump scripts queued via `dispatch_script` (renderer dispatches).
            if let Some(v) = main_view.as_ref() {
                let _ = v.evaluate_script();
            }
            for v in views.values() {
                let _ = v.evaluate_script();
            }
            match event {
                Event::UserEvent(UserEvent::StdinLine(line)) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        return;
                    }
                    let value: Value = match serde_json::from_str(trimmed) {
                        Ok(v) => v,
                        Err(_) => return,
                    };
                    match serde_json::from_value::<IpcRequest>(value) {
                        Ok(req) => {
                            handle_host_request(
                                req,
                                target,
                                &mut host,
                                &mut main_view,
                                &mut views,
                                &mut win_ids,
                                &mut meta,
                                &eval_pending,
                                &mut eval_seq,
                                &boot_url,
                                &boot_title,
                                &mk_rpc,
                                last_cursor,
                            );
                        }
                        Err(_) => {
                            // Response to a renderer forward, or broadcast event.
                            if let Some(v) = main_view.as_mut() {
                                dispatch_to_js(v, trimmed);
                            }
                            for v in views.values_mut() {
                                dispatch_to_js(v, trimmed);
                            }
                        }
                    }
                }
                Event::UserEvent(UserEvent::SweepEvals) => {
                    // Expire evals whose page never answered (no bridge…).
                    let expired: Vec<(String, String)> = eval_pending
                        .lock()
                        .ok()
                        .map(|mut m| {
                            let now = std::time::Instant::now();
                            let old: Vec<String> = m
                                .iter()
                                .filter(|(_, (_, t))| {
                                    now.duration_since(*t)
                                        > std::time::Duration::from_secs(10)
                                })
                                .map(|(k, _)| k.clone())
                                .collect();
                            old.into_iter()
                                .filter_map(|k| m.remove(&k).map(|(id, _)| (k, id)))
                                .collect()
                        })
                        .unwrap_or_default();
                    for (_, ipc_id) in expired {
                        write_stdout(
                            &serde_json::json!({
                                "id": ipc_id,
                                "success": false,
                                "error": {"code": "EVAL_FAILED", "message": "eval timed out (page has no Kitron bridge?)"},
                            })
                            .to_string(),
                        );
                        write_stdout("\n");
                    }
                }
                Event::WindowEvent {
                    window_id,
                    event: win_event,
                    ..
                } => {
                    let bid = win_ids.get(&window_id).cloned();
                    match win_event {
                        WindowEvent::CloseRequested => {
                            if let Some(id) = bid {
                                emit_window_event(&id, "closed", serde_json::Value::Null);
                                views.remove(&id);
                                win_ids.remove(&window_id);
                            } else {
                                // Boot window closed before any binding.
                                main_view = None;
                            }
                            if views.is_empty() && main_view.is_none() {
                                write_stdout(
                                    "{\"channel\":\"app.quit-requested\",\"payload\":null}\n",
                                );
                                *control_flow = ControlFlow::Exit;
                            }
                        }
                        WindowEvent::Focused(true) => {
                            if let Some(id) = bid {
                                emit_window_event(&id, "focused", serde_json::Value::Null);
                            }
                        }
                        WindowEvent::Focused(false) => {
                            if let Some(id) = bid {
                                emit_window_event(&id, "blurred", serde_json::Value::Null);
                            }
                        }
                        WindowEvent::Moved(pos) => {
                            if let Some(id) = bid {
                                emit_window_event(
                                    &id,
                                    "moved",
                                    serde_json::json!({"x": pos.x, "y": pos.y}),
                                );
                            }
                        }
                        WindowEvent::Resized(size) => {
                            if let Some(id) = bid {
                                emit_window_event(
                                    &id,
                                    "resized",
                                    serde_json::json!({"width": size.width, "height": size.height}),
                                );
                            }
                        }
                        WindowEvent::CursorMoved { position, .. } => {
                            last_cursor = (position.x, position.y);
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        });
    }

    #[derive(Debug, Clone)]
    struct ViewMeta {
        url: String,
        title: String,
        zoom: f64,
    }

    fn emit_window_event(backend_id: &str, event: &str, payload: serde_json::Value) {
        write_stdout(&format!(
            "{{\"channel\":\"window.{backend_id}.{event}\",\"payload\":{payload}}}\n",
            payload = payload.to_string()
        ));
    }

    /// Route one hub request: webcontents intercepts, window chrome/nav,
    /// window creation (Tranche C), then the sync `Host` router.
    #[allow(clippy::too_many_arguments)]
    fn handle_host_request(
        req: IpcRequest,
        target: &winit::event_loop::EventLoopWindowTarget<UserEvent>,
        host: &mut Host,
        main_view: &mut Option<wry::webview::WebView>,
        views: &mut std::collections::HashMap<String, wry::webview::WebView>,
        win_ids: &mut std::collections::HashMap<winit::window::WindowId, String>,
        meta: &mut std::collections::HashMap<String, ViewMeta>,
        eval_pending: &std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, (String, std::time::Instant)>>>,
        eval_seq: &mut u64,
        boot_url: &str,
        boot_title: &str,
        mk_rpc: &dyn Fn() -> wry::webview::RpcHandler,
        last_cursor: (f64, f64),
    ) {
        use std::collections::HashMap;

        let payload = &req.payload;
        let id = payload.get("id").and_then(|v| v.as_str()).unwrap_or("");
        let known = !id.is_empty() && host.windows.get(id).is_some();

        // --- Tranche C: bind backend ids to real native windows. ---
        if req.channel == "window.create" && !id.is_empty() {
            if !known {
                if let Some(view) = main_view.take() {
                    let wid = view.window().id();
                    win_ids.insert(wid, id.to_string());
                    views.insert(id.to_string(), view);
                    meta.insert(
                        id.to_string(),
                        ViewMeta {
                            url: boot_url.to_string(),
                            title: boot_title.to_string(),
                            zoom: 1.0,
                        },
                    );
                    if let Some(t) = payload
                        .get("options")
                        .and_then(|o| o.get("title"))
                        .and_then(|v| v.as_str())
                    {
                        if let Some(v) = views.get(id) {
                            v.window().set_title(t);
                        }
                        if let Some(m) = meta.get_mut(id) {
                            m.title = t.to_string();
                        }
                    }
                } else {
                    let t = payload
                        .get("options")
                        .and_then(|o| o.get("title"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("Kitron");
                    let w = payload
                        .get("options")
                        .and_then(|o| o.get("width"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1200) as u32;
                    let h = payload
                        .get("options")
                        .and_then(|o| o.get("height"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(800) as u32;
                    let start_url = payload
                        .get("options")
                        .and_then(|o| o.get("url"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("about:blank");
                    match WindowBuilder::new()
                        .with_title(t)
                        .with_inner_size(LogicalSize::new(w, h))
                        .build(target)
                    {
                        Ok(window) => {
                            let wid = window.id();
                            match wry::webview::WebViewBuilder::new(window)
                                .and_then(|b| b.load_url(start_url))
                            {
                                Ok(builder) => match builder
                                    .initialize_script(BRIDGE_JS)
                                    .set_rpc_handler(mk_rpc())
                                    .build()
                                {
                                    Ok(view) => {
                                        win_ids.insert(wid, id.to_string());
                                        views.insert(id.to_string(), view);
                                        meta.insert(
                                            id.to_string(),
                                            ViewMeta {
                                                url: start_url.to_string(),
                                                title: t.to_string(),
                                                zoom: 1.0,
                                            },
                                        );
                                    }
                                    Err(e) => {
                                        eprintln!("[Kitron][Rust] WebView failed: {e}");
                                    }
                                },
                                Err(e) => {
                                    eprintln!("[Kitron][Rust] WebView failed: {e}");
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("[Kitron][Rust] Window failed: {e}");
                        }
                    }
                }
            }
            let res = host.handle(req);
            write_stdout(&encode_frame(&res));
            return;
        }

        // --- Tranche B: webcontents intercepts (need live-view answers). ---
        if req.channel.starts_with("webcontents.") {
            let view = views.get_mut(id);
            let response = match req.channel.as_str() {
                "webcontents.evalJS" => {
                    if let Some(v) = view {
                        *eval_seq += 1;
                        let eid = format!("e{eval_seq}");
                        if let Ok(mut m) = eval_pending.lock() {
                            m.insert(eid.clone(), (req.id.clone(), std::time::Instant::now()));
                        }
                        let code = payload
                            .get("code")
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let eid_js =
                            serde_json::to_string(&eid).unwrap_or_default();
                        let code_js =
                            serde_json::to_string(code).unwrap_or_default();
                        let js = format!(
                            "Promise.resolve().then(() => eval({code_js})).then(v => window.rpc.notify('kitron-eval-result', {{evalId: {eid_js}, ok: true, value: (v === undefined ? null : v)}})).catch(e => window.rpc.notify('kitron-eval-result', {{evalId: {eid_js}, ok: false, error: String((e && (e.stack || e.message)) || e)}}));"
                        );
                        let _ = v.dispatch_script(&js);
                        let _ = v.evaluate_script();
                        None // async: response arrives with the eval result.
                    } else {
                        Some(encode_frame(&kitron_host::ipc::IpcResponse::err(
                            req.id.clone(),
                            "WINDOW_FAILED",
                            format!("Unknown window \"{id}\""),
                        )))
                    }
                }
                "webcontents.getURL" => Some(encode_frame(&kitron_host::ipc::IpcResponse::ok(
                    req.id.clone(),
                    serde_json::json!(meta.get(id).map(|m| m.url.clone()).unwrap_or_default()),
                ))),
                "webcontents.getTitle" => {
                    Some(encode_frame(&kitron_host::ipc::IpcResponse::ok(
                        req.id.clone(),
                        serde_json::json!(meta.get(id).map(|m| m.title.clone()).unwrap_or_default()),
                    )))
                }
                "webcontents.getZoomFactor" => {
                    Some(encode_frame(&kitron_host::ipc::IpcResponse::ok(
                        req.id.clone(),
                        serde_json::json!(meta.get(id).map(|m| m.zoom).unwrap_or(1.0)),
                    )))
                }
                "webcontents.setZoomFactor" => {
                    let factor = payload
                        .get("factor")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(1.0);
                    if let Some(m) = meta.get_mut(id) {
                        m.zoom = factor;
                    }
                    if let Some(v) = view {
                        let _ = v.dispatch_script(&format!(
                            "document.documentElement.style.zoom = {factor};"
                        ));
                        let _ = v.evaluate_script();
                    }
                    Some(encode_frame(&kitron_host::ipc::IpcResponse::ok(
                        req.id.clone(),
                        serde_json::Value::Null,
                    )))
                }
                "webcontents.reload" => {
                    if let Some(v) = view {
                        let _ = v.dispatch_script("window.location.reload();");
                        let _ = v.evaluate_script();
                    }
                    Some(encode_frame(&kitron_host::ipc::IpcResponse::ok(
                        req.id.clone(),
                        serde_json::Value::Null,
                    )))
                }
                "webcontents.printToPDF" => {
                    match view {
                        None => Some(encode_frame(&kitron_host::ipc::IpcResponse::err(
                            req.id.clone(),
                            "WINDOW_FAILED",
                            format!("Unknown window \"{id}\""),
                        ))),
                        #[allow(unreachable_code)]
                        Some(v) => {
                            #[cfg(target_os = "macos")]
                            {
                                use winit::platform::macos::WindowExtMacOS as _;
                                let ptr =
                                    v.window().ns_window() as *mut std::ffi::c_void;
                                let ipc_id = req.id.clone();
                                match kitron_host::platform::print_webview_to_pdf(
                                    ptr,
                                    Box::new(move |r| {
                                        match r {
                                            Ok(bytes) => {
                                                use base64::Engine as _;
                                                let b64 = base64::engine::general_purpose::STANDARD
                                                    .encode(&bytes);
                                                write_stdout(
                                                    &serde_json::json!({
                                                        "id": ipc_id,
                                                        "success": true,
                                                        "payload": format!(
                                                            "data:application/pdf;base64,{b64}"
                                                        ),
                                                    })
                                                    .to_string(),
                                                );
                                                write_stdout("\n");
                                            }
                                            Err(message) => {
                                                write_stdout(
                                                    &serde_json::json!({
                                                        "id": ipc_id,
                                                        "success": false,
                                                        "error": {"code": "NATIVE_API_FAILED", "message": message},
                                                    })
                                                    .to_string(),
                                                );
                                                write_stdout("\n");
                                            }
                                        }
                                    }),
                                ) {
                                    Ok(()) => None,
                                    Err(e) => {
                                        Some(encode_frame(
                                            &kitron_host::ipc::IpcResponse::err(
                                                req.id.clone(),
                                                "NATIVE_API_FAILED",
                                                e.to_string(),
                                            ),
                                        ))
                                    }
                                }
                            }
                            #[cfg(not(target_os = "macos"))]
                            {
                                let _ = v;
                                Some(encode_frame(
                                    &kitron_host::ipc::IpcResponse::err(
                                        req.id.clone(),
                                        "PLATFORM_UNSUPPORTED",
                                        "printToPDF needs per-OS native glue (macOS done, others planned)"
                                            .to_string(),
                                    ),
                                ))
                            }
                        }
                    }
                }
                "webcontents.capturePage" => {
                    let shot: Result<Vec<u8>, String> = match views.get(id) {
                        None => Err(format!("Unknown window \"{id}\"")),
                        Some(v) => v
                            .window()
                            .outer_position()
                            .map_err(|e| e.to_string())
                            .and_then(|pos| {
                                let size = v.window().inner_size();
                                kitron_host::platform::capture_window_rect(
                                    pos.x, pos.y, size.width, size.height,
                                )
                                .map_err(|e| e.to_string())
                            }),
                    };
                    match shot {
                        Ok(bytes) => {
                            use base64::Engine as _;
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                            Some(encode_frame(&kitron_host::ipc::IpcResponse::ok(
                                req.id.clone(),
                                serde_json::json!(format!("data:image/png;base64,{b64}")),
                            )))
                        }
                        Err(message) => {
                            Some(encode_frame(&kitron_host::ipc::IpcResponse::err(
                                req.id.clone(),
                                "NATIVE_API_FAILED",
                                message,
                            )))
                        }
                    }
                }
                _ => None,
            };
            if let Some(frame) = response {
                write_stdout(&frame);
            } else if req.channel != "webcontents.evalJS"
                && req.channel != "webcontents.printToPDF"
            {
                let res = host.handle(req);
                write_stdout(&encode_frame(&res));
            }
            return;
        }

        if req.channel == "screen.getDisplays" {
            write_stdout(&encode_frame(&kitron_host::ipc::IpcResponse::ok(
                req.id.clone(),
                query_displays(target),
            )));
            return;
        }

        if req.channel == "screen.getCursor" {
            write_stdout(&encode_frame(&kitron_host::ipc::IpcResponse::ok(
                req.id.clone(),
                serde_json::json!({"x": last_cursor.0, "y": last_cursor.1}),
            )));
            return;
        }

        if req.channel == "native.desktop.getSources" {
            let types: Vec<String> = req
                .payload
                .get("types")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_else(|| vec!["screen".to_string(), "window".to_string()]);
            let mut sources: Vec<serde_json::Value> = Vec::new();
            if types.iter().any(|t| t == "screen") {
                if let Some(arr) = query_displays(target).as_array() {
                    for (i, d) in arr.iter().enumerate() {
                        sources.push(serde_json::json!({
                            "id": format!("screen:{i}"),
                            "name": d.get("label").cloned().unwrap_or(serde_json::json!("")),
                            "kind": "screen",
                        }));
                    }
                }
            }
            #[cfg(target_os = "macos")]
            if types.iter().any(|t| t == "window") {
                match kitron_host::platform::window_list() {
                    Ok(list) => sources.extend(list),
                    Err(e) => eprintln!("[Kitron][Rust] window list failed: {e}"),
                }
            }
            write_stdout(&encode_frame(&kitron_host::ipc::IpcResponse::ok(
                req.id.clone(),
                serde_json::Value::Array(sources),
            )));
            return;
        }

        // --- Classic window chrome + router. ---
        if known {
            if let Some(view) = views.get_mut(id) {
                let w = view.window();
                match req.channel.as_str() {
                    "window.show" => {
                        w.set_visible(true);
                        emit_window_event(id, "restored", serde_json::Value::Null);
                    }
                    "window.hide" => w.set_visible(false),
                    "window.minimize" => {
                        w.set_minimized(true);
                        emit_window_event(id, "minimized", serde_json::Value::Null);
                    }
                    "window.maximize" => {
                        w.set_maximized(true);
                        emit_window_event(id, "maximized", serde_json::Value::Null);
                    }
                    "window.restore" => {
                        w.set_maximized(false);
                        w.set_minimized(false);
                        w.set_visible(true);
                        emit_window_event(id, "restored", serde_json::Value::Null);
                    }
                    "window.setTitle" => {
                        if let Some(t) =
                            payload.get("title").and_then(|v| v.as_str())
                        {
                            w.set_title(t);
                            if let Some(m) = meta.get_mut(id) {
                                m.title = t.to_string();
                            }
                        }
                    }
                    "window.setSize" => {
                        let sw = payload
                            .get("width")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(1200) as u32;
                        let sh = payload
                            .get("height")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(800) as u32;
                        w.set_inner_size(LogicalSize::new(sw, sh));
                        emit_window_event(
                            id,
                            "resized",
                            serde_json::json!({"width": sw, "height": sh}),
                        );
                    }
                    "window.setPosition" => {
                        let x = payload
                            .get("x")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0) as f64;
                        let y = payload
                            .get("y")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0) as f64;
                        w.set_outer_position(winit::dpi::Position::Logical(
                            winit::dpi::LogicalPosition::new(x, y),
                        ));
                        emit_window_event(id, "moved", serde_json::json!({"x": x, "y": y}));
                    }
                    "window.setAlwaysOnTop" => {
                        let flag = payload
                            .get("flag")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        w.set_always_on_top(flag);
                    }
                    "window.setFullscreen" => {
                        let flag = payload
                            .get("flag")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(true);
                        w.set_fullscreen(if flag {
                            Some(winit::window::Fullscreen::Borderless(None))
                        } else {
                            None
                        });
                    }
                    "window.setMinimumSize" => {
                        let sw = payload
                            .get("width")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;
                        let sh = payload
                            .get("height")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0) as u32;
                        w.set_min_inner_size(Some(LogicalSize::new(sw, sh)));
                    }
                    "window.setMaximumSize" => {
                        let sw = payload
                            .get("width")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(100000) as u32;
                        let sh = payload
                            .get("height")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(100000) as u32;
                        w.set_max_inner_size(Some(LogicalSize::new(sw, sh)));
                    }
                    "window.openDevTools" | "window.closeDevTools" => {
                        eprintln!("[Kitron][Rust] DevTools inspector is not programmatically available in this wry version");
                    }
                    _ => {}
                }
            }
            if req.channel == "window.loadURL" {
                if let Some(u) = payload.get("url").and_then(|v| v.as_str()) {
                    if let Some(m) = meta.get_mut(id) {
                        m.url = u.to_string();
                    }
                    if let Some(v) = views.get_mut(id) {
                        let arg =
                            serde_json::to_string(u).unwrap_or_default();
                        let _ = v
                            .dispatch_script(&format!("window.location.href = {arg};"));
                        let _ = v.evaluate_script();
                    }
                }
            }
            if req.channel == "window.loadFile" {
                if let Some(p) = payload.get("path").and_then(|v| v.as_str()) {
                    let file_url = format!("file://{p}");
                    if let Some(m) = meta.get_mut(id) {
                        m.url = file_url.clone();
                    }
                    if let Some(v) = views.get_mut(id) {
                        let arg =
                            serde_json::to_string(&file_url).unwrap_or_default();
                        let _ = v
                            .dispatch_script(&format!("window.location.href = {arg};"));
                        let _ = v.evaluate_script();
                    }
                }
            }
            if req.channel == "window.close" {
                emit_window_event(id, "closed", serde_json::Value::Null);
                views.remove(id);
                meta.remove(id);
            }
        }
        let res = host.handle(req);
        write_stdout(&encode_frame(&res));
        let _ = HashMap::<String, String>::new;
    }

    /// Real display list from winit (Tranche A): id, bounds, scale, primary.
    fn query_displays(
        target: &winit::event_loop::EventLoopWindowTarget<UserEvent>,
    ) -> serde_json::Value {
        #[allow(deprecated)]
        let monitors: Vec<_> = target.available_monitors().collect();
        let list: Vec<serde_json::Value> = monitors
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let size = m.size();
                let pos = m.position();
                serde_json::json!({
                    "id": i,
                    "label": m.name().unwrap_or_else(|| format!("Display {i}")),
                    "bounds": {"x": pos.x, "y": pos.y, "width": size.width, "height": size.height},
                    "workArea": {"x": pos.x, "y": pos.y, "width": size.width, "height": size.height},
                    "scaleFactor": m.scale_factor(),
                    "primary": i == 0,
                })
            })
            .collect();
        serde_json::Value::Array(list)
    }
}

fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        prev(info);
        let dir = std::env::var("KITRON_CRASH_DIR").unwrap_or_else(|_| {
            let app = std::env::var("KITRON_APP_ID").unwrap_or_else(|_| "com.kitron.app".into());
            format!(
                "{}/kitron-{}-{}/Crashes",
                std::env::temp_dir().to_string_lossy(),
                app.replace(|c: char| !c.is_alphanumeric() && c != '.' && c != '-' && c != '_', "-"),
                std::process::id()
            )
        });
        let _ = std::fs::create_dir_all(&dir);
        let report = serde_json::json!({
            "id": uuid_like(),
            "time": chrono::Utc::now().to_rfc3339(),
            "kind": "rust-panic",
            "message": info.to_string(),
            "platform": format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        });
        let path = format!("{dir}/{}.json", report["id"].as_str().unwrap_or("panic"));
        let _ = std::fs::write(path, serde_json::to_string_pretty(&report).unwrap_or_default());
    }));
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}-{:x}", nanos, std::process::id())
}

fn main() {
    install_panic_hook();
    let args: Vec<String> = std::env::args().collect();
    let url = arg_value(&args, "--url", "http://127.0.0.1:5173");
    let title = arg_value(&args, "--title", "Kitron App");
    let width: u32 = arg_value(&args, "--width", "1200").parse().unwrap_or(1200);
    let height: u32 = arg_value(&args, "--height", "800").parse().unwrap_or(800);
    let devtools = args.iter().any(|a| a == "--devtools");

    #[cfg(feature = "native-webview")]
    {
        ui::run(url, title, width, height, devtools);
        return;
    }

    #[cfg(not(feature = "native-webview"))]
    {
        use std::io::{BufRead, Write};

        eprintln!(
            "[Kitron][Rust] Host starting ({} via {}) — {title} {width}x{height} → {url}{}",
            platform::platform_name(),
            platform::webview_backend(),
            if devtools { " (devtools)" } else { "" }
        );

        let mut host = Host::new(title, "0.1.0".to_string());
        let stdin = std::io::stdin();
        let mut buf = String::new();

        for line in stdin.lock().lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };
            buf.push_str(&line);
            buf.push('\n');
            for frame in split_lines(&mut buf) {
                match serde_json::from_str::<IpcRequest>(&frame) {
                    Ok(req) => {
                        let res = host.handle(req);
                        let out = encode_frame(&res);
                        let mut stdout = std::io::stdout();
                        let _ = stdout.write_all(out.as_bytes());
                        let _ = stdout.flush();
                    }
                    Err(e) => {
                        eprintln!("[Kitron][Rust] Ignoring malformed frame: {e}");
                    }
                }
            }
        }
        eprintln!("[Kitron][Rust] Host stdin closed — exiting.");
    }
}
