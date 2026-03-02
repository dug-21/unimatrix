//! Wire protocol types and framing for hook IPC.
//!
//! Defines the length-prefixed JSON protocol used between hook processes
//! and the UDS listener. Per ADR-005: 4-byte BE u32 length prefix + JSON payload,
//! with serde-tagged enums for request/response routing.

use std::fmt;
use std::io::{self, Read, Write};
use std::time::Duration;

use serde::{Deserialize, Serialize};

// -- Constants --

/// Maximum payload size: 1 MiB.
pub const MAX_PAYLOAD_SIZE: usize = 1_048_576;

/// Frame header size: 4-byte BE u32 length prefix.
pub const FRAME_HEADER_SIZE: usize = 4;

// -- Error codes --

/// UID mismatch between hook process and server.
pub const ERR_UID_MISMATCH: i32 = -32001;

/// Process lineage verification failed.
pub const ERR_LINEAGE_FAILED: i32 = -32002;

/// Unknown request type received.
pub const ERR_UNKNOWN_REQUEST: i32 = -32003;

/// Invalid payload content.
pub const ERR_INVALID_PAYLOAD: i32 = -32004;

/// Internal server error.
pub const ERR_INTERNAL: i32 = -32005;

// -- HookInput (Claude Code stdin JSON -- ADR-006) --

/// Represents the JSON blob that Claude Code pipes to hook processes on stdin.
///
/// All fields use `#[serde(default)]` for maximum defensive parsing per ADR-006.
/// Unknown fields are captured by the `extra` flatten field.
#[derive(Deserialize, Debug, Clone)]
pub struct HookInput {
    /// The hook event name (e.g., "PreToolUse", "PostToolUse", "Stop").
    #[serde(default)]
    pub hook_event_name: String,

    /// Claude Code session identifier.
    #[serde(default)]
    pub session_id: Option<String>,

    /// Current working directory of the Claude Code session.
    #[serde(default)]
    pub cwd: Option<String>,

    /// Path to the session transcript file.
    #[serde(default)]
    pub transcript_path: Option<String>,

    /// User prompt text (UserPromptSubmit events only).
    #[serde(default)]
    pub prompt: Option<String>,

    /// Catch-all for unknown fields (forward compatibility).
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// -- HookRequest (IPC wire protocol) --

/// Request sent from hook processes to the UDS listener.
///
/// Uses `#[serde(tag = "type")]` for JSON routing per ADR-005.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum HookRequest {
    /// Health check.
    Ping,

    /// Register a new session.
    SessionRegister {
        session_id: String,
        cwd: String,
        agent_role: Option<String>,
        feature: Option<String>,
    },

    /// Close an existing session.
    SessionClose {
        session_id: String,
        outcome: Option<String>,
        duration_secs: u64,
    },

    /// Record a single event.
    RecordEvent {
        #[serde(flatten)]
        event: ImplantEvent,
    },

    /// Record a batch of events.
    RecordEvents { events: Vec<ImplantEvent> },

    // -- Stubs for future features (col-007+) --

    /// Search context entries.
    ContextSearch {
        query: String,
        #[serde(default)]
        session_id: Option<String>,
        role: Option<String>,
        task: Option<String>,
        feature: Option<String>,
        k: Option<u32>,
        max_tokens: Option<u32>,
    },

    /// Request a role briefing (future).
    #[allow(dead_code)]
    Briefing {
        role: String,
        task: String,
        feature: Option<String>,
        max_tokens: Option<u32>,
    },

    /// Request a compact context payload for PreCompact hook.
    CompactPayload {
        session_id: String,
        /// Reserved for col-010: once INJECTION_LOG persists to redb, the hook
        /// process can populate this from disk after a server restart, giving
        /// the server richer input than the briefing fallback. Currently empty
        /// (server tracks injection history in-memory via SessionRegistry).
        injected_entry_ids: Vec<u64>,
        role: Option<String>,
        feature: Option<String>,
        token_limit: Option<u32>,
    },
}

// -- HookResponse (IPC wire protocol) --

/// Response sent from the UDS listener back to hook processes.
///
/// Uses `#[serde(tag = "type")]` for JSON routing per ADR-005.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum HookResponse {
    /// Health check response.
    Pong { server_version: String },

    /// Acknowledgement (for fire-and-forget requests).
    Ack,

    /// Error response.
    Error { code: i32, message: String },

    /// Search/lookup results.
    Entries {
        items: Vec<EntryPayload>,
        total_tokens: u32,
    },

    /// Briefing content (compaction defense or role briefing).
    BriefingContent {
        content: String,
        token_count: u32,
    },
}

// -- ImplantEvent --

/// A single event recorded by a hook process.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ImplantEvent {
    /// Type of event (e.g., "tool_use", "context_read").
    pub event_type: String,

    /// Session that generated the event.
    pub session_id: String,

    /// Unix timestamp (seconds since epoch).
    pub timestamp: u64,

    /// Event-specific data.
    pub payload: serde_json::Value,
}

// -- EntryPayload (stub for future search results) --

/// A knowledge entry returned in search/briefing results.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EntryPayload {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub confidence: f64,
    pub similarity: f64,
    pub category: String,
}

// -- TransportError --

/// Errors that can occur during transport operations.
#[derive(Debug)]
pub enum TransportError {
    /// Server is not reachable (socket not found, connection refused).
    Unavailable(String),

    /// Operation timed out.
    Timeout(Duration),

    /// Server rejected the request.
    Rejected { code: i32, message: String },

    /// Serialization/deserialization error.
    Codec(String),

    /// I/O or connection error.
    Transport(String),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Unavailable(msg) => write!(f, "server unavailable: {msg}"),
            TransportError::Timeout(d) => write!(f, "operation timed out after {d:?}"),
            TransportError::Rejected { code, message } => {
                write!(f, "request rejected ({code}): {message}")
            }
            TransportError::Codec(msg) => write!(f, "codec error: {msg}"),
            TransportError::Transport(msg) => write!(f, "transport error: {msg}"),
        }
    }
}

impl std::error::Error for TransportError {}

impl From<io::Error> for TransportError {
    fn from(err: io::Error) -> Self {
        match err.kind() {
            io::ErrorKind::TimedOut => TransportError::Timeout(Duration::from_secs(0)),
            io::ErrorKind::ConnectionRefused => {
                TransportError::Unavailable(format!("connection refused: {err}"))
            }
            io::ErrorKind::NotFound => {
                TransportError::Unavailable(format!("socket not found: {err}"))
            }
            _ => TransportError::Transport(err.to_string()),
        }
    }
}

// -- Framing Functions --

/// Write a length-prefixed frame to a writer.
///
/// Format: 4-byte big-endian u32 length prefix + payload bytes.
/// Rejects payloads exceeding `MAX_PAYLOAD_SIZE` (1 MiB).
pub fn write_frame(writer: &mut impl Write, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_PAYLOAD_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "payload size {} exceeds maximum {}",
                payload.len(),
                MAX_PAYLOAD_SIZE
            ),
        ));
    }

    let length = payload.len() as u32;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(payload)?;
    writer.flush()?;
    Ok(())
}

/// Read a length-prefixed frame from a reader.
///
/// Returns the payload bytes. Rejects payloads exceeding `max_size`.
/// Returns `TransportError::Transport` on EOF, `TransportError::Codec` on size violations.
pub fn read_frame(reader: &mut impl Read, max_size: usize) -> Result<Vec<u8>, TransportError> {
    let mut header = [0u8; FRAME_HEADER_SIZE];
    reader.read_exact(&mut header).map_err(|e| {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            TransportError::Transport("connection closed during header read".to_string())
        } else {
            TransportError::from(e)
        }
    })?;

    let length = u32::from_be_bytes(header) as usize;

    if length == 0 {
        return Err(TransportError::Codec("empty payload".to_string()));
    }

    if length > max_size {
        return Err(TransportError::Codec(format!(
            "payload size {length} exceeds maximum {max_size}"
        )));
    }

    let mut buffer = vec![0u8; length];
    reader.read_exact(&mut buffer).map_err(|e| {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            TransportError::Transport("connection closed during payload read".to_string())
        } else {
            TransportError::from(e)
        }
    })?;

    Ok(buffer)
}

// -- Serialization Helpers --

/// Serialize a `HookRequest` to JSON bytes.
pub fn serialize_request(request: &HookRequest) -> Result<Vec<u8>, TransportError> {
    serde_json::to_vec(request).map_err(|e| TransportError::Codec(e.to_string()))
}

/// Deserialize a `HookRequest` from JSON bytes.
pub fn deserialize_request(data: &[u8]) -> Result<HookRequest, TransportError> {
    serde_json::from_slice(data).map_err(|e| TransportError::Codec(e.to_string()))
}

/// Serialize a `HookResponse` to JSON bytes.
pub fn serialize_response(response: &HookResponse) -> Result<Vec<u8>, TransportError> {
    serde_json::to_vec(response).map_err(|e| TransportError::Codec(e.to_string()))
}

/// Deserialize a `HookResponse` from JSON bytes.
pub fn deserialize_response(data: &[u8]) -> Result<HookResponse, TransportError> {
    serde_json::from_slice(data).map_err(|e| TransportError::Codec(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // -- Round-trip tests --

    #[test]
    fn round_trip_ping_pong() {
        let req = HookRequest::Ping;
        let bytes = serialize_request(&req).unwrap();
        let decoded = deserialize_request(&bytes).unwrap();
        assert!(matches!(decoded, HookRequest::Ping));
    }

    #[test]
    fn round_trip_session_register() {
        let req = HookRequest::SessionRegister {
            session_id: "sess-123".to_string(),
            cwd: "/workspace".to_string(),
            agent_role: Some("developer".to_string()),
            feature: Some("col-006".to_string()),
        };
        let bytes = serialize_request(&req).unwrap();
        let decoded = deserialize_request(&bytes).unwrap();
        match decoded {
            HookRequest::SessionRegister {
                session_id,
                cwd,
                agent_role,
                feature,
            } => {
                assert_eq!(session_id, "sess-123");
                assert_eq!(cwd, "/workspace");
                assert_eq!(agent_role.as_deref(), Some("developer"));
                assert_eq!(feature.as_deref(), Some("col-006"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_session_close() {
        let req = HookRequest::SessionClose {
            session_id: "sess-123".to_string(),
            outcome: Some("success".to_string()),
            duration_secs: 3600,
        };
        let bytes = serialize_request(&req).unwrap();
        let decoded = deserialize_request(&bytes).unwrap();
        match decoded {
            HookRequest::SessionClose {
                session_id,
                outcome,
                duration_secs,
            } => {
                assert_eq!(session_id, "sess-123");
                assert_eq!(outcome.as_deref(), Some("success"));
                assert_eq!(duration_secs, 3600);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_record_event() {
        let event = ImplantEvent {
            event_type: "tool_use".to_string(),
            session_id: "sess-1".to_string(),
            timestamp: 1700000000,
            payload: serde_json::json!({"tool": "Read"}),
        };
        let req = HookRequest::RecordEvent { event };
        let bytes = serialize_request(&req).unwrap();
        let decoded = deserialize_request(&bytes).unwrap();
        match decoded {
            HookRequest::RecordEvent { event: ev } => {
                assert_eq!(ev.event_type, "tool_use");
                assert_eq!(ev.session_id, "sess-1");
                assert_eq!(ev.timestamp, 1700000000);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_record_events_batch() {
        let events = vec![
            ImplantEvent {
                event_type: "tool_use".to_string(),
                session_id: "sess-1".to_string(),
                timestamp: 1700000000,
                payload: serde_json::json!({}),
            },
            ImplantEvent {
                event_type: "context_read".to_string(),
                session_id: "sess-1".to_string(),
                timestamp: 1700000001,
                payload: serde_json::json!({"entry_id": 42}),
            },
        ];
        let req = HookRequest::RecordEvents { events };
        let bytes = serialize_request(&req).unwrap();
        let decoded = deserialize_request(&bytes).unwrap();
        match decoded {
            HookRequest::RecordEvents { events: evs } => {
                assert_eq!(evs.len(), 2);
                assert_eq!(evs[0].event_type, "tool_use");
                assert_eq!(evs[1].event_type, "context_read");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_pong_response() {
        let resp = HookResponse::Pong {
            server_version: "0.1.0".to_string(),
        };
        let bytes = serialize_response(&resp).unwrap();
        let decoded = deserialize_response(&bytes).unwrap();
        match decoded {
            HookResponse::Pong { server_version } => {
                assert_eq!(server_version, "0.1.0");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn round_trip_ack_response() {
        let resp = HookResponse::Ack;
        let bytes = serialize_response(&resp).unwrap();
        let decoded = deserialize_response(&bytes).unwrap();
        assert!(matches!(decoded, HookResponse::Ack));
    }

    #[test]
    fn round_trip_error_response() {
        let resp = HookResponse::Error {
            code: ERR_UID_MISMATCH,
            message: "uid mismatch".to_string(),
        };
        let bytes = serialize_response(&resp).unwrap();
        let decoded = deserialize_response(&bytes).unwrap();
        match decoded {
            HookResponse::Error { code, message } => {
                assert_eq!(code, ERR_UID_MISMATCH);
                assert_eq!(message, "uid mismatch");
            }
            _ => panic!("wrong variant"),
        }
    }

    // -- Frame round-trip tests --

    #[test]
    fn frame_round_trip() {
        let req = HookRequest::Ping;
        let payload = serialize_request(&req).unwrap();

        let mut buf = Vec::new();
        write_frame(&mut buf, &payload).unwrap();

        let mut cursor = Cursor::new(buf);
        let read_payload = read_frame(&mut cursor, MAX_PAYLOAD_SIZE).unwrap();
        let decoded = deserialize_request(&read_payload).unwrap();
        assert!(matches!(decoded, HookRequest::Ping));
    }

    #[test]
    fn frame_round_trip_session_register() {
        let req = HookRequest::SessionRegister {
            session_id: "s1".to_string(),
            cwd: "/work".to_string(),
            agent_role: None,
            feature: None,
        };
        let payload = serialize_request(&req).unwrap();

        let mut buf = Vec::new();
        write_frame(&mut buf, &payload).unwrap();

        let mut cursor = Cursor::new(buf);
        let read_payload = read_frame(&mut cursor, MAX_PAYLOAD_SIZE).unwrap();
        let decoded = deserialize_request(&read_payload).unwrap();
        match decoded {
            HookRequest::SessionRegister { session_id, .. } => {
                assert_eq!(session_id, "s1");
            }
            _ => panic!("wrong variant"),
        }
    }

    // -- write_frame error tests --

    #[test]
    fn write_frame_rejects_oversized_payload() {
        let payload = vec![0u8; MAX_PAYLOAD_SIZE + 1];
        let mut buf = Vec::new();
        let result = write_frame(&mut buf, &payload);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn write_frame_accepts_exactly_max() {
        let payload = vec![0u8; MAX_PAYLOAD_SIZE];
        let mut buf = Vec::new();
        let result = write_frame(&mut buf, &payload);
        assert!(result.is_ok());
        // Verify header
        assert_eq!(buf.len(), FRAME_HEADER_SIZE + MAX_PAYLOAD_SIZE);
        let len = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        assert_eq!(len as usize, MAX_PAYLOAD_SIZE);
    }

    // -- read_frame error tests --

    #[test]
    fn read_frame_rejects_zero_length() {
        let header = 0u32.to_be_bytes();
        let mut cursor = Cursor::new(header.to_vec());
        let result = read_frame(&mut cursor, MAX_PAYLOAD_SIZE);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("empty payload"));
    }

    #[test]
    fn read_frame_rejects_oversized_length() {
        let big_len = (MAX_PAYLOAD_SIZE as u32) + 1;
        let header = big_len.to_be_bytes();
        let mut cursor = Cursor::new(header.to_vec());
        let result = read_frame(&mut cursor, MAX_PAYLOAD_SIZE);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("exceeds maximum"));
    }

    #[test]
    fn read_frame_partial_header_eof() {
        // Only 2 bytes instead of 4
        let mut cursor = Cursor::new(vec![0u8, 1]);
        let result = read_frame(&mut cursor, MAX_PAYLOAD_SIZE);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("connection closed during header read"));
    }

    #[test]
    fn read_frame_partial_payload_eof() {
        // Valid header saying 100 bytes, but only 10 bytes of payload
        let mut buf = Vec::new();
        buf.extend_from_slice(&100u32.to_be_bytes());
        buf.extend_from_slice(&[0u8; 10]);
        let mut cursor = Cursor::new(buf);
        let result = read_frame(&mut cursor, MAX_PAYLOAD_SIZE);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("connection closed during payload read"));
    }

    #[test]
    fn read_frame_empty_input() {
        let mut cursor = Cursor::new(Vec::<u8>::new());
        let result = read_frame(&mut cursor, MAX_PAYLOAD_SIZE);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("connection closed during header read"));
    }

    // -- Deserialization error tests --

    #[test]
    fn deserialize_request_invalid_utf8() {
        let result = deserialize_request(&[0xFF, 0xFE, 0xFD]);
        assert!(result.is_err());
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("codec error"));
    }

    #[test]
    fn deserialize_request_unknown_type_tag() {
        let json = br#"{"type":"UnknownVariant","data":"hello"}"#;
        let result = deserialize_request(json);
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_request_empty_json() {
        let result = deserialize_request(b"{}");
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_request_valid_ping() {
        let json = br#"{"type":"Ping"}"#;
        let req = deserialize_request(json).unwrap();
        assert!(matches!(req, HookRequest::Ping));
    }

    // -- HookInput defensive parsing tests (ADR-006) --

    #[test]
    fn hook_input_minimal_json() {
        let input: HookInput = serde_json::from_str("{}").unwrap();
        assert_eq!(input.hook_event_name, "");
        assert!(input.session_id.is_none());
        assert!(input.cwd.is_none());
        assert!(input.transcript_path.is_none());
        assert!(input.prompt.is_none());
    }

    #[test]
    fn hook_input_unknown_fields_captured() {
        let json = r#"{"hook_event_name":"Stop","unknown_field":"value","nested":{"a":1}}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.hook_event_name, "Stop");
        assert_eq!(input.extra["unknown_field"], "value");
        assert_eq!(input.extra["nested"]["a"], 1);
    }

    #[test]
    fn hook_input_all_fields() {
        let json = r#"{
            "hook_event_name": "PreToolUse",
            "session_id": "sess-abc",
            "cwd": "/home/user/project",
            "transcript_path": "/tmp/transcript.jsonl"
        }"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.hook_event_name, "PreToolUse");
        assert_eq!(input.session_id.as_deref(), Some("sess-abc"));
        assert_eq!(input.cwd.as_deref(), Some("/home/user/project"));
        assert_eq!(
            input.transcript_path.as_deref(),
            Some("/tmp/transcript.jsonl")
        );
    }

    #[test]
    fn hook_input_empty_string_fields() {
        let json = r#"{"hook_event_name":"","session_id":"","cwd":"","transcript_path":""}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.hook_event_name, "");
        assert_eq!(input.session_id.as_deref(), Some(""));
        assert_eq!(input.cwd.as_deref(), Some(""));
    }

    // -- TransportError From<io::Error> tests --

    #[test]
    fn transport_error_from_connection_refused() {
        let io_err = io::Error::new(io::ErrorKind::ConnectionRefused, "refused");
        let te = TransportError::from(io_err);
        assert!(matches!(te, TransportError::Unavailable(_)));
    }

    #[test]
    fn transport_error_from_not_found() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "not found");
        let te = TransportError::from(io_err);
        assert!(matches!(te, TransportError::Unavailable(_)));
    }

    #[test]
    fn transport_error_from_timed_out() {
        let io_err = io::Error::new(io::ErrorKind::TimedOut, "timed out");
        let te = TransportError::from(io_err);
        assert!(matches!(te, TransportError::Timeout(_)));
    }

    #[test]
    fn transport_error_from_other_io() {
        let io_err = io::Error::new(io::ErrorKind::BrokenPipe, "broken pipe");
        let te = TransportError::from(io_err);
        assert!(matches!(te, TransportError::Transport(_)));
    }

    // -- Error display tests --

    #[test]
    fn transport_error_display() {
        let err = TransportError::Unavailable("no socket".to_string());
        assert!(format!("{err}").contains("server unavailable"));

        let err = TransportError::Timeout(Duration::from_millis(100));
        assert!(format!("{err}").contains("timed out"));

        let err = TransportError::Rejected {
            code: -32001,
            message: "bad uid".to_string(),
        };
        assert!(format!("{err}").contains("-32001"));

        let err = TransportError::Codec("bad json".to_string());
        assert!(format!("{err}").contains("codec error"));

        let err = TransportError::Transport("broken pipe".to_string());
        assert!(format!("{err}").contains("transport error"));
    }

    // -- Error code constant tests --

    #[test]
    fn error_codes_negative() {
        assert!(ERR_UID_MISMATCH < 0);
        assert!(ERR_LINEAGE_FAILED < 0);
        assert!(ERR_UNKNOWN_REQUEST < 0);
        assert!(ERR_INVALID_PAYLOAD < 0);
        assert!(ERR_INTERNAL < 0);
    }

    #[test]
    fn error_codes_unique() {
        let codes = [
            ERR_UID_MISMATCH,
            ERR_LINEAGE_FAILED,
            ERR_UNKNOWN_REQUEST,
            ERR_INVALID_PAYLOAD,
            ERR_INTERNAL,
        ];
        for i in 0..codes.len() {
            for j in (i + 1)..codes.len() {
                assert_ne!(codes[i], codes[j], "codes at {i} and {j} are equal");
            }
        }
    }

    // -- Serde tag verification --

    #[test]
    fn serde_tag_present_in_json() {
        let req = HookRequest::Ping;
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""type":"Ping"#));
    }

    #[test]
    fn serde_tag_session_register() {
        let req = HookRequest::SessionRegister {
            session_id: "s1".to_string(),
            cwd: "/w".to_string(),
            agent_role: None,
            feature: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains(r#""type":"SessionRegister"#));
    }

    #[test]
    fn serde_tag_response_pong() {
        let resp = HookResponse::Pong {
            server_version: "0.1.0".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""type":"Pong"#));
    }

    #[test]
    fn serde_tag_response_error() {
        let resp = HookResponse::Error {
            code: ERR_INTERNAL,
            message: "fail".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""type":"Error"#));
    }

    // -- Multiple frames in sequence --

    #[test]
    fn multiple_frames_in_sequence() {
        let mut buf = Vec::new();

        let req1 = HookRequest::Ping;
        let payload1 = serialize_request(&req1).unwrap();
        write_frame(&mut buf, &payload1).unwrap();

        let req2 = HookRequest::SessionClose {
            session_id: "s1".to_string(),
            outcome: None,
            duration_secs: 0,
        };
        let payload2 = serialize_request(&req2).unwrap();
        write_frame(&mut buf, &payload2).unwrap();

        let mut cursor = Cursor::new(buf);
        let read1 = read_frame(&mut cursor, MAX_PAYLOAD_SIZE).unwrap();
        let decoded1 = deserialize_request(&read1).unwrap();
        assert!(matches!(decoded1, HookRequest::Ping));

        let read2 = read_frame(&mut cursor, MAX_PAYLOAD_SIZE).unwrap();
        let decoded2 = deserialize_request(&read2).unwrap();
        assert!(matches!(decoded2, HookRequest::SessionClose { .. }));
    }

    // -- ImplantEvent serialization --

    #[test]
    fn implant_event_round_trip() {
        let event = ImplantEvent {
            event_type: "tool_use".to_string(),
            session_id: "sess-1".to_string(),
            timestamp: 1700000000,
            payload: serde_json::json!({"tool": "Bash", "duration_ms": 150}),
        };
        let bytes = serde_json::to_vec(&event).unwrap();
        let decoded: ImplantEvent = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.event_type, "tool_use");
        assert_eq!(decoded.payload["tool"], "Bash");
        assert_eq!(decoded.payload["duration_ms"], 150);
    }

    // -- EntryPayload serialization --

    #[test]
    fn entry_payload_round_trip() {
        let entry = EntryPayload {
            id: 42,
            title: "Test Entry".to_string(),
            content: "Some content".to_string(),
            confidence: 0.85,
            similarity: 0.92,
            category: "decision".to_string(),
        };
        let bytes = serde_json::to_vec(&entry).unwrap();
        let decoded: EntryPayload = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.id, 42);
        assert_eq!(decoded.title, "Test Entry");
        assert!((decoded.confidence - 0.85).abs() < f64::EPSILON);
    }

    // -- HookInput.prompt field tests (col-007) --

    #[test]
    fn hook_input_with_prompt() {
        let json = r#"{"hook_event_name":"UserPromptSubmit","prompt":"test query"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.prompt.as_deref(), Some("test query"));
    }

    #[test]
    fn hook_input_without_prompt() {
        let json = r#"{"hook_event_name":"SessionStart"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert!(input.prompt.is_none());
    }

    #[test]
    fn hook_input_empty_prompt() {
        let json = r#"{"hook_event_name":"UserPromptSubmit","prompt":""}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.prompt.as_deref(), Some(""));
    }

    #[test]
    fn hook_input_prompt_with_unknown_fields() {
        let json = r#"{"hook_event_name":"Test","prompt":"q","custom":"val"}"#;
        let input: HookInput = serde_json::from_str(json).unwrap();
        assert_eq!(input.prompt.as_deref(), Some("q"));
        assert_eq!(input.extra["custom"], "val");
    }

    // -- ContextSearch round-trip (col-007: dead_code removed) --

    #[test]
    fn round_trip_context_search() {
        let req = HookRequest::ContextSearch {
            query: "test query".to_string(),
            session_id: None,
            role: Some("developer".to_string()),
            task: None,
            feature: None,
            k: Some(5),
            max_tokens: None,
        };
        let bytes = serialize_request(&req).unwrap();
        let decoded = deserialize_request(&bytes).unwrap();
        match decoded {
            HookRequest::ContextSearch {
                query,
                session_id,
                role,
                k,
                ..
            } => {
                assert_eq!(query, "test query");
                assert!(session_id.is_none());
                assert_eq!(role.as_deref(), Some("developer"));
                assert_eq!(k, Some(5));
            }
            _ => panic!("expected ContextSearch"),
        }
    }

    // -- col-008: ContextSearch session_id tests --

    #[test]
    fn context_search_with_session_id() {
        let req = HookRequest::ContextSearch {
            query: "test".to_string(),
            session_id: Some("sess-1".to_string()),
            role: None,
            task: None,
            feature: None,
            k: None,
            max_tokens: None,
        };
        let bytes = serialize_request(&req).unwrap();
        let decoded = deserialize_request(&bytes).unwrap();
        match decoded {
            HookRequest::ContextSearch { session_id, .. } => {
                assert_eq!(session_id.as_deref(), Some("sess-1"));
            }
            _ => panic!("expected ContextSearch"),
        }
    }

    #[test]
    fn context_search_missing_session_id_field_defaults_none() {
        // Simulate a JSON payload without the session_id field (backward compat)
        let json = br#"{"type":"ContextSearch","query":"test"}"#;
        let decoded = deserialize_request(json).unwrap();
        match decoded {
            HookRequest::ContextSearch { session_id, .. } => {
                assert!(session_id.is_none());
            }
            _ => panic!("expected ContextSearch"),
        }
    }

    // -- col-008: CompactPayload round-trip tests --

    #[test]
    fn round_trip_compact_payload() {
        let req = HookRequest::CompactPayload {
            session_id: "s1".to_string(),
            injected_entry_ids: vec![1, 2, 3],
            role: Some("developer".to_string()),
            feature: None,
            token_limit: Some(500),
        };
        let bytes = serialize_request(&req).unwrap();
        let decoded = deserialize_request(&bytes).unwrap();
        match decoded {
            HookRequest::CompactPayload {
                session_id,
                injected_entry_ids,
                role,
                feature,
                token_limit,
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(injected_entry_ids, vec![1, 2, 3]);
                assert_eq!(role.as_deref(), Some("developer"));
                assert!(feature.is_none());
                assert_eq!(token_limit, Some(500));
            }
            _ => panic!("expected CompactPayload"),
        }
    }

    #[test]
    fn compact_payload_empty_entry_ids() {
        let req = HookRequest::CompactPayload {
            session_id: "s1".to_string(),
            injected_entry_ids: vec![],
            role: None,
            feature: None,
            token_limit: None,
        };
        let bytes = serialize_request(&req).unwrap();
        let decoded = deserialize_request(&bytes).unwrap();
        match decoded {
            HookRequest::CompactPayload {
                injected_entry_ids, ..
            } => {
                assert!(injected_entry_ids.is_empty());
            }
            _ => panic!("expected CompactPayload"),
        }
    }

    // -- col-008: BriefingContent round-trip tests --

    #[test]
    fn round_trip_briefing_content() {
        let resp = HookResponse::BriefingContent {
            content: "test content".to_string(),
            token_count: 25,
        };
        let bytes = serialize_response(&resp).unwrap();
        let decoded = deserialize_response(&bytes).unwrap();
        match decoded {
            HookResponse::BriefingContent {
                content,
                token_count,
            } => {
                assert_eq!(content, "test content");
                assert_eq!(token_count, 25);
            }
            _ => panic!("expected BriefingContent"),
        }
    }

    #[test]
    fn briefing_content_empty() {
        let resp = HookResponse::BriefingContent {
            content: String::new(),
            token_count: 0,
        };
        let bytes = serialize_response(&resp).unwrap();
        let decoded = deserialize_response(&bytes).unwrap();
        match decoded {
            HookResponse::BriefingContent {
                content,
                token_count,
            } => {
                assert!(content.is_empty());
                assert_eq!(token_count, 0);
            }
            _ => panic!("expected BriefingContent"),
        }
    }

    #[test]
    fn round_trip_entries_response() {
        let resp = HookResponse::Entries {
            items: vec![EntryPayload {
                id: 1,
                title: "Test".to_string(),
                content: "content".to_string(),
                confidence: 0.8,
                similarity: 0.9,
                category: "decision".to_string(),
            }],
            total_tokens: 10,
        };
        let bytes = serialize_response(&resp).unwrap();
        let decoded = deserialize_response(&bytes).unwrap();
        match decoded {
            HookResponse::Entries { items, total_tokens } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].id, 1);
                assert_eq!(total_tokens, 10);
            }
            _ => panic!("expected Entries"),
        }
    }
}
