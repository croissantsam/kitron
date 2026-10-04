//! App lifecycle. macOS (stay alive without windows, `activate`) vs
//! Windows (quit on last window) divergence is handled by the event loop
//! in the `kitron` binary; this module owns the state machine.

use tokio::sync::broadcast;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppState {
    Starting,
    Ready,
    Hidden,
    Quitting,
}

pub struct AppManager {
    state: std::sync::Mutex<AppState>,
    events: broadcast::Sender<(String, serde_json::Value)>,
}

impl AppManager {
    pub fn new() -> (Self, broadcast::Receiver<(String, serde_json::Value)>) {
        let (tx, rx) = broadcast::channel(128);
        (Self { state: std::sync::Mutex::new(AppState::Starting), events: tx }, rx)
    }

    pub fn ready(&self) {
        *self.state.lock().unwrap() = AppState::Ready;
        let _ = self.events.send(("app.ready".into(), serde_json::Value::Null));
    }

    pub fn hide(&self) {
        *self.state.lock().unwrap() = AppState::Hidden;
    }

    pub fn show(&self) {
        *self.state.lock().unwrap() = AppState::Ready;
    }

    pub fn quit(&self) {
        *self.state.lock().unwrap() = AppState::Quitting;
        let _ = self.events.send(("app.quit".into(), serde_json::Value::Null));
    }

    pub fn state(&self) -> AppState {
        self.state.lock().unwrap().clone()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<(String, serde_json::Value)> {
        self.events.subscribe()
    }

    pub fn emit(&self, name: &str, payload: serde_json::Value) {
        let _ = self.events.send((name.into(), payload));
    }
}

impl Default for AppManager {
    fn default() -> Self {
        let (m, _) = Self::new();
        m
    }
}
