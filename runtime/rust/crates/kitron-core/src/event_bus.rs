//! Event bus shared between native (windows/lifecycle), the Node supervisor
//! and the JSI bridge pump.

use std::collections::VecDeque;
use std::sync::Mutex;

/// Stable event names surfaced to the JS layer (`app.on(...)`, `window.on(...)`,
/// `node.on(...)` — spec §11/§20/§30).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventName {
    AppReady,
    AppActivate,
    AppBeforeQuit,
    AppQuit,
    WindowCreated,
    WindowClosed,
    WindowFocus,
    WindowBlur,
    WindowResize,
    WindowMove,
    WindowMinimize,
    WindowRestore,
    NodeStarted,
    NodeReady,
    NodeLog,
    NodeStopping,
    NodeExited,
    NodeEvent,
    MenuAction,
    ShortcutTriggered,
}

impl EventName {
    /// Parses an event name (used when flushing queued events to a sink).
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "app.ready" => Self::AppReady,
            "app.activate" => Self::AppActivate,
            "app.beforeQuit" => Self::AppBeforeQuit,
            "app.quit" => Self::AppQuit,
            "window.created" => Self::WindowCreated,
            "window.closed" => Self::WindowClosed,
            "window.focus" => Self::WindowFocus,
            "window.blur" => Self::WindowBlur,
            "window.resize" => Self::WindowResize,
            "window.move" => Self::WindowMove,
            "window.minimize" => Self::WindowMinimize,
            "window.restore" => Self::WindowRestore,
            "node.started" => Self::NodeStarted,
            "node.ready" => Self::NodeReady,
            "node.log" => Self::NodeLog,
            "node.stopping" => Self::NodeStopping,
            "node.exited" => Self::NodeExited,
            "backend.event" => Self::NodeEvent,
            "menu.action" => Self::MenuAction,
            "shortcut.triggered" => Self::ShortcutTriggered,
            _ => return None,
        })
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppReady => "app.ready",
            Self::AppActivate => "app.activate",
            Self::AppBeforeQuit => "app.beforeQuit",
            Self::AppQuit => "app.quit",
            Self::WindowCreated => "window.created",
            Self::WindowClosed => "window.closed",
            Self::WindowFocus => "window.focus",
            Self::WindowBlur => "window.blur",
            Self::WindowResize => "window.resize",
            Self::WindowMove => "window.move",
            Self::WindowMinimize => "window.minimize",
            Self::WindowRestore => "window.restore",
            Self::NodeStarted => "node.started",
            Self::NodeReady => "node.ready",
            Self::NodeLog => "node.log",
            Self::NodeStopping => "node.stopping",
            Self::NodeExited => "node.exited",
            Self::NodeEvent => "backend.event",
            Self::MenuAction => "menu.action",
            Self::ShortcutTriggered => "shortcut.triggered",
        }
    }
}

/// A runtime event crossing to JavaScript.
#[derive(Debug, Clone)]
pub struct RuntimeEvent {
    pub name: &'static str,
    pub window_id: Option<u64>,
    pub payload: serde_json::Value,
}

/// Thread-safe, unbounded event queue. The platform shell and supervisor push;
/// the main-thread pump drains and dispatches to JS.
#[derive(Default)]
pub struct EventBus {
    queue: Mutex<VecDeque<RuntimeEvent>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self { queue: Mutex::new(VecDeque::new()) }
    }

    pub fn push(&self, name: EventName, payload: serde_json::Value) {
        self.push_windowed(name, None, payload);
    }

    pub fn push_windowed(
        &self,
        name: EventName,
        window_id: Option<u64>,
        payload: serde_json::Value,
    ) {
        self.queue.lock().unwrap().push_back(RuntimeEvent {
            name: name.as_str(),
            window_id,
            payload,
        });
    }

    /// Drains all pending events (FIFO).
    pub fn drain(&self) -> Vec<RuntimeEvent> {
        let mut q = self.queue.lock().unwrap();
        q.drain(..).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_fifo() {
        let bus = EventBus::new();
        bus.push(EventName::AppReady, serde_json::json!({}));
        bus.push_windowed(EventName::WindowFocus, Some(3), serde_json::json!({}));
        let events = bus.drain();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].name, "app.ready");
        assert_eq!(events[0].window_id, None);
        assert_eq!(events[1].name, "window.focus");
        assert_eq!(events[1].window_id, Some(3));
        assert!(bus.drain().is_empty());
    }
}
