# Pseudocode: wire-protocol

## Purpose

Define the wire protocol types and framing functions for IPC between hook processes and the UDS listener. All types live in `unimatrix-engine/src/wire.rs`. Per ADR-005, the protocol uses length-prefixed JSON with serde-tagged enums.

## File: crates/unimatrix-engine/src/wire.rs

### Constants

```
MAX_PAYLOAD_SIZE: usize = 1_048_576  // 1 MiB
FRAME_HEADER_SIZE: usize = 4          // 4-byte BE u32 length prefix
```

### HookInput (Claude Code stdin JSON -- ADR-006)

```
#[derive(Deserialize, Debug)]
struct HookInput {
    #[serde(default)]
    hook_event_name: String,

    #[serde(default)]
    session_id: Option<String>,

    #[serde(default)]
    cwd: Option<String>,

    #[serde(default)]
    transcript_path: Option<String>,

    #[serde(flatten)]
    extra: serde_json::Value,
}
```

All fields use `#[serde(default)]`. Unknown fields captured by `#[serde(flatten)]`. No `deny_unknown_fields`.

### HookRequest (IPC wire protocol)

```
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
enum HookRequest {
    Ping,

    SessionRegister {
        session_id: String,
        cwd: String,
        agent_role: Option<String>,
        feature: Option<String>,
    },

    SessionClose {
        session_id: String,
        outcome: Option<String>,
        duration_secs: u64,
    },

    RecordEvent(ImplantEvent),

    RecordEvents(Vec<ImplantEvent>),

    // Stubs for future features (col-007+)
    #[allow(dead_code)]
    ContextSearch {
        query: String,
        role: Option<String>,
        task: Option<String>,
        feature: Option<String>,
        k: Option<u32>,
        max_tokens: Option<u32>,
    },

    #[allow(dead_code)]
    Briefing {
        role: String,
        task: String,
        feature: Option<String>,
        max_tokens: Option<u32>,
    },

    #[allow(dead_code)]
    CompactPayload {
        session_id: String,
        injected_entry_ids: Vec<u64>,
        role: Option<String>,
        feature: Option<String>,
        token_limit: Option<u32>,
    },
}
```

### HookResponse (IPC wire protocol)

```
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
enum HookResponse {
    Pong { server_version: String },

    Ack,

    Error { code: i32, message: String },

    // Stubs for future features
    #[allow(dead_code)]
    Entries {
        items: Vec<EntryPayload>,
        total_tokens: u32,
    },

    #[allow(dead_code)]
    BriefingContent {
        content: String,
        token_count: u32,
    },
}
```

### ImplantEvent

```
#[derive(Serialize, Deserialize, Debug, Clone)]
struct ImplantEvent {
    event_type: String,
    session_id: String,
    timestamp: u64,
    payload: serde_json::Value,
}
```

### EntryPayload (stub for future search results)

```
#[derive(Serialize, Deserialize, Debug, Clone)]
#[allow(dead_code)]
struct EntryPayload {
    id: u64,
    title: String,
    content: String,
    confidence: f64,
    similarity: f64,
    category: String,
}
```

### TransportError

```
#[derive(Debug)]
enum TransportError {
    Unavailable(String),
    Timeout(Duration),
    Rejected { code: i32, message: String },
    Codec(String),
    Transport(String),
}

impl fmt::Display for TransportError { ... }
impl std::error::Error for TransportError {}

impl From<io::Error> for TransportError {
    // Map io::Error to Transport variant
    // Special case: io::ErrorKind::TimedOut -> Timeout
    // Special case: io::ErrorKind::ConnectionRefused -> Unavailable
    // Special case: io::ErrorKind::NotFound -> Unavailable
}
```

### Framing Functions

#### write_frame

```
fn write_frame(writer: &mut impl Write, payload: &[u8]) -> io::Result<()>:
    if payload.len() > MAX_PAYLOAD_SIZE:
        return Err(io::Error::new(InvalidInput, "payload exceeds 1 MiB limit"))

    let length = payload.len() as u32
    writer.write_all(&length.to_be_bytes())
    writer.write_all(payload)
    writer.flush()
```

#### read_frame

```
fn read_frame(reader: &mut impl Read, max_size: usize) -> Result<Vec<u8>, TransportError>:
    let mut header = [0u8; 4]
    reader.read_exact(&mut header)
        .map_err(|e| if e.kind() == UnexpectedEof:
            TransportError::Transport("connection closed during header read")
        else:
            TransportError::from(e))

    let length = u32::from_be_bytes(header) as usize

    if length == 0:
        return Err(TransportError::Codec("empty payload"))

    if length > max_size:
        return Err(TransportError::Codec(format!("payload {} exceeds max {}", length, max_size)))

    let mut buffer = vec![0u8; length]
    reader.read_exact(&mut buffer)
        .map_err(|e| if e.kind() == UnexpectedEof:
            TransportError::Transport("connection closed during payload read")
        else:
            TransportError::from(e))

    Ok(buffer)
```

### Serialization Helpers

```
fn serialize_request(request: &HookRequest) -> Result<Vec<u8>, TransportError>:
    serde_json::to_vec(request)
        .map_err(|e| TransportError::Codec(e.to_string()))

fn deserialize_request(data: &[u8]) -> Result<HookRequest, TransportError>:
    serde_json::from_slice(data)
        .map_err(|e| TransportError::Codec(e.to_string()))

fn serialize_response(response: &HookResponse) -> Result<Vec<u8>, TransportError>:
    serde_json::to_vec(response)
        .map_err(|e| TransportError::Codec(e.to_string()))

fn deserialize_response(data: &[u8]) -> Result<HookResponse, TransportError>:
    serde_json::from_slice(data)
        .map_err(|e| TransportError::Codec(e.to_string()))
```

### Error Codes

```
pub const ERR_UID_MISMATCH: i32 = -32001;
pub const ERR_LINEAGE_FAILED: i32 = -32002;
pub const ERR_UNKNOWN_REQUEST: i32 = -32003;
pub const ERR_INVALID_PAYLOAD: i32 = -32004;
pub const ERR_INTERNAL: i32 = -32005;
```

## Error Handling

- `write_frame`: Returns `io::Error` directly (caller decides transport vs codec)
- `read_frame`: Returns `TransportError` with EOF -> Transport, size -> Codec
- Serialization: serde errors -> `TransportError::Codec`
- All framing errors are non-panicking

## Key Test Scenarios

1. Round-trip: serialize Ping -> write_frame -> read_frame -> deserialize -> matches Ping
2. Round-trip for each HookRequest variant with all fields populated
3. Round-trip for each HookResponse variant
4. Oversized payload rejected by write_frame (> 1 MiB)
5. Oversized length prefix rejected by read_frame
6. Zero-length prefix rejected by read_frame
7. Partial header (2 bytes then EOF) -> Transport error
8. Partial payload (valid header, truncated body) -> Transport error
9. Invalid UTF-8 payload -> Codec error from deserialize
10. Unknown type tag -> serde error from deserialize
11. HookInput defensive parsing: minimal JSON, unknown fields, missing fields, empty string
12. MAX_PAYLOAD_SIZE boundary: exactly 1 MiB accepted, 1 MiB + 1 rejected
