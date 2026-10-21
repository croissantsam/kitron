//! Unified logger (spec §34): one sink for Rust, JS and Node logs.
//!
//! The logger writes to stderr with a standard tag prefix so `kitron dev` and
//! `kitron doctor` can tail a single stream. Node output is piped through the
//! supervisor (`NodeEvent::Log` → `EventName::NodeLog` → bridge → logger) so
//! everything funnels through here.

use std::io::Write;
use std::sync::atomic::{AtomicU8, Ordering};

/// Log levels matching spec §34 (`logger.debug/info/warn/error`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LogLevelTag(pub u8);

pub const DEBUG: LogLevelTag = LogLevelTag(0);
pub const INFO: LogLevelTag = LogLevelTag(1);
pub const WARN: LogLevelTag = LogLevelTag(2);
pub const ERROR: LogLevelTag = LogLevelTag(3);

impl LogLevelTag {
    fn label(self) -> &'static str {
        match self {
            DEBUG => "debug",
            INFO => "info",
            WARN => "warn",
            _ => "error",
        }
    }
}

static MIN_LEVEL: AtomicU8 = AtomicU8::new(0); // 0 = debug (all)

fn label_css<'a>(l: LogLevelTag) -> &'a str {
    match l {
        DEBUG => "34m",
        INFO => "32m",
        WARN => "33m",
        _ => "31m",
    }
}

/// Shared logger (cheap clone via Arc in `Core`).
pub struct SharedLogger {
    tag_path: String,
}

impl SharedLogger {
    pub fn new(tag_path: &str) -> Self {
        Self {
            tag_path: tag_path.to_string(),
        }
    }

    /// Sets the minimum level (0=debug .. 3=error). Maps `KITRON_LOG` env.
    pub fn set_min_level(level: LogLevelTag) {
        MIN_LEVEL.store(level.0, Ordering::Relaxed);
    }

    fn log(&self, level: LogLevelTag, scope: &str, message: &str) {
        if level.0 < MIN_LEVEL.load(Ordering::Relaxed) {
            return;
        }
        let mut err = std::io::stderr().lock();
        let _ = writeln!(
            err,
            "[\x1b[{}m{}\x1b[0m] [{}:{}] {}",
            label_css(level),
            level.label(),
            self.tag_path,
            scope,
            message
        );
    }

    pub fn debug(&self, scope: &str, message: &str) {
        self.log(DEBUG, scope, message);
    }

    pub fn info(&self, scope: &str, message: &str) {
        self.log(INFO, scope, message);
    }

    pub fn warn(&self, scope: &str, message: &str) {
        self.log(WARN, scope, message);
    }

    pub fn error(&self, scope: &str, message: &str) {
        self.log(ERROR, scope, message);
    }
}

/// Convenience macro to emit a log and a matching bus event.
/// Decouples future file-sink implementation (log rotation etc.).
#[macro_export]
macro_rules! kitron_log {
    ($core:expr, dbg, $($arg:tt)*) => {
        $core.logger.debug("core", &format!($($arg)*))
    };
    ($core:expr, info, $($arg:tt)*) => {
        $core.logger.info("core", &format!($($arg)*))
    };
    ($core:expr, warn, $($arg:tt)*) => {
        $core.logger.warn("core", &format!($($arg)*))
    };
    ($core:expr, error, $($arg:tt)*) => {
        $core.logger.error("core", &format!($($arg)*))
    };
}
