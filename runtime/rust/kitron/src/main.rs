//! `kitron-runtime` binary — MVP native host.
//!
//! Modes:
//!   `kitron-runtime run --config kitron.config.json [--headless]`
//!     headless: IPC over stdio only (CI / tests, no display).
//!     default:  opens a real OS window via winit (AppKit on macOS,
//!               Win32 on Windows) and serves IPC on stdio.
//!
//! stdio protocol (MVP, newline-delimited JSON):
//!   IN:  { id, domain, method, params }
//!   OUT: { id, ok: true, result } | { id, ok: false, error }
//!   OUT (async): { type: "event", name, payload }
//!
//! The socket transport (Unix socket / named pipe) and the JSI binding
//! reuse the same `Runtime::dispatch`, so this stays compatible.

use clap::{Parser, Subcommand};
use kitron_core::IpcRequest;
use kitron_runtime::{app::AppManager, ipc_server::Runtime};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Parser)]
#[command(name = "kitron-runtime", version, about = "Kitron native runtime host")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run the app host (window + IPC + optional Node sidecar).
    Run {
        /// App directory containing kitron.config.json (or .ts — resolved by CLI).
        #[arg(long, default_value = ".")]
        app_dir: String,
        /// No OS window; IPC only. Used by tests/CI.
        #[arg(long, default_value_t = false)]
        headless: bool,
        /// Window title override.
        #[arg(long)]
        title: Option<String>,
        /// Window size override `WxH`, e.g. `1200x800`.
        #[arg(long)]
        size: Option<String>,
        /// Granted sensitive domains (§23), e.g. `--permissions shell,process`.
        /// Mirrors `permissions` in kitron.config.
        #[arg(long)]
        permissions: Option<String>,
        /// Unix socket path for UI-runner IPC (in addition to stdio).
        /// The CLI starts the UI entry with KITRON_SOCKET pointing here.
        #[arg(long)]
        socket: Option<String>,
    },
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    kitron_runtime::crash::init(env!("CARGO_PKG_VERSION").to_string());
    match cli.cmd {
        Cmd::Run { app_dir, headless, title, size, permissions, socket } => {
            run(app_dir, headless, title, size, permissions, socket)?;
        }
    }
    Ok(())
}

mod anyhow {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
}

/// AppKit requires the winit event loop on the MAIN thread, so `run` splits
/// in two: host tasks (stdio/socket IPC, Node sidecar) live on a Tokio
/// worker thread, and `run_windowed` owns the main thread below.
fn run(
    app_dir: String,
    headless: bool,
    _title: Option<String>,
    _size: Option<String>,
    permissions: Option<String>,
    socket: Option<String>,
) -> anyhow::Result<()> {
    let tokio_rt = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let (app, _rx) = AppManager::new();
    let app = Arc::new(app);
    let perms = kitron_core::permissions::Permissions::parse_flag(permissions.as_deref());
    let rt = Arc::new(Runtime::new_with_permissions(app.clone(), perms));
    app.ready();
    eprintln!("[kitron] runtime up (headless={headless})");

    let rt_host = rt.clone();
    let host = std::thread::spawn(move || tokio_rt.block_on(run_host(rt_host, app_dir, socket)));

    if headless {
        // Park the main thread here too; Ctrl+C ends run_host above.
        host.join().map_err(|_| "host thread panicked")??;
        return Ok(());
    }

    run_windowed(rt)?;
    // Event loop exited: tear everything down.
    std::process::exit(0);
}

/// stdio + socket IPC and the Node sidecar. Runs on a Tokio worker thread;
/// parks forever (windowed) or until Ctrl+C (headless).
async fn run_host(rt: Arc<Runtime>, app_dir: String, socket: Option<String>) -> anyhow::Result<()> {
    // stdio IPC loop — never blocks the UI thread.
    let rt_ipc = rt.clone();
    tokio::spawn(async move {
        if let Err(e) = stdio_loop(rt_ipc).await {
            tracing::error!(error = %e, "stdio IPC loop ended");
        }
    });

    // Optional Unix socket: the CLI's UI runner connects here so app code
    // (`src/App.tsx`) can drive windows while the host owns the event loop.
    if let Some(path) = socket {
        let rt_sock = rt.clone();
        tokio::spawn(async move {
            if let Err(e) = socket_loop(rt_sock, path).await {
                tracing::error!(error = %e, "socket IPC loop ended");
            }
        });
    }

    // Best-effort Node sidecar: `node <app_dir>/backend/index.js` if present.
    let backend_entry = std::path::Path::new(&app_dir).join("backend").join("index.js");
    if backend_entry.exists() {
        let entry = backend_entry.to_string_lossy().into_owned();
        let node = rt.node.clone();
        tokio::spawn(async move {
            if let Err(e) = node.start(entry, vec![], Default::default(), None).await {
                tracing::warn!(error = %e, "node sidecar failed to start");
            }
        });
    }

    tokio::signal::ctrl_c().await.ok();
    Ok(())
}

fn run_windowed(rt: Arc<Runtime>) -> anyhow::Result<()> {
    use kitron_runtime::os_events::KitronUserEvent;
    use winit::event::{ElementState, Event, MouseButton, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoop};

    type KitronWindow = Arc<winit::window::Window>;
    struct LiveWindow {
        kitron_id: String,
        handle: KitronWindow,
        surface: Option<softbuffer::Surface<KitronWindow, KitronWindow>>,
        cursor: (f32, f32),
    }

    let event_loop = EventLoop::<KitronUserEvent>::with_user_event().build()?;
    kitron_runtime::os_events::set_proxy(event_loop.create_proxy());
    let mut windows: std::collections::HashMap<winit::window::WindowId, LiveWindow> =
        std::collections::HashMap::new();
    #[allow(unused_mut)]
    let mut first_hwnd: Option<isize> = None;

    // Open a real OS window for every headless record that has none, and
    // drop locals whose records were closed. Main thread only.
    let sync_windows = |rt: &Arc<Runtime>,
                        windows: &mut std::collections::HashMap<winit::window::WindowId, LiveWindow>,
                        elwt: &winit::event_loop::ActiveEventLoop,
                        first_hwnd: &mut Option<isize>| {
        windows.retain(|_, lw| rt.windows.get(&lw.kitron_id).is_ok());
        let pending = rt.windows.unbound();
        if !pending.is_empty() {
            eprintln!("[kitron] sync_windows: opening {} window(s)", pending.len());
        }
        for (kid, opts) in pending {
            let mut attrs = winit::window::WindowAttributes::default()
                .with_title(opts.title.clone())
                .with_inner_size(winit::dpi::LogicalSize::new(opts.width, opts.height))
                .with_decorations(opts.decorations)
                .with_resizable(opts.resizable)
                .with_active(true)
                .with_visible(opts.visible);
            if opts.center {
                if let Some(pos) = centered_position(elwt, opts.width, opts.height) {
                    attrs = attrs.with_position(pos);
                }
            } else if let (Some(x), Some(y)) = (opts.x, opts.y) {
                attrs = attrs.with_position(winit::dpi::LogicalPosition::new(x as f64, y as f64));
            }
            match elwt.create_window(attrs) {
                Ok(created) => {
                    let win: KitronWindow = Arc::new(created);
                    let wid = win.id();
                    let info = rt.windows.register_live(kid.clone(), &opts, win.clone());
                    #[cfg(target_os = "windows")]
                    {
                        use raw_window_handle::HasWindowHandle;
                        if first_hwnd.is_none() {
                            if let Ok(handle) = win.window_handle() {
                                if let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
                                    *first_hwnd = Some(h.hwnd.get() as isize);
                                }
                            }
                        }
                    }
                    let _ = first_hwnd.is_none();
                    win.focus_window();
                    win.request_redraw();
                    windows.insert(wid, LiveWindow { kitron_id: info.id.clone(), handle: win, surface: None, cursor: (0.0, 0.0) });
                    tracing::info!(window_id = %info.id, "native window created");
                }
                Err(e) => tracing::error!(error = %e, "window creation failed"),
            }
        }
    };

    // Paint one window with its Kitron UI tree (no-op without a ui root).
    // KITRON_NO_PAINT=1 exercises layout + pixmap paint but skips the
    // softbuffer present (diagnostic: isolates display-driver crashes).
    let no_paint = std::env::var("KITRON_NO_PAINT").is_ok();
    let paint_window = move |rt: &Arc<Runtime>, lw: &mut LiveWindow| {
        let size = lw.handle.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        let mut pixmap = match tiny_skia::Pixmap::new(size.width, size.height) {
            Some(p) => p,
            None => return,
        };
        match rt.ui.paint_window(&lw.kitron_id, size.width as f32, size.height as f32, &mut pixmap) {
            Ok(()) => {}
            Err(_) => return, // NoRoot: leave the window blank.
        }
        if no_paint {
            return;
        }
        if lw.surface.is_none() {
            match softbuffer::Context::new(lw.handle.clone()) {
                Ok(ctx) => match softbuffer::Surface::new(&ctx, lw.handle.clone()) {
                    Ok(s) => {
                        std::mem::forget(ctx);
                        lw.surface = Some(s);
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "softbuffer surface failed");
                        return;
                    }
                },
                Err(e) => {
                    tracing::warn!(error = %e, "softbuffer context failed");
                    return;
                }
            }
        }
        if let Some(surface) = lw.surface.as_mut() {
            use std::num::NonZeroU32;
            if let (Some(ww), Some(hh)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) {
                if surface.resize(ww, hh).is_ok() {
                    if let Ok(mut buffer) = surface.buffer_mut() {
                        let src = pixmap.pixels();
                        for (dst, px) in buffer.iter_mut().zip(src.iter()) {
                            // Premultiplied RGBA → 0x00RRGGBB.
                            let a = px.alpha() as u32;
                            if a == 0 {
                                *dst = 0;
                            } else {
                                let (r, g, b) = (px.red() as u32, px.green() as u32, px.blue() as u32);
                                *dst = ((r * 255 / a) << 16) | ((g * 255 / a) << 8) | (b * 255 / a);
                            }
                        }
                        if let Err(e) = buffer.present() {
                            tracing::warn!(error = %e, "present failed");
                        }
                    }
                }
            }
        }
    };

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);
        match event {
            Event::Resumed => {
                sync_windows(&rt, &mut windows, elwt, &mut first_hwnd);
            }
            Event::UserEvent(KitronUserEvent::BindOs) => {
                // Main thread: safe to create NSStatusItem / NSMenu / hotkeys.
                rt.bind_os(first_hwnd);
            }
            Event::UserEvent(KitronUserEvent::SyncWindows) => {
                sync_windows(&rt, &mut windows, elwt, &mut first_hwnd);
            }
            Event::WindowEvent { window_id, event } => {
                let kid = windows.get(&window_id).map(|lw| lw.kitron_id.clone());
                match event {
                    WindowEvent::CloseRequested => {
                        if let Some(kid) = kid {
                            let _ = rt.windows.close(&kid);
                        }
                        windows.remove(&window_id);
                        if windows.is_empty() {
                            tracing::info!("last window closed — exiting");
                            elwt.exit();
                        }
                    }
                    WindowEvent::Resized(s) => {
                        if let Some(kid) = &kid {
                            let _ = rt.windows.set_size(kid, s.width, s.height);
                        }
                        if let Some(lw) = windows.get_mut(&window_id) {
                            paint_window(&rt, lw);
                        }
                    }
                    WindowEvent::RedrawRequested => {
                        if let Some(lw) = windows.get_mut(&window_id) {
                            paint_window(&rt, lw);
                        }
                    }
                    WindowEvent::CursorMoved { position, .. } => {
                        if let Some(lw) = windows.get_mut(&window_id) {
                            lw.cursor = (position.x as f32, position.y as f32);
                        }
                    }
                    WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                        if let Some(lw) = windows.get_mut(&window_id) {
                            let (x, y) = lw.cursor;
                            let size = lw.handle.inner_size();
                            let kid = lw.kitron_id.clone();
                            match rt.ui.press_at(&kid, x, y, size.width as f32, size.height as f32) {
                                Ok(Some(btn)) => {
                                    rt.app.emit("ui.press", serde_json::json!({ "windowId": kid, "nodeId": btn }));
                                }
                                _ => {}
                            }
                            paint_window(&rt, lw);
                        }
                    }
                    WindowEvent::MouseWheel { delta, .. } => {
                        use winit::event::MouseScrollDelta;
                        if let Some(lw) = windows.get_mut(&window_id) {
                            let (x, y) = lw.cursor;
                            let size = lw.handle.inner_size();
                            let kid = lw.kitron_id.clone();
                            let (dx, dy) = match delta {
                                MouseScrollDelta::LineDelta(dx, dy) => (dx * 24.0, dy * 24.0),
                                MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
                            };
                            // Natural scrolling: content follows the fingers.
                            match rt.ui.scroll_by(&kid, x, y, -dx, -dy, size.width as f32, size.height as f32) {
                                Ok(Some((node, (sx, sy)))) => {
                                    rt.app.emit("ui.scroll", serde_json::json!({ "windowId": kid, "nodeId": node, "x": sx, "y": sy }));
                                }
                                _ => {}
                            }
                            paint_window(&rt, lw);
                        }
                    }
                    WindowEvent::KeyboardInput { event: key, .. } => {
                        use winit::keyboard::{Key, NamedKey};
                        if key.state == ElementState::Pressed {
                            if let Some(lw) = windows.get_mut(&window_id) {
                                let kid = lw.kitron_id.clone();
                                let mut changed: Option<(String, String)> = None;
                                if let Some(text) = key.text.as_ref() {
                                    changed = rt.ui.input_text(&kid, Some(text.as_str()), false).unwrap_or(None);
                                } else if matches!(key.logical_key, Key::Named(NamedKey::Backspace)) {
                                    changed = rt.ui.input_text(&kid, None, true).unwrap_or(None);
                                }
                                if let Some((node, value)) = changed {
                                    rt.app.emit("ui.change", serde_json::json!({ "windowId": kid, "nodeId": node, "value": value }));
                                }
                                paint_window(&rt, lw);
                            }
                        }
                    }
                    WindowEvent::Moved(p) => {
                        if let Some(kid) = &kid {
                            let _ = rt.windows.set_position(kid, p.x, p.y);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    })?;
    Ok(())
}

/// Center a WxH logical window on the primary monitor (logical px).
fn centered_position(
    elwt: &winit::event_loop::ActiveEventLoop,
    w: u32,
    h: u32,
) -> Option<winit::dpi::LogicalPosition<f64>> {
    let m = elwt.primary_monitor()?;
    let scale = m.scale_factor().max(1.0);
    let pos = m.position();
    let size = m.size();
    let (mx, my) = (pos.x as f64 / scale, pos.y as f64 / scale);
    let (mw, mh) = (size.width as f64 / scale, size.height as f64 / scale);
    Some(winit::dpi::LogicalPosition::new(
        mx + (mw - w as f64) / 2.0,
        my + (mh - h as f64) / 2.0,
    ))
}

#[cfg(unix)]
async fn socket_loop(rt: Arc<Runtime>, path: String) -> anyhow::Result<()> {
    use tokio::net::UnixListener;
    let _ = std::fs::remove_file(&path); // stale socket from a previous run
    let listener = UnixListener::bind(&path)?;
    eprintln!("[kitron] IPC socket listening on {path}");
    tracing::info!(socket = %path, "IPC socket listening");
    loop {
        let (stream, _) = listener.accept().await?;
        let rt = rt.clone();
        tokio::spawn(async move {
            if let Err(e) = serve_stream(rt, stream).await {
                tracing::debug!(error = %e, "socket client gone");
            }
        });
    }
}

#[cfg(unix)]
async fn serve_stream(rt: Arc<Runtime>, stream: tokio::net::UnixStream) -> anyhow::Result<()> {
    let (rh, wh) = tokio::io::split(stream);
    let mut reader = BufReader::new(rh).lines();
    // Native events fan out to every connected client.
    let mut rx = rt.app.subscribe();
    let wh = Arc::new(tokio::sync::Mutex::new(wh));
    let event_out = wh.clone();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok((name, payload)) => {
                    let line = serde_json::json!({ "type": "event", "name": name, "payload": payload });
                    let mut s = serde_json::to_string(&line).unwrap_or_default();
                    s.push('\n');
                    let mut g = event_out.lock().await;
                    use tokio::io::AsyncWriteExt;
                    if g.write_all(s.as_bytes()).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });
    while let Some(line) = reader.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let req: Result<IpcRequest, _> = serde_json::from_str(&line);
        let resp_value: serde_json::Value = match req {
            Ok(r) => serde_json::to_value(rt.dispatch(r).await)?,
            Err(e) => serde_json::json!({
                "id": 0, "ok": false,
                "error": { "code": "KITRON_INVALID_PARAMS", "message": format!("bad IPC frame: {e}"), "platform": kitron_core::current_platform() }
            }),
        };
        let mut out = serde_json::to_string(&resp_value)?;
        out.push('\n');
        let mut g = wh.lock().await;
        use tokio::io::AsyncWriteExt;
        g.write_all(out.as_bytes()).await?;
        g.flush().await?;
    }
    Ok(())
}

#[cfg(not(unix))]
async fn socket_loop(_rt: Arc<Runtime>, _path: String) -> anyhow::Result<()> {
    anyhow::bail!("--socket is Unix-only for now (Windows named pipes are Phase 4)");
}

async fn stdio_loop(rt: Arc<Runtime>) -> anyhow::Result<()> {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    let stdout = Arc::new(tokio::sync::Mutex::new(tokio::io::stdout()));

    // Forward native events (tray clicks, menu actions, shortcut presses,
    // deep links, app lifecycle) to JS as `{ type: "event", ... }` lines.
    // The TS `StdioTransport` routes them to `onNativeEvent` listeners.
    let mut rx = rt.app.subscribe();
    let event_out = stdout.clone();
    tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok((name, payload)) => {
                    let line =
                        serde_json::json!({ "type": "event", "name": name, "payload": payload });
                    let mut s = serde_json::to_string(&line).unwrap_or_default();
                    s.push('\n');
                    let mut g = event_out.lock().await;
                    if g.write_all(s.as_bytes()).await.is_err() {
                        break;
                    }
                    if g.flush().await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });

    while let Some(line) = reader.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let req: Result<IpcRequest, _> = serde_json::from_str(&line);
        let resp_value: serde_json::Value = match req {
            Ok(r) => serde_json::to_value(rt.dispatch(r).await)?,
            Err(e) => serde_json::json!({
                "id": 0, "ok": false,
                "error": { "code": "KITRON_INVALID_PARAMS", "message": format!("bad IPC frame: {e}"), "platform": kitron_core::current_platform() }
            }),
        };
        let mut out = serde_json::to_string(&resp_value)?;
        out.push('\n');
        let mut g = stdout.lock().await;
        g.write_all(out.as_bytes()).await?;
        g.flush().await?;
    }
    Ok(())
}
