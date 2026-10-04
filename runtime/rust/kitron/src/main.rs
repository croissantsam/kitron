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
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Run { app_dir, headless, title, size } => {
            run(app_dir, headless, title, size).await?;
        }
    }
    Ok(())
}

mod anyhow {
    pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
}

async fn run(app_dir: String, headless: bool, title: Option<String>, size: Option<String>) -> anyhow::Result<()> {
    let (app, _rx) = AppManager::new();
    let app = Arc::new(app);
    let rt = Arc::new(Runtime::new(app.clone()));
    app.ready();

    let (mut w, mut h) = (1200u32, 800u32);
    if let Some(s) = size {
        if let Some((a, b)) = s.split_once('x') {
            if let (Ok(a), Ok(b)) = (a.parse(), b.parse()) {
                w = a;
                h = b;
            }
        }
    }
    let title = title.unwrap_or_else(|| "Kitron App".into());

    // Spawn the stdio IPC loop on a background task — never blocks the UI thread.
    let rt_ipc = rt.clone();
    tokio::spawn(async move {
        if let Err(e) = stdio_loop(rt_ipc).await {
            tracing::error!(error = %e, "stdio IPC loop ended");
        }
    });

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

    if headless {
        tracing::info!("kitron-runtime headless: serving IPC on stdio");
        // Park until killed; IPC runs on the background task.
        tokio::signal::ctrl_c().await.ok();
        return Ok(());
    }

    run_windowed(rt, title, w, h)?;
    Ok(())
}

fn run_windowed(rt: Arc<Runtime>, title: String, w: u32, h: u32) -> anyhow::Result<()> {
    use winit::event::{Event, WindowEvent};
    use winit::event_loop::{ControlFlow, EventLoop};
    use winit::window::WindowAttributes;

    let event_loop = EventLoop::new()?;
    let attrs = WindowAttributes::default()
        .with_title(title.clone())
        .with_inner_size(winit::dpi::LogicalSize::new(w, h));

    let mut window: Option<Arc<winit::window::Window>> = None;
    let mut window_id: Option<String> = None;

    // Pre-register the primary window in headless state so early
    // `window.get` calls from JS succeed before first paint.
    let boot_opts = kitron_core::WindowCreateOptions {
        width: w,
        height: h,
        title: title.clone(),
        center: true,
        ..Default::default()
    };
    let boot_info = rt.windows.create_headless(boot_opts);
    window_id = Some(boot_info.id.clone());

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Wait);
        match event {
            Event::Resumed => {
                if window.is_none() {
                    match elwt.create_window(attrs.clone()) {
                        Ok(created) => {
                            let win = Arc::new(created);
                            // Upgrade the boot record to a live OS window.
                            let _ = window_id.take();
                            let id = uuid::Uuid::new_v4().to_string();
                            let info = rt.windows.register_live(
                                id.clone(),
                                &kitron_core::WindowCreateOptions {
                                    width: w,
                                    height: h,
                                    title: title.clone(),
                                    ..Default::default()
                                },
                                win.clone(),
                            );
                            window_id = Some(info.id.clone());
                            window = Some(win);
                            tracing::info!(window_id = %info.id, "native window created");
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "window creation failed");
                            elwt.exit();
                        }
                    }
                }
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                tracing::info!("close requested — exiting (Windows behavior; macOS keeps running in full shell)");
                elwt.exit();
            }
            Event::WindowEvent { event: WindowEvent::Resized(s), .. } => {
                if let Some(id) = &window_id {
                    let _ = rt.windows.set_size(id, s.width, s.height);
                }
            }
            Event::WindowEvent { event: WindowEvent::Moved(p), .. } => {
                if let Some(id) = &window_id {
                    let _ = rt.windows.set_position(id, p.x, p.y);
                }
            }
            _ => {}
        }
    })?;
    Ok(())
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
