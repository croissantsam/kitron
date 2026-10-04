//! IPC codec — mirrors `packages/types/src/ipc.ts` (§11).
//!
//! Wire format is NDJSON: one JSON object per line on stdio.
//! Frames are requests (`{id, channel, payload}`), responses
//! (`{id, success, payload?, error?}`) or events (`{channel, payload}`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcRequest {
    pub id: String,
    pub channel: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcResponse {
    pub id: String,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<IpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcEvent {
    pub channel: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl IpcResponse {
    pub fn ok(id: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            success: true,
            payload: Some(payload),
            error: None,
        }
    }

    pub fn err(id: impl Into<String>, code: &str, message: String) -> Self {
        Self {
            id: id.into(),
            success: false,
            payload: None,
            error: Some(IpcError {
                code: code.to_string(),
                message,
                details: None,
            }),
        }
    }
}

pub fn encode_frame<T: Serialize>(value: &T) -> String {
    let mut s = serde_json::to_string(value).expect("frame must serialize");
    s.push('\n');
    s
}

/// Split buffered text into complete lines (without the newline).
pub fn split_lines(buf: &mut String) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(idx) = buf.find('\n') {
        let line: String = buf.drain(..=idx).collect();
        let trimmed = line.trim().to_string();
        if !trimmed.is_empty() {
            out.push(trimmed);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ndjson_round_trip() {
        let req = IpcRequest {
            id: "1".into(),
            channel: "ping".into(),
            payload: serde_json::Value::Null,
        };
        let line = encode_frame(&req);
        assert!(line.ends_with('\n'));
        let back: IpcRequest = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(back.channel, "ping");
    }

    #[test]
    fn split_lines_buffers_partials() {
        let mut buf = String::from("{\"a\":1}\n{\"b\":");
        let got = split_lines(&mut buf);
        assert_eq!(got.len(), 1);
        buf.push_str("2}\n");
        let got = split_lines(&mut buf);
        assert_eq!(got.len(), 1);
    }
}
