//! Rust ↔ Node wire protocol (spec §28/§30).
//!
//! Transport: NDJSON over the Node child process' stdio. One JSON value per
//! line, UTF-8. Frames are small structured messages — app-level IPC is
//! intentionally serialized here because RN's JSON bridge already serializes
//! everything anyway; the JSI path (§8) is for high-frequency native calls and
//! bypasses this transport entirely.

use serde::{Deserialize, Serialize};

/// Protocol version negotiated on the `hello` frame.
pub const PROTOCOL_VERSION: u32 = 1;
/// Frame written by the Rust side to shut the host down gracefully.
pub const OP_SHUTDOWN: &str = "shutdown";

/// Frame sent by Node → Kitron right after boot (spec §29).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloFrame {
    pub protocol: u32,
    pub pid: u32,
    /// Host script version; enables forward compatibility checks.
    pub host_version: String,
    /// Path of the backend entry that was dynamically imported.
    pub entry: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyFrame {
    /// Number of handlers registered before `ready` was sent.
    pub handlers: u32,
}

/// Kitron → Node: invoke a registered backend handler (spec §30).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvokeFrame {
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultFrame {
    pub id: u64,
    pub result: serde_json::Value,
}

/// Node → Kitron: tagged log line, forwarded to the unified logger (spec §34).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogFrame {
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorFrame {
    pub id: u64,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Unidirectional Node → Kitron event (spec §20/§30 event plumbing).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventFrame {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

/// Rust → Node: host shutdown request (spec §16; supervisor kills hard after).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownFrame {
    pub reason: Option<String>,
}

/// Any frame in either direction.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
#[allow(clippy::large_enum_variant)]
pub enum Frame {
    Hello(HelloFrame),
    Ready(ReadyFrame),
    Invoke(InvokeFrame),
    Result(ResultFrame),
    Error(ErrorFrame),
    Log(LogFrame),
    Event(EventFrame),
    Shutdown(ShutdownFrame),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl Frame {
    /// Serialize a frame to one NDJSON line (including the terminating `\n`).
    pub fn to_line(&self) -> String {
        let mut body = serde_json::to_string(self).expect("frame is serializable");
        body.push('\n');
        body
    }

    /// Parse one NDJSON line into a frame.
    pub fn from_line(line: &str) -> serde_json::Result<Frame> {
        serde_json::from_str(line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoke_roundtrip() {
        let f = Frame::Invoke(InvokeFrame {
            id: 7,
            method: "users.get".into(),
            payload: Some(serde_json::json!({ "id": 123 })),
        });
        let line = f.to_line();
        let back = Frame::from_line(&line).unwrap();
        match back {
            Frame::Invoke(i) => {
                assert_eq!(i.id, 7);
                assert_eq!(i.method, "users.get");
                assert_eq!(i.payload.unwrap()["id"], 123);
            }
            other => panic!("wrong frame: {other:?}"),
        }
    }

    #[test]
    fn error_frame_keeps_code() {
        let line = Frame::Error(ErrorFrame {
            id: 1,
            code: "NODE_NO_HANDLER".into(),
            message: "no handler for users.get".into(),
            details: None,
        })
        .to_line();
        let back = Frame::from_line(&line).unwrap();
        match back {
            Frame::Error(e) => assert_eq!(e.code, "NODE_NO_HANDLER"),
            other => panic!("wrong frame: {other:?}"),
        }
    }
}
