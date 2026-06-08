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

// -- Event type values (free-form `ImplantEvent.event_type`) --

/// `event_type` value carrying a client-streamed raw transcript span. Routed by the
/// accept-and-drop guard (vnc-024 ADR-004). Carried in `ImplantEvent.payload` as
/// [`TranscriptDeltaPayload`] `{ offset, bytes }` — NOT a new wire variant.
///
/// Mirrors the existing event-type-as-routing pattern (col-022 ADR-001) so the
/// hook/listener coupling is not stringly-typed.
pub const TRANSCRIPT_DELTA_EVENT: &str = "transcript_delta";

// -- HookInput (Claude Code stdin JSON -- ADR-006) --

/// Represents the JSON blob that Claude Code pipes to hook processes on stdin.
///
/// All fields use `#[serde(default)]` for maximum defensive parsing per ADR-006.
/// Unknown fields are captured by the `extra` flatten field.
#[derive(Deserialize, Debug, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../bindings/"))]
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

    /// Originating LLM provider. Populated by hook::run() after normalize_event_name(),
    /// NOT from stdin JSON. `#[serde(default)]` ensures existing Claude Code hook JSON
    /// (which omits this field) deserializes to None without error (NFR-05, ADR-002).
    ///
    /// Valid values: "claude-code" | "gemini-cli" | "codex-cli" | None
    #[serde(default)]
    pub provider: Option<String>,

    /// Gemini CLI structured MCP context field. Present in BeforeTool and AfterTool
    /// payloads. Structure: { "server_name": str, "tool_name": str, "url": str }.
    /// Also captured by the `extra` flatten, but the named field enables typed access
    /// in build_cycle_event_or_fallthrough() without stringly-typed extra access (ADR-003).
    ///
    /// Claude Code and Codex payloads omit this field; serde(default) → None.
    #[serde(default)]
    pub mcp_context: Option<serde_json::Value>,

    /// Catch-all for unknown fields (forward compatibility).
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

// -- HookRequest (IPC wire protocol) --

/// Request sent from hook processes to the UDS listener.
///
/// Uses `#[serde(tag = "type")]` for JSON routing per ADR-005.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../bindings/"))]
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
        /// Originating hook event type; `None` is treated as `"UserPromptSubmit"` by
        /// `dispatch_request`. Set to `Some("SubagentStart")` by the SubagentStart arm
        /// in `hook.rs`. See ADR-001 crt-027.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        /// HTTP-`Accept` mirror for server-side preformatted sync responses (vnc-027
        /// ADR-001 §1). Set ONLY by `transport-uds.js` at serialization time, value
        /// `Some("text/plain")`. `hook.rs` construction sites pass `None` (mechanical
        /// edit, approved variance). `skip_serializing_if` keeps `None` absent on the
        /// wire, so frozen-hook frames stay byte-identical (AC-11).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accept: Option<String>,
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
        /// Reserved for col-010: once INJECTION_LOG persists to the database, the hook
        /// process can populate this from disk after a server restart, giving
        /// the server richer input than the briefing fallback. Currently empty
        /// (server tracks injection history in-memory via SessionRegistry).
        injected_entry_ids: Vec<u64>,
        role: Option<String>,
        feature: Option<String>,
        token_limit: Option<u32>,
        /// Forward-compatible transcript excerpt for PreCompact restoration (#670).
        /// Day 1: always None over HTTP; handle_compact_payload ignores it.
        /// Future clients (or hook-remote CLI) can populate this field to send
        /// transcript data for server-side restoration. See ADR-005 vnc-022.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transcript_excerpt: Option<String>,
        /// HTTP-`Accept` mirror for server-side preformatted sync responses (vnc-027
        /// ADR-001 §1). Set ONLY by `transport-uds.js` at serialization time, value
        /// `Some("text/plain")`. `hook.rs` construction sites pass `None` (mechanical
        /// edit, approved variance). `skip_serializing_if` keeps `None` absent on the
        /// wire, so frozen-hook frames stay byte-identical (AC-11).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        accept: Option<String>,
    },
}

// -- HookResponse (IPC wire protocol) --

/// Response sent from the UDS listener back to hook processes.
///
/// Uses `#[serde(tag = "type")]` for JSON routing per ADR-005.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../bindings/"))]
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
    BriefingContent { content: String, token_count: u32 },

    /// Server-side preformatted injection text for UDS sync callers that sent
    /// `accept` (vnc-027 ADR-001 §3). `body` is the exact bytes the HTTP text/plain
    /// path would return: an `Entries` result formats to
    /// `format_injection(items, MAX_INJECTION_BYTES)` output INCLUDING the
    /// load-bearing `--- Unimatrix Context ---\n` header; a `BriefingContent`
    /// result is `content` verbatim (no header). Returned ONLY to callers that sent
    /// `accept` — the frozen Rust hook never does, so it never receives `Text` and
    /// never fails to deserialize (ADR-001 §6 coupling, R-08). Additive: existing
    /// variants are unchanged and there is no `deny_unknown_fields`, so old
    /// serialized responses still deserialize.
    Text { body: String },
}

// -- ImplantEvent --

/// A single event recorded by a hook process.
///
/// transcript_delta precedence (vnc-024 FR-15 / SR-06, documentary only): when both a
/// streamed `transcript_delta` (`event_type == TRANSCRIPT_DELTA_EVENT`, `payload` =
/// [`TranscriptDeltaPayload`]) and a legacy `CompactPayload.transcript_excerpt` are present,
/// the streamed delta is the authoritative forward path and supersedes the excerpt (ass-069).
/// F1 documents this precedence; the merge logic is #670. No merge code is added here.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../bindings/"))]
pub struct ImplantEvent {
    /// Type of event (e.g., "tool_use", "context_read").
    pub event_type: String,

    /// Session that generated the event.
    pub session_id: String,

    /// Unix timestamp (seconds since epoch).
    pub timestamp: u64,

    /// Event-specific data.
    pub payload: serde_json::Value,

    /// Hook-side topic signal extracted from event content (col-017).
    /// Used for session-level feature attribution via majority vote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic_signal: Option<String>,

    /// Provider identity propagated from HookInput.provider through normalization.
    /// Non-None for all events processed through the normalization layer.
    /// None for events deserialized from wire frames that predate vnc-013 (backward compat).
    ///
    /// `skip_serializing_if = "Option::is_none"`: Claude Code events without --provider
    /// produce wire frames without this field; the listener handles missing field as None.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
}

// -- EntryPayload (stub for future search results) --

/// A knowledge entry returned in search/briefing results.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../bindings/"))]
pub struct EntryPayload {
    pub id: u64,
    pub title: String,
    pub content: String,
    pub confidence: f64,
    pub similarity: f64,
    pub category: String,
}

// -- TranscriptDeltaPayload (vnc-024 ADR-001/ADR-004) --

/// Typed payload for a `transcript_delta` event (`event_type == TRANSCRIPT_DELTA_EVENT`).
///
/// A client-streamed raw transcript span: `offset` is the byte offset of this span within
/// the session transcript and `bytes` is the verbatim text. This is **not** a new wire
/// carrier — the value still rides [`ImplantEvent::payload`] (`serde_json::Value`) unchanged.
/// The struct exists to (a) give the one genuinely-new field a typed cross-language binding
/// (`bindings/TranscriptDeltaPayload.ts`) so the TS client does not hand-mirror it, and
/// (b) be the deserialization shape the accept-and-drop guard parses into (vnc-024 ADR-004).
///
/// Raw transcript bytes may contain secrets; they are accepted-and-dropped, never persisted
/// (principle 8). The 6th ts-rs export. Verified dual-sided (Rust↔TS) by AC-11.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(test, derive(ts_rs::TS))]
#[cfg_attr(test, ts(export, export_to = "../bindings/"))]
pub struct TranscriptDeltaPayload {
    /// Byte offset of this span within the session transcript.
    pub offset: u64,
    /// Verbatim transcript text for this span.
    pub bytes: String,
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
    use ts_rs::TS;

    // -- vnc-024 Component 1: ts-rs codegen sentinel (AC-02) --

    /// Force-export all six `#[ts(export)]` wire types and assert each committed
    /// `bindings/<Name>.ts` exists and is non-empty after `cargo test` (AC-02 / FR-02).
    ///
    /// `export_all()` writes the type plus its transitive dependencies to its `export_to`
    /// path. Referencing all six explicitly guarantees a partial build cannot skip one so
    /// the CI diff-gate (`git diff --exit-code bindings/`) compares against full output.
    #[test]
    fn test_export_bindings_all_six_written_and_nonempty() {
        let cfg = ts_rs::Config::default();
        HookInput::export_all(&cfg).expect("export HookInput bindings");
        HookRequest::export_all(&cfg).expect("export HookRequest bindings");
        HookResponse::export_all(&cfg).expect("export HookResponse bindings");
        ImplantEvent::export_all(&cfg).expect("export ImplantEvent bindings");
        EntryPayload::export_all(&cfg).expect("export EntryPayload bindings");
        TranscriptDeltaPayload::export_all(&cfg).expect("export TranscriptDeltaPayload bindings");

        let bindings_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/bindings");
        for name in [
            "HookInput",
            "HookRequest",
            "HookResponse",
            "ImplantEvent",
            "EntryPayload",
            "TranscriptDeltaPayload",
        ] {
            let path = std::path::Path::new(bindings_dir).join(format!("{name}.ts"));
            let meta = std::fs::metadata(&path)
                .unwrap_or_else(|e| panic!("binding {} must exist: {e}", path.display()));
            assert!(
                meta.len() > 0,
                "binding {} must be non-empty",
                path.display()
            );
        }
    }

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
            topic_signal: None,
            provider: None,
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
                topic_signal: None,
                provider: None,
            },
            ImplantEvent {
                event_type: "context_read".to_string(),
                session_id: "sess-1".to_string(),
                timestamp: 1700000001,
                payload: serde_json::json!({"entry_id": 42}),
                topic_signal: None,
                provider: None,
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
            topic_signal: None,
            provider: None,
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
            source: None,
            accept: None,
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
            source: None,
            accept: None,
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
            transcript_excerpt: None,
            accept: None,
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
                transcript_excerpt,
                accept: _,
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(injected_entry_ids, vec![1, 2, 3]);
                assert_eq!(role.as_deref(), Some("developer"));
                assert!(feature.is_none());
                assert_eq!(token_limit, Some(500));
                assert!(transcript_excerpt.is_none());
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
            transcript_excerpt: None,
            accept: None,
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

    // -- vnc-022: CompactPayload transcript_excerpt tests (ADR-005) --

    #[test]
    fn test_compact_payload_with_transcript_excerpt_round_trip() {
        // AC-09, R-07: round-trip with transcript_excerpt present
        let req = HookRequest::CompactPayload {
            session_id: "s1".to_string(),
            injected_entry_ids: vec![1, 2],
            role: Some("developer".to_string()),
            feature: Some("vnc-022".to_string()),
            token_limit: Some(1000),
            transcript_excerpt: Some("excerpt text".to_string()),
            accept: None,
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
                transcript_excerpt,
                accept: _,
            } => {
                assert_eq!(session_id, "s1");
                assert_eq!(injected_entry_ids, vec![1, 2]);
                assert_eq!(role.as_deref(), Some("developer"));
                assert_eq!(feature.as_deref(), Some("vnc-022"));
                assert_eq!(token_limit, Some(1000));
                assert_eq!(transcript_excerpt, Some("excerpt text".to_string()));
            }
            _ => panic!("expected CompactPayload"),
        }
    }

    #[test]
    fn test_compact_payload_without_transcript_excerpt_defaults_to_none() {
        // R-07: missing field deserializes to None via serde(default)
        let json = r#"{"type":"CompactPayload","session_id":"s1","injected_entry_ids":[],"role":null,"feature":null,"token_limit":null}"#;
        let decoded: HookRequest = serde_json::from_str(json).unwrap();
        match decoded {
            HookRequest::CompactPayload {
                transcript_excerpt, ..
            } => {
                assert!(
                    transcript_excerpt.is_none(),
                    "transcript_excerpt must be None when key is absent"
                );
            }
            _ => panic!("expected CompactPayload"),
        }
    }

    #[test]
    fn test_compact_payload_none_transcript_excerpt_omitted_from_json() {
        // R-07: skip_serializing_if = "Option::is_none" must omit field from JSON
        let req = HookRequest::CompactPayload {
            session_id: "s1".to_string(),
            injected_entry_ids: vec![],
            role: None,
            feature: None,
            token_limit: None,
            transcript_excerpt: None,
            accept: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(
            !json.contains("transcript_excerpt"),
            "transcript_excerpt: None must not appear in serialized JSON; got: {json}"
        );
    }

    #[test]
    fn test_compact_payload_transcript_excerpt_null_deserializes_to_none() {
        // R-07: explicit null value deserializes to None
        let json = r#"{"type":"CompactPayload","session_id":"s1","injected_entry_ids":[],"transcript_excerpt":null}"#;
        let decoded: HookRequest = serde_json::from_str(json).unwrap();
        match decoded {
            HookRequest::CompactPayload {
                transcript_excerpt, ..
            } => {
                assert!(
                    transcript_excerpt.is_none(),
                    "transcript_excerpt: null must deserialize to None"
                );
            }
            _ => panic!("expected CompactPayload"),
        }
    }

    #[test]
    fn test_compact_payload_transcript_excerpt_empty_string() {
        // Edge case: empty string is valid, not None
        let json = r#"{"type":"CompactPayload","session_id":"s1","injected_entry_ids":[],"transcript_excerpt":""}"#;
        let decoded: HookRequest = serde_json::from_str(json).unwrap();
        match decoded {
            HookRequest::CompactPayload {
                transcript_excerpt, ..
            } => {
                assert_eq!(
                    transcript_excerpt,
                    Some(String::new()),
                    "empty string must be Some(\"\"), not None"
                );
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
            HookResponse::Entries {
                items,
                total_tokens,
            } => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0].id, 1);
                assert_eq!(total_tokens, 10);
            }
            _ => panic!("expected Entries"),
        }
    }

    // -- col-017: ImplantEvent topic_signal serde tests (T-05) --

    #[test]
    fn implant_event_without_topic_signal_deserializes_to_none() {
        // AC-06: backward compat -- old JSON without field
        let json = r#"{"event_type":"tool_use","session_id":"s1","timestamp":100,"payload":{}}"#;
        let event: ImplantEvent = serde_json::from_str(json).unwrap();
        assert!(event.topic_signal.is_none());
    }

    #[test]
    fn implant_event_with_topic_signal_deserializes() {
        // AC-07: new JSON with field
        let json = r#"{"event_type":"tool_use","session_id":"s1","timestamp":100,"payload":{},"topic_signal":"col-017"}"#;
        let event: ImplantEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.topic_signal.as_deref(), Some("col-017"));
    }

    #[test]
    fn implant_event_with_null_topic_signal_deserializes_to_none() {
        let json = r#"{"event_type":"tool_use","session_id":"s1","timestamp":100,"payload":{},"topic_signal":null}"#;
        let event: ImplantEvent = serde_json::from_str(json).unwrap();
        assert!(event.topic_signal.is_none());
    }

    #[test]
    fn implant_event_serialize_none_omits_field() {
        let event = ImplantEvent {
            event_type: "tool_use".to_string(),
            session_id: "s1".to_string(),
            timestamp: 100,
            payload: serde_json::json!({}),
            topic_signal: None,
            provider: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            !json.contains("topic_signal"),
            "None should omit the field: {json}"
        );
    }

    #[test]
    fn implant_event_serialize_some_includes_field() {
        let event = ImplantEvent {
            event_type: "tool_use".to_string(),
            session_id: "s1".to_string(),
            timestamp: 100,
            payload: serde_json::json!({}),
            topic_signal: Some("col-017".to_string()),
            provider: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(
            json.contains("topic_signal"),
            "Some should include the field: {json}"
        );
        assert!(json.contains("col-017"));
    }

    // -- crt-027: ContextSearch source field tests (ADR-001) --

    #[test]
    fn wire_context_search_source_absent_deserializes_to_none() {
        // R-01 scenario 1, AC-05a: JSON without `source` key deserializes to None (backward compat)
        let json = br#"{"type":"ContextSearch","query":"design the hook","session_id":null,"role":null,"task":null,"feature":null,"k":null,"max_tokens":null}"#;
        let decoded = deserialize_request(json).unwrap();
        match decoded {
            HookRequest::ContextSearch { source, .. } => {
                assert!(source.is_none(), "source should be None when key is absent");
            }
            _ => panic!("expected ContextSearch"),
        }
    }

    #[test]
    fn wire_context_search_source_present_deserializes_to_value() {
        // R-01 scenario 2: JSON with `"source": "SubagentStart"` deserializes correctly
        let json = br#"{"type":"ContextSearch","query":"subagent query","source":"SubagentStart"}"#;
        let decoded = deserialize_request(json).unwrap();
        match decoded {
            HookRequest::ContextSearch { source, .. } => {
                assert_eq!(
                    source.as_deref(),
                    Some("SubagentStart"),
                    "source should be Some(\"SubagentStart\")"
                );
            }
            _ => panic!("expected ContextSearch"),
        }
    }

    #[test]
    fn wire_context_search_source_none_serializes_without_field() {
        // R-01 scenario 3: source: None must not emit the `source` key in serialized JSON
        let req = HookRequest::ContextSearch {
            query: "test".to_string(),
            session_id: None,
            role: None,
            task: None,
            feature: None,
            k: None,
            max_tokens: None,
            source: None,
            accept: None,
        };
        let bytes = serialize_request(&req).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            !value.as_object().unwrap().contains_key("source"),
            "source: None should omit the key from serialized JSON; got: {value}"
        );
    }

    #[test]
    fn context_search_source_none_round_trip() {
        // R-01 scenario 3 (round-trip): None survives serialize → deserialize
        let req = HookRequest::ContextSearch {
            query: "test".to_string(),
            session_id: None,
            role: None,
            task: None,
            feature: None,
            k: None,
            max_tokens: None,
            source: None,
            accept: None,
        };
        let bytes = serialize_request(&req).unwrap();
        let decoded = deserialize_request(&bytes).unwrap();
        match decoded {
            HookRequest::ContextSearch { source, .. } => {
                assert!(source.is_none(), "source should round-trip as None");
            }
            _ => panic!("expected ContextSearch"),
        }
    }

    #[test]
    fn context_search_source_subagentstart_round_trip() {
        // R-01 scenario 2 (round-trip): Some("SubagentStart") survives serialize → deserialize
        let req = HookRequest::ContextSearch {
            query: "subagent task".to_string(),
            session_id: Some("sess-sa".to_string()),
            role: None,
            task: None,
            feature: None,
            k: None,
            max_tokens: None,
            source: Some("SubagentStart".to_string()),
            accept: None,
        };
        let bytes = serialize_request(&req).unwrap();
        let decoded = deserialize_request(&bytes).unwrap();
        match decoded {
            HookRequest::ContextSearch { source, .. } => {
                assert_eq!(
                    source.as_deref(),
                    Some("SubagentStart"),
                    "source should round-trip as Some(\"SubagentStart\")"
                );
            }
            _ => panic!("expected ContextSearch"),
        }
    }

    #[test]
    fn hook_request_briefing_variant_still_present() {
        // R-13, C-04: HookRequest::Briefing variant must not be removed by crt-027
        let req = HookRequest::Briefing {
            role: "developer".to_string(),
            task: "implement feature".to_string(),
            feature: None,
            max_tokens: None,
        };
        assert!(
            matches!(req, HookRequest::Briefing { .. }),
            "HookRequest::Briefing variant must still be present"
        );
    }

    // -- vnc-013: wire-protocol new field tests --

    #[test]
    fn test_hook_input_deserializes_without_new_fields() {
        // NFR-05, AC-08: minimal Claude Code hook JSON omits provider and mcp_context.
        // serde(default) must produce None for both, with no deserialization error.
        let json = r#"{"hook_event_name":"PreToolUse","session_id":"sess-1"}"#;
        let input: HookInput = serde_json::from_str(json).expect("deserialize");
        assert_eq!(input.provider, None);
        assert_eq!(input.mcp_context, None);
        assert_eq!(input.hook_event_name, "PreToolUse");
    }

    #[test]
    fn test_hook_input_deserializes_with_provider_field() {
        // AC-17: Codex-style payload with explicit provider field.
        let json = r#"{
            "hook_event_name": "PreToolUse",
            "provider": "codex-cli"
        }"#;
        let input: HookInput = serde_json::from_str(json).expect("deserialize");
        assert_eq!(input.provider, Some("codex-cli".to_string()));
        assert_eq!(input.mcp_context, None);
    }

    #[test]
    fn test_hook_input_deserializes_gemini_payload_with_mcp_context() {
        // AC-14: Gemini BeforeTool payload with mcp_context structured field.
        // This is the serde foundation for R-01 (mcp_context.tool_name promotion).
        let json = r#"{
            "hook_event_name": "BeforeTool",
            "mcp_context": {
                "server_name": "unimatrix",
                "tool_name": "context_cycle",
                "url": "http://localhost:3000"
            }
        }"#;
        let input: HookInput = serde_json::from_str(json).expect("deserialize");
        assert!(input.mcp_context.is_some());
        let mcp = input.mcp_context.as_ref().unwrap();
        assert_eq!(
            mcp.get("tool_name").and_then(|v| v.as_str()),
            Some("context_cycle")
        );
        assert_eq!(input.provider, None); // not in payload — inference applies
    }

    #[test]
    fn test_hook_input_mcp_context_non_object_deserializes() {
        // NFR-04 edge case: mcp_context present but is a string, not an object.
        // Must deserialize without error; promotion adapter handles gracefully via as_object().
        let json = r#"{
            "hook_event_name": "BeforeTool",
            "mcp_context": "unexpected-string"
        }"#;
        let input: HookInput = serde_json::from_str(json).expect("deserialize — must not error");
        assert!(input.mcp_context.is_some());
        // as_object() on a string Value returns None — no panic
        assert!(input.mcp_context.as_ref().unwrap().as_object().is_none());
    }

    #[test]
    fn test_mcp_context_not_duplicated_in_extra() {
        // ADR-003 correctness: named field mcp_context takes priority over flatten.
        // After deserialization, extra must NOT also contain an mcp_context key.
        let json = r#"{
            "hook_event_name": "BeforeTool",
            "mcp_context": {
                "server_name": "unimatrix",
                "tool_name": "context_cycle",
                "url": "http://localhost:3000"
            }
        }"#;
        let input: HookInput = serde_json::from_str(json).expect("deserialize");
        assert!(
            input.extra.get("mcp_context").is_none(),
            "mcp_context must not appear in extra flatten when named field captures it; extra={:?}",
            input.extra
        );
    }

    #[test]
    fn test_implant_event_deserializes_without_provider() {
        // R-13 secondary, R-02 degraded path: legacy ImplantEvent JSON without provider field.
        // serde(default) must produce None without error.
        let json = r#"{
            "event_type": "PreToolUse",
            "session_id": "sess-1",
            "timestamp": 1700000000,
            "payload": {}
        }"#;
        let event: ImplantEvent = serde_json::from_str(json).expect("deserialize");
        assert_eq!(event.provider, None);
        assert_eq!(event.event_type, "PreToolUse");
    }

    #[test]
    fn test_implant_event_provider_present_serializes() {
        // AC-05: provider field IS present in JSON when Some.
        let event = ImplantEvent {
            event_type: "PreToolUse".to_string(),
            session_id: "sess-1".to_string(),
            timestamp: 1700000000,
            payload: serde_json::json!({}),
            topic_signal: None,
            provider: Some("gemini-cli".to_string()),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.get("provider").and_then(|v| v.as_str()),
            Some("gemini-cli")
        );
        // Round-trip
        let decoded: ImplantEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.provider, Some("gemini-cli".to_string()));
    }

    #[test]
    fn test_implant_event_provider_none_not_serialized() {
        // NFR-05: skip_serializing_if = "Option::is_none" — provider key must be absent
        // from JSON when None, preserving backward compat for Claude Code consumers.
        let event = ImplantEvent {
            event_type: "PreToolUse".to_string(),
            session_id: "sess-1".to_string(),
            timestamp: 1700000000,
            payload: serde_json::json!({}),
            topic_signal: None,
            provider: None,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(
            parsed.get("provider").is_none(),
            "provider: None must not be serialized (skip_serializing_if); json={json}"
        );
    }

    #[test]
    fn test_hook_input_provider_none_when_absent() {
        // NFR-05: JSON without provider field → provider == None.
        let json = r#"{"hook_event_name":"SessionStart"}"#;
        let input: HookInput = serde_json::from_str(json).expect("deserialize");
        assert_eq!(input.provider, None);
    }

    // -- vnc-024 Component 2: round-trip fixtures + node-harness contract (ADR-002) --
    //
    // The fixture (committed JSON under bindings/fixtures/) — not the generated `.ts` — is the
    // contract authority. The Rust half here EMITS the fixtures from typed values and asserts
    // serde BEHAVIOR (tagged discriminant, None-vs-omission dual-direction, flatten, dual-sided
    // delta). The node harness (bindings/contract.test.mjs) consumes the SAME committed fixtures
    // and asserts the consuming-language side. A fixture is the contract only if both runtimes agree.

    /// Absolute path to the committed fixtures directory (sibling of the generated `.ts`).
    fn fixtures_dir() -> std::path::PathBuf {
        std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/bindings/fixtures"))
            .to_path_buf()
    }

    /// Read a committed fixture by file name (e.g. `"request_ping.json"`).
    fn read_fixture(name: &str) -> String {
        let path = fixtures_dir().join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("fixture {} must exist: {e}", path.display()))
    }

    /// Write a fixture atomically (temp file + rename). The reader tests and the emitter run in the
    /// same lib test binary concurrently; a non-atomic write would let a reader observe a truncated
    /// file. `rename` within the same dir is atomic on the platforms we target, so a concurrent
    /// reader always sees either the prior committed bytes or the full new bytes — never a partial.
    fn write_fixture_atomic(dir: &std::path::Path, name: &str, contents: &str) {
        let final_path = dir.join(name);
        let tmp_path = dir.join(format!(".{name}.{}.tmp", std::process::id()));
        std::fs::write(&tmp_path, contents).expect("write temp fixture");
        std::fs::rename(&tmp_path, &final_path).expect("atomic rename fixture");
    }

    /// Offset value used by the delta fixture. Kept well within 2^53 so the JSON number
    /// round-trips losslessly through Node's `number` (the ts-rs `bigint` annotation is a
    /// compile-time type only; the wire form is a plain JSON integer). Non-trivial, non-zero.
    const DELTA_OFFSET: u64 = 4_294_967_296; // 2^32 — > u32 range, << 2^53
    const DELTA_BYTES: &str = "user: explain the auth flow\nassistant: tok sk-NOTAKEY example span";

    /// Build one named value per `HookRequest`/`HookResponse` variant plus the serde edge cases.
    /// Field values are NON-TRIVIAL (not all-None) so a partial wiring cannot pass on an empty path.
    fn request_fixtures() -> Vec<(&'static str, HookRequest)> {
        vec![
            ("request_ping", HookRequest::Ping),
            (
                "request_session_register",
                HookRequest::SessionRegister {
                    session_id: "sess-fixture".to_string(),
                    cwd: "/workspace/unimatrix".to_string(),
                    agent_role: Some("developer".to_string()),
                    feature: Some("vnc-024".to_string()),
                },
            ),
            (
                "request_session_close",
                HookRequest::SessionClose {
                    session_id: "sess-fixture".to_string(),
                    outcome: Some("success".to_string()),
                    duration_secs: 3600,
                },
            ),
            (
                // topic_signal + provider PRESENT (non-trivial) — emit/parse the skip_serializing_if keys.
                "request_record_event",
                HookRequest::RecordEvent {
                    event: ImplantEvent {
                        event_type: "tool_use".to_string(),
                        session_id: "sess-fixture".to_string(),
                        timestamp: 1_700_000_000,
                        payload: serde_json::json!({ "tool": "Read", "path": "src/wire.rs" }),
                        topic_signal: Some("vnc-024".to_string()),
                        provider: Some("claude-code".to_string()),
                    },
                },
            ),
            (
                // topic_signal + provider ABSENT (None-vs-omission, parse-default side).
                "request_record_event_omitted",
                HookRequest::RecordEvent {
                    event: ImplantEvent {
                        event_type: "tool_use".to_string(),
                        session_id: "sess-fixture".to_string(),
                        timestamp: 1_700_000_000,
                        payload: serde_json::json!({ "tool": "Read" }),
                        topic_signal: None,
                        provider: None,
                    },
                },
            ),
            (
                "request_record_events",
                HookRequest::RecordEvents {
                    events: vec![
                        ImplantEvent {
                            event_type: "tool_use".to_string(),
                            session_id: "sess-fixture".to_string(),
                            timestamp: 1_700_000_000,
                            payload: serde_json::json!({ "tool": "Bash" }),
                            topic_signal: Some("vnc-024".to_string()),
                            provider: Some("claude-code".to_string()),
                        },
                        ImplantEvent {
                            event_type: "context_read".to_string(),
                            session_id: "sess-fixture".to_string(),
                            timestamp: 1_700_000_001,
                            payload: serde_json::json!({ "entry_id": 42 }),
                            topic_signal: None,
                            provider: None,
                        },
                    ],
                },
            ),
            (
                // source PRESENT (skip_serializing_if dual-direction, present side).
                "request_context_search",
                HookRequest::ContextSearch {
                    query: "explain the auth flow".to_string(),
                    session_id: Some("sess-fixture".to_string()),
                    role: Some("developer".to_string()),
                    task: Some("implement contract fixtures".to_string()),
                    feature: Some("vnc-024".to_string()),
                    k: Some(5),
                    max_tokens: Some(1500),
                    source: Some("SubagentStart".to_string()),
                    accept: None,
                },
            ),
            (
                // source ABSENT.
                "request_context_search_no_source",
                HookRequest::ContextSearch {
                    query: "explain the auth flow".to_string(),
                    session_id: Some("sess-fixture".to_string()),
                    role: Some("developer".to_string()),
                    task: None,
                    feature: Some("vnc-024".to_string()),
                    k: Some(5),
                    max_tokens: Some(1500),
                    source: None,
                    accept: None,
                },
            ),
            (
                "request_briefing",
                HookRequest::Briefing {
                    role: "developer".to_string(),
                    task: "implement feature".to_string(),
                    feature: Some("vnc-024".to_string()),
                    max_tokens: Some(2000),
                },
            ),
            (
                // transcript_excerpt PRESENT.
                "request_compact_payload",
                HookRequest::CompactPayload {
                    session_id: "sess-fixture".to_string(),
                    injected_entry_ids: vec![1, 2, 3],
                    role: Some("developer".to_string()),
                    feature: Some("vnc-024".to_string()),
                    token_limit: Some(1000),
                    transcript_excerpt: Some("prior excerpt text".to_string()),
                    accept: None,
                },
            ),
            (
                // transcript_excerpt ABSENT.
                "request_compact_payload_no_excerpt",
                HookRequest::CompactPayload {
                    session_id: "sess-fixture".to_string(),
                    injected_entry_ids: vec![1, 2, 3],
                    role: Some("developer".to_string()),
                    feature: Some("vnc-024".to_string()),
                    token_limit: Some(1000),
                    transcript_excerpt: None,
                    accept: None,
                },
            ),
        ]
    }

    fn response_fixtures() -> Vec<(&'static str, HookResponse)> {
        vec![
            (
                "response_pong",
                HookResponse::Pong {
                    server_version: "0.1.0".to_string(),
                },
            ),
            ("response_ack", HookResponse::Ack),
            (
                "response_error",
                HookResponse::Error {
                    code: ERR_UID_MISMATCH,
                    message: "uid mismatch".to_string(),
                },
            ),
            (
                "response_entries",
                HookResponse::Entries {
                    items: vec![EntryPayload {
                        id: 42,
                        title: "Round-trip fixtures are the contract authority".to_string(),
                        content: "ADR-002: fixtures assert serde behavior".to_string(),
                        confidence: 0.85,
                        similarity: 0.92,
                        category: "decision".to_string(),
                    }],
                    total_tokens: 128,
                },
            ),
            (
                "response_briefing_content",
                HookResponse::BriefingContent {
                    content: "You are a Rust developer for Unimatrix.".to_string(),
                    token_count: 25,
                },
            ),
        ]
    }

    /// EMITTER (ADR-002 step 1): serialize every variant + edge case + the typed delta payload to
    /// committed JSON fixtures. Run on `cargo test`; the result is reviewed and committed, then the
    /// node harness consumes the same files. `request_hookinput_flatten.json` is the one fixture
    /// authored as raw JSON (it carries unknown top-level keys that no typed `serialize` can emit —
    /// the flatten *parse* side is what we assert), so the emitter writes it from a literal.
    #[test]
    fn test_emit_fixtures() {
        let dir = fixtures_dir();
        std::fs::create_dir_all(&dir).expect("create fixtures dir");

        for (name, req) in request_fixtures() {
            let json = serde_json::to_string_pretty(&req).expect("serialize request fixture");
            write_fixture_atomic(&dir, &format!("{name}.json"), &(json + "\n"));
        }
        for (name, resp) in response_fixtures() {
            let json = serde_json::to_string_pretty(&resp).expect("serialize response fixture");
            write_fixture_atomic(&dir, &format!("{name}.json"), &(json + "\n"));
        }

        // Flatten fixture: HookInput JSON with extra unknown top-level keys (land under `extra`)
        // PLUS a key colliding with a named field (`session_id`) — the named field must win and the
        // collision must NOT leak into `extra`. Authored as a literal because flatten extras have no
        // typed serialize source.
        let flatten = serde_json::json!({
            "hook_event_name": "PreToolUse",
            "session_id": "sess-fixture",
            "unknown_extra_key": "extra-value",
            "another_extra": { "nested": 7 }
        });
        write_fixture_atomic(
            &dir,
            "request_hookinput_flatten.json",
            &(serde_json::to_string_pretty(&flatten).unwrap() + "\n"),
        );

        // Dual-sided delta: emitted from the TYPED struct (not hand-written) so the Rust struct is
        // the source of the {offset,bytes} shape both runtimes verify (AC-11).
        let delta = TranscriptDeltaPayload {
            offset: DELTA_OFFSET,
            bytes: DELTA_BYTES.to_string(),
        };
        write_fixture_atomic(
            &dir,
            "transcript_delta_payload.json",
            &(serde_json::to_string_pretty(&delta).unwrap() + "\n"),
        );
    }

    /// Structural round-trip identity for every request fixture: parse → re-serialize → compare as
    /// `serde_json::Value` (semantic, key-order-independent). Proves the committed fixture
    /// deserializes to the right `HookRequest` variant and re-emits the same shape (AC-05).
    #[test]
    fn test_round_trip_request_fixtures() {
        for (name, _) in request_fixtures() {
            let raw = read_fixture(&format!("{name}.json"));
            let decoded: HookRequest = serde_json::from_str(&raw)
                .unwrap_or_else(|e| panic!("fixture {name} must parse as HookRequest: {e}"));
            let re = serde_json::to_value(&decoded).unwrap();
            let original: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(re, original, "fixture {name} must round-trip structurally");
            // Tagged discriminant must be present on every HookRequest fixture.
            assert!(
                original.get("type").and_then(|t| t.as_str()).is_some(),
                "fixture {name} must carry a literal `type` discriminant"
            );
        }
    }

    /// Same for every response fixture (AC-05).
    #[test]
    fn test_round_trip_response_fixtures() {
        for (name, _) in response_fixtures() {
            let raw = read_fixture(&format!("{name}.json"));
            let decoded: HookResponse = serde_json::from_str(&raw)
                .unwrap_or_else(|e| panic!("fixture {name} must parse as HookResponse: {e}"));
            let re = serde_json::to_value(&decoded).unwrap();
            let original: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(re, original, "fixture {name} must round-trip structurally");
            assert!(
                original.get("type").and_then(|t| t.as_str()).is_some(),
                "fixture {name} must carry a literal `type` discriminant"
            );
        }
    }

    /// None-vs-omission, DUAL-DIRECTION, for ALL FOUR `skip_serializing_if = "Option::is_none"`
    /// fields (R-02 / AC-06 / #3557 — the single most-omitted test category). For each field:
    ///   (a) EMIT-absent: when `None`, the key is ABSENT from the JSON (not `null`).
    ///   (b) PARSE-default: an omitting fixture deserializes to the default (`None`).
    ///   (c) PRESENT round-trip: a non-trivial value survives intact.
    #[test]
    fn test_none_vs_omission_dual_direction_all_four_fields() {
        // --- ImplantEvent.topic_signal + ImplantEvent.provider ---
        // (a) emit-absent
        let ev_none = ImplantEvent {
            event_type: "tool_use".to_string(),
            session_id: "s1".to_string(),
            timestamp: 100,
            payload: serde_json::json!({}),
            topic_signal: None,
            provider: None,
        };
        let ev_val = serde_json::to_value(&ev_none).unwrap();
        let ev_obj = ev_val.as_object().unwrap();
        assert!(
            !ev_obj.contains_key("topic_signal"),
            "topic_signal: None must be absent, not null"
        );
        assert!(
            !ev_obj.contains_key("provider"),
            "provider: None must be absent, not null"
        );
        // (b) parse-default from the omitting fixture
        let raw = read_fixture("request_record_event_omitted.json");
        let omitted: HookRequest = serde_json::from_str(&raw).unwrap();
        match omitted {
            HookRequest::RecordEvent { event } => {
                assert!(event.topic_signal.is_none(), "omitted topic_signal → None");
                assert!(event.provider.is_none(), "omitted provider → None");
            }
            _ => panic!("expected RecordEvent"),
        }
        // (c) present non-trivial round-trip
        let raw = read_fixture("request_record_event.json");
        let present: HookRequest = serde_json::from_str(&raw).unwrap();
        match present {
            HookRequest::RecordEvent { event } => {
                assert_eq!(event.topic_signal.as_deref(), Some("vnc-024"));
                assert_eq!(event.provider.as_deref(), Some("claude-code"));
            }
            _ => panic!("expected RecordEvent"),
        }

        // --- ContextSearch.source ---
        let cs_none = HookRequest::ContextSearch {
            query: "q".to_string(),
            session_id: None,
            role: None,
            task: None,
            feature: None,
            k: None,
            max_tokens: None,
            source: None,
            accept: None,
        };
        let cs_val = serde_json::to_value(&cs_none).unwrap();
        assert!(
            !cs_val.as_object().unwrap().contains_key("source"),
            "source: None must be absent, not null"
        );
        let raw = read_fixture("request_context_search_no_source.json");
        match serde_json::from_str::<HookRequest>(&raw).unwrap() {
            HookRequest::ContextSearch { source, .. } => {
                assert!(source.is_none(), "omitted source → None")
            }
            _ => panic!("expected ContextSearch"),
        }
        let raw = read_fixture("request_context_search.json");
        match serde_json::from_str::<HookRequest>(&raw).unwrap() {
            HookRequest::ContextSearch { source, .. } => {
                assert_eq!(source.as_deref(), Some("SubagentStart"))
            }
            _ => panic!("expected ContextSearch"),
        }

        // --- CompactPayload.transcript_excerpt ---
        let cp_none = HookRequest::CompactPayload {
            session_id: "s1".to_string(),
            injected_entry_ids: vec![],
            role: None,
            feature: None,
            token_limit: None,
            transcript_excerpt: None,
            accept: None,
        };
        let cp_val = serde_json::to_value(&cp_none).unwrap();
        assert!(
            !cp_val
                .as_object()
                .unwrap()
                .contains_key("transcript_excerpt"),
            "transcript_excerpt: None must be absent, not null"
        );
        let raw = read_fixture("request_compact_payload_no_excerpt.json");
        match serde_json::from_str::<HookRequest>(&raw).unwrap() {
            HookRequest::CompactPayload {
                transcript_excerpt, ..
            } => assert!(transcript_excerpt.is_none(), "omitted excerpt → None"),
            _ => panic!("expected CompactPayload"),
        }
        let raw = read_fixture("request_compact_payload.json");
        match serde_json::from_str::<HookRequest>(&raw).unwrap() {
            HookRequest::CompactPayload {
                transcript_excerpt, ..
            } => assert_eq!(transcript_excerpt.as_deref(), Some("prior excerpt text")),
            _ => panic!("expected CompactPayload"),
        }
    }

    /// Flatten (R-01 scenario 2): unknown top-level keys land under `extra`; a collision key
    /// (`session_id`) is captured by the NAMED field and does NOT leak into `extra`.
    #[test]
    fn test_flatten_extra_and_collision() {
        let raw = read_fixture("request_hookinput_flatten.json");
        let hi: HookInput = serde_json::from_str(&raw).unwrap();
        // named fields parse alongside the extras
        assert_eq!(hi.hook_event_name, "PreToolUse");
        assert_eq!(hi.session_id.as_deref(), Some("sess-fixture"));
        // unknown keys land in extra
        assert_eq!(hi.extra["unknown_extra_key"], "extra-value");
        assert_eq!(hi.extra["another_extra"]["nested"], 7);
        // collision: the named field won; the colliding key is NOT duplicated into extra
        assert!(
            hi.extra.get("session_id").is_none(),
            "named field session_id must win; collision must not leak into extra; extra={:?}",
            hi.extra
        );
    }

    /// AC-11 Rust half: parse the committed delta fixture into the TYPED `TranscriptDeltaPayload`
    /// (the SAME struct the accept-and-drop guard deserializes into — ADR-004) and re-serialize
    /// losslessly. The node harness asserts the TS→Rust direction against the `{offset,bytes}`
    /// shape the binding declares; together they make AC-11 dual-sided (a Rust-emit-only check
    /// does NOT satisfy AC-11).
    #[test]
    fn test_transcript_delta_payload_round_trip() {
        let raw = read_fixture("transcript_delta_payload.json");
        let parsed: TranscriptDeltaPayload = serde_json::from_str(&raw)
            .expect("delta fixture must parse into TranscriptDeltaPayload");
        assert_eq!(
            parsed.offset, DELTA_OFFSET,
            "offset must round-trip losslessly"
        );
        assert_eq!(parsed.bytes, DELTA_BYTES, "bytes must round-trip intact");
        // structural re-serialization identity
        let re = serde_json::to_value(&parsed).unwrap();
        let original: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(re, original, "delta fixture must round-trip structurally");
        // exactly the two declared keys — a drift in the binding shape is caught here + in node.
        let keys: Vec<&str> = original
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys.len(), 2, "delta payload has exactly offset + bytes");
        assert!(original.get("offset").and_then(|v| v.as_u64()).is_some());
        assert!(original.get("bytes").and_then(|v| v.as_str()).is_some());
    }

    #[test]
    fn test_hook_input_clone_includes_new_fields() {
        // Wave 2 requirement: HookInput must be Clone (already derived) and carry
        // provider + mcp_context through the clone for the mcp_context promotion adapter.
        let json = r#"{
            "hook_event_name": "BeforeTool",
            "provider": "gemini-cli",
            "mcp_context": {"tool_name": "context_cycle"}
        }"#;
        let input: HookInput = serde_json::from_str(json).expect("deserialize");
        let cloned = input.clone();
        assert_eq!(cloned.provider, Some("gemini-cli".to_string()));
        assert_eq!(
            cloned
                .mcp_context
                .as_ref()
                .and_then(|v| v.get("tool_name"))
                .and_then(|v| v.as_str()),
            Some("context_cycle")
        );
    }

    // -- vnc-027 wire-accept-text: additive `accept` field + `HookResponse::Text` (ADR-001) --

    #[test]
    fn test_context_search_without_accept_serializes_byte_unchanged() {
        // R-07 / AC-11: accept: None must produce the exact pre-feature bytes (no `accept`
        // key present). `skip_serializing_if = "Option::is_none"` is the byte authority.
        let req = HookRequest::ContextSearch {
            query: "frozen frame".to_string(),
            session_id: Some("sess-1".to_string()),
            role: Some("developer".to_string()),
            task: None,
            feature: None,
            k: Some(5),
            max_tokens: None,
            source: None,
            accept: None,
        };
        let bytes = serialize_request(&req).unwrap();
        let json = String::from_utf8(bytes).unwrap();
        assert!(
            !json.contains("accept"),
            "accept: None must omit the key from serialized JSON; got: {json}"
        );
        // The exact pre-feature byte string for these field values.
        assert_eq!(
            json,
            r#"{"type":"ContextSearch","query":"frozen frame","session_id":"sess-1","role":"developer","task":null,"feature":null,"k":5,"max_tokens":null}"#
        );
    }

    #[test]
    fn test_compact_payload_without_accept_serializes_byte_unchanged() {
        // R-07 / AC-11: same skip_serializing_if proof for CompactPayload.
        let req = HookRequest::CompactPayload {
            session_id: "s1".to_string(),
            injected_entry_ids: vec![1, 2],
            role: Some("developer".to_string()),
            feature: None,
            token_limit: Some(500),
            transcript_excerpt: None,
            accept: None,
        };
        let bytes = serialize_request(&req).unwrap();
        let json = String::from_utf8(bytes).unwrap();
        assert!(
            !json.contains("accept"),
            "accept: None must omit the key from serialized JSON; got: {json}"
        );
        assert_eq!(
            json,
            r#"{"type":"CompactPayload","session_id":"s1","injected_entry_ids":[1,2],"role":"developer","feature":null,"token_limit":500}"#
        );
    }

    #[test]
    fn test_context_search_with_accept_text_plain_roundtrips() {
        // accept: Some("text/plain") serializes WITH the key and deserializes back equal.
        let req = HookRequest::ContextSearch {
            query: "q".to_string(),
            session_id: None,
            role: None,
            task: None,
            feature: None,
            k: None,
            max_tokens: None,
            source: None,
            accept: Some("text/plain".to_string()),
        };
        let bytes = serialize_request(&req).unwrap();
        let json = String::from_utf8(bytes.clone()).unwrap();
        assert!(
            json.contains(r#""accept":"text/plain""#),
            "accept: Some must emit the key; got: {json}"
        );
        match deserialize_request(&bytes).unwrap() {
            HookRequest::ContextSearch { accept, .. } => {
                assert_eq!(accept.as_deref(), Some("text/plain"));
            }
            _ => panic!("expected ContextSearch"),
        }
    }

    #[test]
    fn test_compact_payload_with_accept_text_plain_roundtrips() {
        let req = HookRequest::CompactPayload {
            session_id: "s1".to_string(),
            injected_entry_ids: vec![],
            role: None,
            feature: None,
            token_limit: None,
            transcript_excerpt: None,
            accept: Some("text/plain".to_string()),
        };
        let bytes = serialize_request(&req).unwrap();
        match deserialize_request(&bytes).unwrap() {
            HookRequest::CompactPayload { accept, .. } => {
                assert_eq!(accept.as_deref(), Some("text/plain"));
            }
            _ => panic!("expected CompactPayload"),
        }
    }

    #[test]
    fn test_accept_default_on_missing_field() {
        // A JSON body with no `accept` key deserializes to accept: None (serde default),
        // proving old-client frames still parse on both injection-bearing variants.
        let cs = br#"{"type":"ContextSearch","query":"old client"}"#;
        match deserialize_request(cs).unwrap() {
            HookRequest::ContextSearch { accept, .. } => assert!(accept.is_none()),
            _ => panic!("expected ContextSearch"),
        }
        let cp = br#"{"type":"CompactPayload","session_id":"s1","injected_entry_ids":[]}"#;
        match deserialize_request(cp).unwrap() {
            HookRequest::CompactPayload { accept, .. } => assert!(accept.is_none()),
            _ => panic!("expected CompactPayload"),
        }
    }

    #[test]
    fn test_no_deny_unknown_fields() {
        // A frame with an unrecognized extra key still deserializes (no deny_unknown_fields).
        let json = br#"{"type":"ContextSearch","query":"q","accept":"text/plain","future_field":42}"#;
        match deserialize_request(json).unwrap() {
            HookRequest::ContextSearch { query, accept, .. } => {
                assert_eq!(query, "q");
                assert_eq!(accept.as_deref(), Some("text/plain"));
            }
            _ => panic!("expected ContextSearch"),
        }
    }

    #[test]
    fn test_response_text_variant_roundtrips() {
        // HookResponse::Text serializes with "type":"Text" and roundtrips; body bytes
        // preserved verbatim including the load-bearing header prefix and multibyte content.
        let body = "--- Unimatrix Context ---\n• café ☕ entry\n".to_string();
        let resp = HookResponse::Text { body: body.clone() };
        let bytes = serialize_response(&resp).unwrap();
        let json = String::from_utf8(bytes.clone()).unwrap();
        assert!(
            json.contains(r#""type":"Text""#),
            "Text variant must carry the type tag; got: {json}"
        );
        match deserialize_response(&bytes).unwrap() {
            HookResponse::Text { body: decoded } => assert_eq!(decoded, body),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_response_text_variant_empty_body() {
        // Empty body is structurally valid at the wire layer (listener returns Ack for the
        // empty-injection case, per ADR-001 §4 — enforced at the listener, not here).
        let resp = HookResponse::Text {
            body: String::new(),
        };
        let bytes = serialize_response(&resp).unwrap();
        match deserialize_response(&bytes).unwrap() {
            HookResponse::Text { body } => assert!(body.is_empty()),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn test_older_response_json_without_text_still_deserializes() {
        // Additive proof: pre-feature HookResponse JSON (no Text variant) still parses.
        let json = br#"{"type":"BriefingContent","content":"c","token_count":3}"#;
        assert!(matches!(
            deserialize_response(json).unwrap(),
            HookResponse::BriefingContent { .. }
        ));
    }

    #[test]
    fn test_ping_briefing_have_no_accept_field() {
        // ADR-001 §1: accept is NOT added to Ping or Briefing (Pong stays JSON).
        let ping = serde_json::to_string(&HookRequest::Ping).unwrap();
        assert!(!ping.contains("accept"), "Ping must have no accept: {ping}");
        let briefing = serde_json::to_string(&HookRequest::Briefing {
            role: "developer".to_string(),
            task: "t".to_string(),
            feature: None,
            max_tokens: None,
        })
        .unwrap();
        assert!(
            !briefing.contains("accept"),
            "Briefing must have no accept: {briefing}"
        );
    }

    #[test]
    fn test_existing_response_variants_unchanged() {
        // Adding Text must not reorder/retag existing variants — each serializes byte-unchanged.
        assert_eq!(
            serde_json::to_string(&HookResponse::Ack).unwrap(),
            r#"{"type":"Ack"}"#
        );
        assert_eq!(
            serde_json::to_string(&HookResponse::Pong {
                server_version: "0.1.0".to_string()
            })
            .unwrap(),
            r#"{"type":"Pong","server_version":"0.1.0"}"#
        );
        assert_eq!(
            serde_json::to_string(&HookResponse::Error {
                code: -32005,
                message: "x".to_string()
            })
            .unwrap(),
            r#"{"type":"Error","code":-32005,"message":"x"}"#
        );
        assert_eq!(
            serde_json::to_string(&HookResponse::BriefingContent {
                content: "c".to_string(),
                token_count: 3
            })
            .unwrap(),
            r#"{"type":"BriefingContent","content":"c","token_count":3}"#
        );
        assert_eq!(
            serde_json::to_string(&HookResponse::Entries {
                items: vec![],
                total_tokens: 0
            })
            .unwrap(),
            r#"{"type":"Entries","items":[],"total_tokens":0}"#
        );
    }
}
