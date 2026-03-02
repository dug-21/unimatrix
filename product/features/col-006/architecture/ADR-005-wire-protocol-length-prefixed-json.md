## ADR-005: Wire Protocol Uses Length-Prefixed JSON (Not JSON-RPC)

### Context

The IPC between hook processes and the UDS listener needs a wire protocol for message framing and type routing. Three candidates were evaluated:

1. **JSON-RPC:** The MCP protocol uses JSON-RPC 2.0 over stdio. Using the same protocol for the internal IPC would provide consistency. However, JSON-RPC requires an `id` field for request/response correlation, a `jsonrpc: "2.0"` version field, and a `method` string for routing — all overhead for a single-request-per-connection model where routing is handled by serde enum tags.

2. **Length-prefixed JSON:** A 4-byte big-endian length prefix followed by a JSON payload. Type routing via serde-tagged enums (`#[serde(tag = "type")]`). No protocol-level versioning (the bundled subcommand guarantees version alignment).

3. **Binary (bincode):** Compact, fast serialization. Used internally by redb for entry storage. However, not human-readable, making debugging harder. Serialization time difference vs JSON is ~0.05ms — negligible within the 50ms budget.

### Decision

Use length-prefixed JSON with serde-tagged enums for type routing.

**Framing format:**
```
+---+---+---+---+------...------+
| Length (4 bytes, big-endian u32) | JSON payload (UTF-8) |
+---+---+---+---+------...------+
```

- Maximum payload size: 1 MiB (1,048,576 bytes). Reject payloads larger than this.
- The 4-byte length prefix supports payloads up to 4 GiB, but the 1 MiB limit prevents accidental or malicious memory exhaustion.

**Type routing via serde tags:**
```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HookRequest {
    Ping,
    SessionRegister { session_id: String, cwd: String },
    SessionClose { session_id: String },
    RecordEvent { event_type: String, payload: serde_json::Value },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HookResponse {
    Pong { server_version: String },
    Ack,
    Error { code: i32, message: String },
}
```

The `#[serde(tag = "type")]` attribute produces JSON like `{"type":"Ping"}` or `{"type":"SessionRegister","session_id":"abc","cwd":"/project"}`. This is self-describing, human-readable, and extensible.

**No protocol version field.** The hook subcommand is part of the same binary as the server. Version mismatch is impossible in the bundled subcommand model. If a future remote transport introduces version mismatch risk, a `Handshake` request type with version negotiation can be added as a backward-compatible extension to the enum.

**Why not JSON-RPC:**
- JSON-RPC's `id` field is unnecessary for single-request-per-connection (no request multiplexing).
- JSON-RPC's `method` string duplicates the information in the serde tag.
- JSON-RPC's error format (`code`, `message`, `data`) is more complex than needed. The `HookResponse::Error` variant is sufficient.
- Using a different protocol than MCP for the internal IPC avoids confusion about which protocol is in use on which transport (stdio = JSON-RPC/MCP, UDS = length-prefixed JSON).

**Why not bincode:**
- JSON is debuggable with `socat` or `nc` — useful for development and troubleshooting.
- JSON serialization at ~0.2ms is negligible within the 50ms budget.
- The existing codebase uses JSON for all external interfaces (MCP, hook stdin). Using JSON internally maintains consistency.
- Bincode saves ~0.15ms per message — not enough to justify the debugging cost.

### Consequences

**Easier:**
- Messages are human-readable in network traces and logs.
- Serde enum tags provide exhaustive match-based routing — the compiler catches missing handlers.
- Extension is simple: add a new variant to the enum, rebuild both sides (same binary).
- JSONL event queue (Component 7) uses the same serialization format.

**Harder:**
- JSON is larger than bincode (~2-5x for typical messages). For col-006 messages (Ping, SessionRegister), payloads are <200 bytes — negligible.
- Serde tag-based routing means the full payload must be deserialized before routing. For small payloads, this is acceptable. If future payloads become large (e.g., embedding vectors), a header-first protocol may be needed.
- The 4-byte length prefix requires reading exactly 4 bytes first, then exactly N bytes. Partial reads must be handled (read in a loop until all bytes are received or timeout).
