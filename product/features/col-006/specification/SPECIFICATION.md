# Specification: col-006 Hook Transport Layer ("Cortical Implant")

## Objective

Establish a Unix domain socket (UDS) transport layer in the Unimatrix MCP server that enables hook processes to communicate with the running server via IPC. This is the foundation for all hook-driven knowledge delivery (col-007 through col-011). The core constraint driving the architecture is redb v3.1.x's exclusive file lock: no second process can open the database while the MCP server is running, forcing all hook-to-server communication through IPC (D14-2 RQ-2). col-006 delivers the transport infrastructure, a `hook` subcommand on the existing binary, a `Transport` trait abstraction, shared business logic extraction into a `unimatrix-engine` crate, layered zero-configuration authentication, graceful degradation with event queuing, and end-to-end validation via SessionStart/Stop smoke tests.

## Functional Requirements

### FR-01: UDS Listener in MCP Server

- FR-01.1: The MCP server spawns a tokio task on startup that binds a `UnixListener` on `~/.unimatrix/{project_hash}/unimatrix.sock`.
- FR-01.2: The socket file is created with permissions mode `0o600` (owner read/write only).
- FR-01.3: The UDS listener accepts connections concurrently without blocking the existing stdio MCP transport. Each accepted connection is handled in a separate tokio task.
- FR-01.4: Each connection handler reads a length-prefixed JSON request (4-byte big-endian u32 length prefix followed by a JSON payload), dispatches it to the appropriate handler, and writes a length-prefixed JSON response.
- FR-01.5: The UDS listener operates alongside the stdio transport for the lifetime of the server process. Both transports share the same underlying `Store`, `VectorIndex`, and `EmbedServiceHandle` via `Arc`.
- FR-01.6: The UDS handler uses `spawn_blocking` for operations that access redb or HNSW, consistent with the existing async wrapper pattern in `unimatrix-core`.

### FR-02: Socket Lifecycle Management

- FR-02.1: Socket creation occurs after PidGuard acquisition and before the server begins accepting MCP connections over stdio.
- FR-02.2: On startup, if a socket file already exists at the bind path, the server removes it before binding. This is safe because PidGuard has already established mutual exclusion -- no other legitimate server is running.
- FR-02.3: On graceful shutdown, the socket file is removed before database compaction begins. The shutdown sequence is: stop accepting new UDS connections, drain in-flight UDS requests (1-second timeout), remove socket file, then proceed to vector dump and compaction.
- FR-02.4: A `SocketGuard` RAII struct manages the socket file lifecycle, analogous to `PidGuard`. It removes the socket file on drop.
- FR-02.5: `LifecycleHandles` is extended to include the `SocketGuard`, ensuring socket cleanup participates in the shutdown sequence.
- FR-02.6: `ProjectPaths` is extended with a `socket_path` field (`~/.unimatrix/{project_hash}/unimatrix.sock`).

### FR-03: Hook Subcommand

- FR-03.1: The `unimatrix-server` binary gains a `hook` subcommand via clap: `unimatrix-server hook <EVENT>`. The `EVENT` argument is positional and required.
- FR-03.2: The hook subcommand reads JSON from stdin. This JSON is the Claude Code hook event payload containing at minimum `hook_event_name` and `cwd`. Fields `session_id` and `transcript_path` may or may not be present.
- FR-03.3: All fields in the stdin JSON are parsed with `#[serde(default)]` and `Option<T>` wrappers. Unknown fields are captured via `#[serde(flatten)]` into a `HashMap<String, serde_json::Value>` for forward compatibility (SR-05).
- FR-03.4: The hook subcommand computes the project hash from the `cwd` field (or the current working directory as fallback) using the same `compute_project_hash` logic in `project.rs`.
- FR-03.5: Instance discovery: the hook subcommand looks for `unimatrix.sock` at `~/.unimatrix/{project_hash}/unimatrix.sock`. If the socket does not exist, it follows the graceful degradation path (FR-08).
- FR-03.6: The hook subcommand connects to the UDS, constructs a `Request` from the parsed hook event, sends it, and processes the `Response`.
- FR-03.7: Exit codes: `0` for success (including graceful degradation where the server is unavailable), `1` for unexpected errors. Errors are logged to stderr. The hook subcommand never exits non-zero for expected failure scenarios (server down, timeout, parse failure).
- FR-03.8: The hook subcommand does not initialize tokio runtime, ONNX, redb, or HNSW. It uses synchronous `std::os::unix::net::UnixStream` for UDS communication.
- FR-03.9: For synchronous requests (where a response is needed), the hook writes the response payload to stdout. For fire-and-forget requests, the hook exits immediately after sending.

### FR-04: Transport Trait and LocalTransport

- FR-04.1: A `Transport` trait is defined with five methods: `request(&self, req: Request, timeout: Duration) -> Result<Response, TransportError>`, `fire_and_forget(&self, req: Request) -> Result<(), TransportError>`, `is_connected(&self) -> bool`, `connect(&mut self) -> Result<(), TransportError>`, `disconnect(&mut self)`.
- FR-04.2: The trait has a synchronous public interface. Implementations may use async internals, but callers are not required to have a tokio runtime.
- FR-04.3: `LocalTransport` implements `Transport` over UDS. It connects to a socket path, sets `SO_RCVTIMEO` and `SO_SNDTIMEO` for timeout enforcement, and uses the length-prefixed JSON wire protocol.
- FR-04.4: The `TransportError` enum has five variants: `Unavailable(String)` for when the server is unreachable, `Timeout(Duration)` for operation timeouts, `Rejected { code: i32, message: String }` for server-side rejections (auth failure, invalid request), `Codec(String)` for serialization/deserialization errors, `Transport(String)` for transport-level errors (broken pipe, socket errors).
- FR-04.5: The `Request` enum includes at minimum: `Ping`, `SessionRegister { session_id, agent_role: Option<String>, feature: Option<String> }`, `SessionClose { session_id, outcome: Option<String>, duration_secs: u64 }`, `RecordEvent(ImplantEvent)`, and `RecordEvents(Vec<ImplantEvent>)`. Additional variants (`ContextSearch`, `Briefing`, `CompactPayload`) are defined as stubs with `#[allow(dead_code)]` for future features (col-007, col-008).
- FR-04.6: The `Response` enum includes at minimum: `Pong { version: String }`, `Ack`, and `Error { code: i32, message: String }`. Additional variants (`Entries`, `Briefing`) are defined as stubs for future features.
- FR-04.7: The `ImplantEvent` struct contains: `event_type: String`, `session_id: String`, `timestamp: u64`, `payload: serde_json::Value`.
- FR-04.8: The `Transport` trait is `Send + Sync` to allow use from multiple threads.

### FR-05: Wire Protocol

- FR-05.1: The wire protocol uses length-prefixed JSON: a 4-byte big-endian `u32` length prefix followed by the JSON payload bytes.
- FR-05.2: The maximum message size is 1 MB (1,048,576 bytes). Messages exceeding this limit are rejected with `TransportError::Codec`.
- FR-05.3: Both `Request` and `Response` are serialized as JSON using serde. Unknown fields in deserialized JSON are ignored (forward compatibility).
- FR-05.4: Connection model: one request/response per connection. The hook process opens a connection, sends one request, reads one response (or skips the read for fire-and-forget), and closes the connection. Persistent connections are deferred to future optimization.

### FR-06: `unimatrix-engine` Crate Extraction

- FR-06.1: A new `unimatrix-engine` crate is created in the workspace at `crates/unimatrix-engine/`.
- FR-06.2: The following modules are extracted from `unimatrix-server` into `unimatrix-engine`: `confidence.rs` (confidence formula, `compute_confidence`, `rerank_score`, `co_access_affinity`), `coaccess.rs` (co-access pair generation, boost computation), `project.rs` (project root detection, hash computation, data directory management).
- FR-06.3: The extraction is purely structural. No function signatures, algorithms, or constants change. The modules are moved, not rewritten.
- FR-06.4: The extraction is incremental: one module at a time, with the full test suite (1025 unit + 174 integration) run after each move. All tests must pass without modification after each incremental extraction.
- FR-06.5: `unimatrix-server` gains a dependency on `unimatrix-engine` and re-exports or imports the moved modules from the engine crate.
- FR-06.6: The `unimatrix-engine` crate depends on `unimatrix-core` and `unimatrix-store` (for types used by confidence and co-access computation). It does not depend on `unimatrix-server`, `unimatrix-embed`, or `unimatrix-vector`.
- FR-06.7: New modules `search.rs` and `query.rs` are created as stubs in `unimatrix-engine` with documented interfaces but no business logic. Full implementation is deferred to col-007 (which first needs them).
- FR-06.8: The `unimatrix-engine` crate uses `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89, consistent with all other workspace crates.

### FR-07: Layered Authentication

- FR-07.1: On accepting a UDS connection, the server extracts peer credentials using `SO_PEERCRED` (Linux) to obtain the connecting process's UID, GID, and PID. On macOS, `getpeereid()` is used to obtain UID and GID (PID is unavailable).
- FR-07.2: The server verifies that the connecting process's UID matches the server's own UID (`std::process::id()` is not used; the server compares effective UIDs). Connections from different UIDs are rejected with `TransportError::Rejected { code: -32001, message: "uid mismatch" }`.
- FR-07.3: On Linux, when a PID is available from `SO_PEERCRED`, the server performs process lineage verification by reading `/proc/{pid}/cmdline` and checking that the command line contains "unimatrix-server". This confirms the connecting process is the hook subcommand of the same binary.
- FR-07.4: On macOS, Layer 3 (process lineage) is unavailable. Authentication degrades to Layer 1 (filesystem permissions: socket mode 0o600) plus Layer 2 (UID verification via `getpeereid`). This is documented as the expected macOS behavior (SR-04).
- FR-07.5: Authentication failure at any layer closes the connection immediately. No response is sent. A warning is logged to the server's stderr with the peer UID and (if available) PID.
- FR-07.6: The `cortical-implant` agent is pre-enrolled in `bootstrap_defaults()` with `TrustLevel::Internal` and capabilities `[Read, Search]`. This agent identity is associated with all UDS requests.
- FR-07.7: No tokens, passwords, or configuration files are required for authentication. All three layers are zero-ceremony.

### FR-08: Graceful Degradation

- FR-08.1: When the hook subcommand cannot connect to the UDS (socket file does not exist, connection refused), it enters degradation mode.
- FR-08.2: In degradation mode, for fire-and-forget events (SessionStart, Stop, RecordEvent), the hook subcommand writes the event to a local event queue file and exits with code 0.
- FR-08.3: In degradation mode, for synchronous queries (future: ContextSearch, Briefing), the hook subcommand writes nothing to stdout and exits with code 0. The agent operates without enrichment.
- FR-08.4: The event queue directory is `~/.unimatrix/{project_hash}/event-queue/`. Files are named `pending-{unix_timestamp_millis}.jsonl`.
- FR-08.5: Each event is written as a single JSON line to the queue file. Events from a single hook invocation are appended to the most recent pending file if it has fewer than 1000 lines. Otherwise, a new file is created.
- FR-08.6: The event queue respects size limits: maximum 1000 events per file, maximum 10 files in the queue directory.
- FR-08.7: When the file count exceeds 10, the oldest file is deleted before writing a new one.
- FR-08.8: Event queue files older than 7 days are pruned on each hook invocation that accesses the queue directory.
- FR-08.9: Queue replay is not implemented in col-006. The queue provides durability for future features (col-010 session lifecycle) to drain on server startup. The queue file format is stable.
- FR-08.10: All degradation paths produce exit code 0. The hook subcommand never blocks the user's workflow.

### FR-09: Hook Configuration Documentation

- FR-09.1: The feature documentation includes a copy-paste JSON block for `.claude/settings.json` that registers the `SessionStart` and `Stop` hooks using the `unimatrix-server hook <EVENT>` command.
- FR-09.2: The hook configuration uses the command format: `"command": "unimatrix-server hook SessionStart"` (and `Stop` respectively).
- FR-09.3: The documentation notes that this is a manual configuration step. Automated configuration is deferred to alc-003.
- FR-09.4: Existing observation hooks (col-002 bash scripts) coexist with the new hook subcommand. Both are registered in `.claude/settings.json`. col-006 does not replace the col-002 hooks.

### FR-10: SessionStart and Stop Smoke Tests

- FR-10.1: The `SessionStart` hook event is handled by the hook subcommand. It parses session identity from stdin JSON, sends a `SessionRegister` request via UDS, and exits.
- FR-10.2: The `Stop` hook event is handled by the hook subcommand. It sends a `SessionClose` request via UDS and exits.
- FR-10.3: A `Ping` request type is supported. The hook subcommand can send `Ping` and receive `Pong { version }` as a connectivity test.
- FR-10.4: The server-side handler for `SessionRegister` logs the session registration to stderr and responds with `Ack`. Session state persistence (writing to SESSIONS table) is deferred to col-010.
- FR-10.5: The server-side handler for `SessionClose` logs the session close and responds with `Ack`. Session cleanup logic is deferred to col-010.
- FR-10.6: The server-side handler for `Ping` responds with `Pong { version }` where `version` is the crate version from `Cargo.toml`.

## Non-Functional Requirements

### NFR-01: Latency

- The end-to-end round-trip for a `Ping`/`Pong` exchange must complete in under 50ms, measured from hook process start (fork+exec) through UDS connect, request send, server processing, response receive, and stdout write.
- Individual latency components: process startup <5ms, socket connect <1ms, request serialization <1ms, server dispatch <5ms, response deserialization <1ms.
- The hook subcommand startup path must not initialize tokio runtime, ONNX, redb, or HNSW index. It uses only synchronous standard library I/O.

### NFR-02: Zero Regression

- All existing MCP tools (10 tools: context_{search, lookup, get, store, correct, deprecate, status, briefing, quarantine, enroll}) must function identically after the UDS listener is added and the engine extraction is complete.
- The existing test suite (1025 unit + 174 integration tests) must pass without modification after all changes.
- The stdio MCP transport must not be affected by the presence or activity of the UDS transport.

### NFR-03: Reliability

- The UDS listener must handle connection failures (broken pipe, client disconnect mid-request) without affecting the server or other connections.
- A crash or timeout on one UDS connection must not affect the stdio MCP transport or other UDS connections.
- The server must not leak resources (file descriptors, tokio tasks) from failed UDS connections.

### NFR-04: Security

- Socket file permissions `0o600` prevent access by other users on the system.
- UID verification prevents connections from processes running as different users.
- Process lineage verification (Linux only) confirms the connecting process is the same `unimatrix-server` binary.
- No network exposure: the UDS socket is filesystem-local. No TCP, HTTP, or network listeners are opened.

### NFR-05: Platform Compatibility

- The UDS transport must work on Linux (x86_64 and aarch64) and macOS (aarch64).
- `SO_PEERCRED` is used on Linux. `getpeereid()` is used on macOS. Platform-specific code uses `#[cfg(target_os = "...")]`.
- Windows support (named pipes) is deferred. The `Transport` trait abstraction enables future Windows implementations without changes to callers.

### NFR-06: Code Quality

- `#![forbid(unsafe_code)]` on the `unimatrix-engine` crate.
- Edition 2024, MSRV 1.89 on all new and modified crates.
- No new external crate dependencies beyond what is already in the workspace, except for platform-specific peer credential extraction if needed (e.g., `libc` types for `SO_PEERCRED` structure definition, accessed via safe wrappers).

### NFR-07: Binary Size

- The hook subcommand dispatch logic adds no more than 200 KB to the `unimatrix-server` release binary (SR-08).
- No new heavy dependencies are introduced.

## Acceptance Criteria

| AC-ID | Description | Verification Method | Conditions |
|-------|-------------|---------------------|------------|
| AC-01 | UDS listener starts on `~/.unimatrix/{project_hash}/unimatrix.sock` alongside stdio transport, with socket permissions 0o600 | integration test | Server startup creates socket file. `stat` confirms mode 0o600. Server accepts connections on the socket while simultaneously handling MCP tool calls on stdio. |
| AC-02 | UDS listener handles concurrent connections without blocking stdio MCP transport | integration test | Two UDS clients connect simultaneously, send Ping requests, and both receive Pong responses. A stdio MCP tool call executes concurrently without delay. |
| AC-03 | `unimatrix-server hook <EVENT>` reads Claude Code hook JSON from stdin, connects to UDS, dispatches event | integration test | Pipe synthetic `{"hook_event_name": "SessionStart", "session_id": "test-123", "cwd": "/tmp/project"}` to `unimatrix-server hook SessionStart`. Verify the hook process connects to the UDS and the server receives a `SessionRegister` request. Exit code is 0. |
| AC-04 | Ping/Pong round-trip under 50ms end-to-end | benchmark test | Spawn `unimatrix-server hook Ping` (or equivalent) with timing instrumentation. Measure wall-clock time from process start to exit. Assert <50ms. Run 10 iterations, assert p95 <50ms. |
| AC-05 | `Transport` trait defined with 5 methods; `LocalTransport` implements it over UDS | unit test | `LocalTransport::connect()` succeeds against a running server. `request(Ping, 50ms)` returns `Pong`. `fire_and_forget(RecordEvent(...))` returns `Ok(())`. `is_connected()` returns true after connect, false after disconnect. `disconnect()` closes cleanly. |
| AC-06 | `unimatrix-engine` crate exists with `confidence`, `coaccess`, `project` modules; all 174+ integration tests pass without modification | cargo test | `cargo test --workspace` passes. The `unimatrix-engine` crate is in `Cargo.toml` workspace members. `confidence.rs`, `coaccess.rs`, `project.rs` are importable from `unimatrix_engine`. No copies remain in `unimatrix-server` (modules are re-imported, not duplicated). |
| AC-07 | UDS handler authenticates via UID verification; rejects connections from different users | unit test | On Linux: mock `SO_PEERCRED` with a different UID. Verify connection is rejected. On both platforms: connect from the same user succeeds. A synthetic connection with mismatched UID is rejected with error code -32001. |
| AC-08 | Hook subcommand exits 0 when server is unavailable; queues fire-and-forget events | unit test + filesystem check | Run `unimatrix-server hook SessionStart` with synthetic stdin but no running server (no socket file). Verify exit code 0. Verify a queue file exists at `~/.unimatrix/{hash}/event-queue/pending-*.jsonl` containing the serialized event. |
| AC-09 | Stale socket files detected and cleaned up on server startup | integration test | Create a socket file at the bind path (simulating a crashed server). Start the server. Verify the stale socket is removed and a new socket is bound. Server starts successfully. |
| AC-10 | Socket cleanup on graceful shutdown; socket file does not persist after clean exit | integration test | Start the server, verify socket exists. Send shutdown signal (SIGTERM). Verify socket file is removed before the process exits. Verify PID file is also cleaned up. |
| AC-11 | `cortical-implant` agent pre-enrolled as Internal trust in `bootstrap_defaults()` | unit test | After `bootstrap_defaults()`, call `resolve_or_enroll("cortical-implant")`. Verify `trust_level` is `Internal`. Verify capabilities include `Read` and `Search`. Verify the agent exists in `AGENT_REGISTRY`. |
| AC-12 | Event queue respects size limits: 1000 events/file, 10 files max | unit test | Write 1001 events -- verify a second file is created after 1000. Write events across 11 files -- verify the oldest file is deleted when the 11th is created. Create queue files with timestamps >7 days old -- verify they are pruned on next hook invocation. |
| AC-13 | SessionStart and Stop hooks round-trip through the full chain | end-to-end test | Start server. Pipe SessionStart JSON to `unimatrix-server hook SessionStart`. Verify server log confirms session registration. Pipe Stop JSON to `unimatrix-server hook Stop`. Verify server log confirms session close. Both hook processes exit 0. The full chain is validated: hook stdin -> parse -> UDS connect -> authenticate -> server dispatch -> response -> hook exit. |

## Domain Models

### HookEvent (input from Claude Code)

The JSON payload provided by Claude Code on stdin to the hook subcommand. Parsed defensively.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `hook_event_name` | `String` | Yes | The hook event type (e.g., "SessionStart", "Stop", "UserPromptSubmit") |
| `session_id` | `Option<String>` | No | Claude Code session identifier. May not be present in all events. |
| `cwd` | `Option<String>` | No | Working directory of the Claude Code session. Used for project hash computation. Falls back to process cwd if absent. |
| `transcript_path` | `Option<String>` | No | Path to the session transcript file. Not used by col-006. |
| `_extra` | `HashMap<String, Value>` | No | Captured via `#[serde(flatten)]`. Holds unknown fields for forward compatibility. |

### Request (IPC protocol)

Type-tagged enum sent from hook process to server over UDS.

| Variant | Fields | Semantics |
|---------|--------|-----------|
| `Ping` | (none) | Health check. Server responds with `Pong`. |
| `SessionRegister` | `session_id: String`, `agent_role: Option<String>`, `feature: Option<String>` | Register a new session. Fire-and-forget in col-006; becomes stateful in col-010. |
| `SessionClose` | `session_id: String`, `outcome: Option<String>`, `duration_secs: u64` | Close a session. Fire-and-forget in col-006. |
| `RecordEvent` | `ImplantEvent` | Record a single telemetry event. Fire-and-forget. |
| `RecordEvents` | `Vec<ImplantEvent>` | Record multiple telemetry events in one round-trip. Fire-and-forget. |
| `ContextSearch` | `query, role, task, feature, k, max_tokens` | (Stub) Semantic search. Deferred to col-007. |
| `Briefing` | `role, task, feature, max_tokens` | (Stub) Compiled briefing. Deferred to col-008. |
| `CompactPayload` | `session_id, injected_entry_ids, role, feature, token_limit` | (Stub) Compaction defense payload. Deferred to col-008. |

### Response (IPC protocol)

Type-tagged enum sent from server to hook process over UDS.

| Variant | Fields | Semantics |
|---------|--------|-----------|
| `Pong` | `version: String` | Server is alive. Returns the server's crate version. |
| `Ack` | (none) | Acknowledgment of a fire-and-forget request. |
| `Error` | `code: i32, message: String` | Server rejected the request. |
| `Entries` | `items: Vec<EntryPayload>, total_tokens: u32` | (Stub) Search results. Deferred to col-007. |
| `BriefingContent` | `content: String, token_count: u32` | (Stub) Briefing text. Deferred to col-008. |

### TransportError

| Variant | Meaning |
|---------|---------|
| `Unavailable(String)` | Server not running, socket not found, connection refused. |
| `Timeout(Duration)` | Operation timed out waiting for response. |
| `Rejected { code: i32, message: String }` | Server rejected the request (auth, validation). |
| `Codec(String)` | JSON serialization or deserialization failure, message too large. |
| `Transport(String)` | Socket-level error (broken pipe, I/O error). |

### Error Codes (Rejected variant)

| Code | Meaning |
|------|---------|
| -32001 | UID mismatch (authentication failure) |
| -32002 | Process lineage verification failed |
| -32003 | Unknown request type |
| -32004 | Invalid request payload |
| -32005 | Internal server error |

### ImplantEvent

| Field | Type | Description |
|-------|------|-------------|
| `event_type` | `String` | Hook event type (e.g., "session_start", "session_close", "tool_use") |
| `session_id` | `String` | Session identifier (from Claude Code or generated) |
| `timestamp` | `u64` | Unix epoch seconds when the event occurred |
| `payload` | `serde_json::Value` | Event-specific data (varies by event_type) |

### EntryPayload (stub for future features)

| Field | Type | Description |
|-------|------|-------------|
| `id` | `u64` | Entry ID |
| `title` | `String` | Entry title |
| `content` | `String` | Entry content (potentially truncated to token budget) |
| `confidence` | `f64` | Current confidence score |
| `similarity` | `f64` | Similarity to query (for search results) |
| `category` | `String` | Entry category |

### SocketGuard

RAII guard for socket file lifecycle, analogous to `PidGuard`.

| State | Description |
|-------|-------------|
| `Bound` | Socket file exists, listener is active. Normal operating state. |
| `Dropped` | Socket file removed, listener stopped. Terminal state (happens automatically on drop). |

### ProjectPaths (extended)

The existing `ProjectPaths` struct gains one new field:

| Field | Type | Description |
|-------|------|-------------|
| `socket_path` | `PathBuf` | `~/.unimatrix/{project_hash}/unimatrix.sock` |

### AuthContext

Peer credential information extracted from a UDS connection.

| Field | Type | Description |
|-------|------|-------------|
| `uid` | `u32` | Effective UID of the connecting process |
| `gid` | `u32` | Effective GID of the connecting process |
| `pid` | `Option<u32>` | PID of the connecting process (Linux only, via `SO_PEERCRED`) |

### EventQueueFile

| Property | Value |
|----------|-------|
| Location | `~/.unimatrix/{project_hash}/event-queue/` |
| Naming | `pending-{unix_timestamp_millis}.jsonl` |
| Format | One JSON object per line (JSONL) |
| Max lines per file | 1000 |
| Max files | 10 |
| Retention | 7 days |

## User Workflows

### Workflow 1: Normal Hook Execution (Server Running)

1. Claude Code fires a SessionStart hook event.
2. Claude Code spawns `unimatrix-server hook SessionStart` and pipes event JSON to stdin.
3. The hook subcommand parses stdin JSON, extracts `cwd`, computes project hash.
4. The hook subcommand locates `~/.unimatrix/{hash}/unimatrix.sock`.
5. The hook subcommand connects to the UDS, constructs a `SessionRegister` request, sends it.
6. The server authenticates the connection (UID match, lineage check on Linux).
7. The server dispatches the request, logs the session, responds with `Ack`.
8. The hook subcommand exits with code 0.

### Workflow 2: Graceful Degradation (Server Not Running)

1. Claude Code fires a SessionStart hook event.
2. Claude Code spawns `unimatrix-server hook SessionStart` and pipes event JSON to stdin.
3. The hook subcommand parses stdin JSON, computes project hash.
4. The hook subcommand looks for `unimatrix.sock` -- file does not exist.
5. The hook subcommand writes the event to `~/.unimatrix/{hash}/event-queue/pending-{ts}.jsonl`.
6. The hook subcommand exits with code 0. No error visible to the user.
7. On next server startup, the event queue is available for future replay (col-010).

### Workflow 3: Server Startup with Stale Socket

1. Previous server crashed (SIGKILL), leaving `unimatrix.sock` on disk.
2. New server instance starts.
3. PidGuard resolves stale PID (existing behavior).
4. Server detects existing socket file at bind path.
5. Server removes the stale socket file (unconditional unlink, safe after PidGuard).
6. Server binds new `UnixListener` on the socket path.
7. Server proceeds to accept connections on both stdio and UDS.

### Workflow 4: Server Graceful Shutdown

1. MCP session closes or SIGTERM received.
2. Shutdown sequence begins.
3. UDS listener stops accepting new connections.
4. In-flight UDS requests drain (1-second timeout).
5. `SocketGuard` drops: socket file is removed.
6. Vector index is dumped; adaptation state saved.
7. Database is compacted.
8. `PidGuard` drops: PID file is removed.
9. Process exits.

### Workflow 5: Connectivity Test (Ping/Pong)

1. Developer runs `unimatrix-server hook Ping` (or an integration test does).
2. Hook subcommand connects to UDS, sends `Ping` request.
3. Server responds with `Pong { version: "0.x.y" }`.
4. Hook subcommand prints the version to stdout and exits with code 0.
5. Round-trip measured at <50ms.

## Interface Contracts

### Wire Protocol Format

```
[4 bytes: u32 big-endian length][JSON payload bytes]
```

- Length is the byte count of the JSON payload, not including the 4-byte prefix.
- Maximum payload size: 1,048,576 bytes (1 MB).
- JSON is UTF-8 encoded.
- Requests and responses use the same framing.

### Hook Subcommand CLI

```
unimatrix-server hook <EVENT> [--project-dir <DIR>]
```

- `EVENT`: positional, required. The Claude Code hook event name.
- `--project-dir`: optional. Override project root for hash computation. Inherited from the top-level CLI.
- stdin: Claude Code hook JSON payload.
- stdout: response payload (for synchronous hooks) or empty (for fire-and-forget).
- stderr: diagnostic logging.
- Exit code: 0 (success or graceful degradation), 1 (unexpected error).

### Hook Configuration (`.claude/settings.json`)

```json
{
  "hooks": {
    "SessionStart": [{
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook SessionStart"
      }]
    }],
    "Stop": [{
      "hooks": [{
        "type": "command",
        "command": "unimatrix-server hook Stop"
      }]
    }]
  }
}
```

### Startup Order

```
1. Parse CLI args (clap)
2. Initialize tracing
3. Detect project root, compute hash, ensure data directory
4. Handle stale PID file
5. Open database (with retry)
6. Acquire PidGuard
7. Remove stale socket file (unconditional unlink)
8. Bind UDS listener, spawn accept loop
9. Initialize vector index, embed handle, registry, audit, etc.
10. Bootstrap defaults (including "cortical-implant" agent)
11. Build UnimatrixServer
12. Serve MCP over stdio
```

### Shutdown Order

```
1. MCP session close or signal received
2. Stop UDS accept loop
3. Drain in-flight UDS requests (1s timeout)
4. SocketGuard drops (remove socket file)
5. Dump vector index
6. Save adaptation state
7. Drop Arc<Store> holders (registry, audit, adapt_service, vector_index)
8. Compact database
9. PidGuard drops (remove PID file)
```

## Integration Points

### PidGuard Coordination (vnc-004)

The socket lifecycle is tightly coordinated with PidGuard:
- PidGuard acquisition (step 6) precedes socket binding (step 8). If PidGuard fails, no socket is created.
- PidGuard's stale process detection already handles the mutual exclusion guarantee. By the time socket binding occurs, the server knows it is the sole instance.
- The unconditional socket unlink before bind (step 7) is safe because PidGuard has already established mutual exclusion.
- Socket cleanup (step 4 of shutdown) occurs before PidGuard cleanup (step 9 of shutdown).

### LifecycleHandles Extension

`LifecycleHandles` gains a `socket_guard: Option<SocketGuard>` field. The shutdown sequence drops the `SocketGuard` after draining UDS connections but before vector dump and compaction.

### Agent Registry (alc-002)

`bootstrap_defaults()` is extended with a third pre-enrolled agent:

| Agent ID | Trust Level | Capabilities | Purpose |
|----------|-------------|-------------|---------|
| `system` | System | Read, Write, Search, Admin | Server internals (existing) |
| `human` | Privileged | Read, Write, Search, Admin | Human user (existing) |
| `cortical-implant` | Internal | Read, Search | Hook transport agent (new) |

The `cortical-implant` agent is idempotent: if it already exists (from a prior server run), `bootstrap_defaults()` does not modify it.

### Crate Dependency Graph (after extraction)

```
unimatrix-embed     (unchanged)
       |
       v
unimatrix-store     (unchanged)
       |
       v
unimatrix-vector    (unchanged)
       |
       v
unimatrix-core      (unchanged)
       |
       v
unimatrix-engine    (NEW: confidence, coaccess, project, search stub, query stub)
       |
       v
unimatrix-server    (modified: imports from engine, adds UDS listener + hook subcommand)
```

### Existing Module Mapping (post-extraction)

| Module | Before | After |
|--------|--------|-------|
| `confidence.rs` | `unimatrix-server` | `unimatrix-engine` (server re-imports) |
| `coaccess.rs` | `unimatrix-server` | `unimatrix-engine` (server re-imports) |
| `project.rs` | `unimatrix-server` | `unimatrix-engine` (server re-imports, hook subcommand also imports) |
| `search.rs` | does not exist | `unimatrix-engine` (stub) |
| `query.rs` | does not exist | `unimatrix-engine` (stub) |

## Constraints

### Hard Constraints

- **redb exclusive file lock**: Hook processes cannot open the database. All data access goes through IPC to the running MCP server. Non-negotiable with redb v3.1.x (SR-10, D14-2 RQ-2).
- **50ms latency budget**: End-to-end hook execution (process start to exit) must complete within 50ms for synchronous hooks. This includes process startup, IPC round-trip, and response formatting (SR-03).
- **Zero regression**: All 10 existing MCP tools must function identically. The 1025 unit + 174 integration tests must pass without modification (SR-01, AC-06).
- **Single binary**: The hook subcommand is part of `unimatrix-server`. No separate binary (D14-5 RQ-5).
- **`#![forbid(unsafe_code)]`**: On all new crates and modified crates within the workspace.

### Soft Constraints

- **Linux + macOS only**: UDS transport. Windows named pipes deferred (NFR-05).
- **Manual hook configuration**: Users edit `.claude/settings.json` manually. Automated setup is alc-003 scope.
- **No new redb tables**: col-006 does not create telemetry tables (SESSIONS, INJECTION_LOG, SIGNAL_QUEUE). Those are deferred to col-010 which first writes to them.
- **No schema migration**: Schema version remains v3. No EntryRecord field additions.

## Dependencies

### Existing Crate Dependencies

| Crate | Role |
|-------|------|
| `unimatrix-store` | redb storage, `Store`, `EntryRecord`, tables |
| `unimatrix-vector` | HNSW index, `VectorIndex` |
| `unimatrix-embed` | ONNX embedding pipeline |
| `unimatrix-core` | Traits (`EntryStore`, `VectorStore`, `EmbedService`), adapters, async wrappers |
| `unimatrix-adapt` | Adaptation service (crt-006) |
| `unimatrix-observe` | Observation pipeline (col-002) |

### New Crate

| Crate | Dependencies | Purpose |
|-------|-------------|---------|
| `unimatrix-engine` | `unimatrix-core`, `unimatrix-store` | Shared business logic: confidence, co-access, project discovery |

### External Dependencies

| Dependency | Already in workspace? | Purpose |
|-----------|----------------------|---------|
| `serde` + `serde_json` | Yes | Request/Response serialization for wire protocol |
| `clap` | Yes | Hook subcommand argument parsing |
| `sha2` | Yes | Project hash computation (moved to engine) |
| `dirs` | Yes | Home directory resolution (moved to engine) |
| `fs2` | Yes | Advisory file locking (PidGuard pattern, SocketGuard) |
| `tokio` | Yes | UDS listener in server (server-side only) |
| `tracing` | Yes | Diagnostic logging |

### Feature Dependencies

| Feature | Relationship |
|---------|-------------|
| vnc-001 through vnc-004 | col-006 extends the existing server |
| All core crates (nxs-001 through nxs-004) | Foundation |
| crt-001 through crt-005 | Confidence, co-access logic extracted to engine |
| alc-002 | Agent enrollment; `cortical-implant` added to bootstrap |
| col-007 through col-011 | Depend on col-006 transport; col-006 has no dependency on them |

## NOT in Scope

- **Context injection logic** -- col-007 implements UserPromptSubmit knowledge injection.
- **Compaction resilience** -- col-008 implements PreCompact knowledge preservation.
- **Confidence feedback** -- col-009 implements implicit helpfulness signals.
- **Session lifecycle tracking** -- col-010 implements SESSIONS table, schema v4, structured event ingestion.
- **Agent routing** -- col-011 implements semantic agent matching.
- **Telemetry tables** -- SESSIONS, INJECTION_LOG, SIGNAL_QUEUE are deferred to col-010.
- **Schema v4 migration** -- Deferred to col-010.
- **Queue replay** -- The event queue provides durable storage; draining/replaying is col-010 scope.
- **Daemon architecture** -- Future optimization; col-006 uses ephemeral hook processes.
- **Remote/HTTPS transport** -- Future centralized deployment; col-006 is local-only UDS.
- **Windows named pipe transport** -- P2 platform; col-006 targets Linux + macOS.
- **`unimatrix init` auto-configuration** -- alc-003 scope.
- **Replacing existing col-002 observation hooks** -- col-006 hooks coexist with existing bash hooks.
- **`search.rs` and `query.rs` implementation** -- Stubs only in col-006; full implementation in col-007.
- **Connection pooling or persistent connections** -- col-006 uses one-connection-per-request for simplicity.
- **Shared secret fallback authentication** -- Deferred unless shared-environment deployment is an immediate need (SR-04).

## Open Questions

1. **Session identity**: Claude Code may or may not expose `session_id` in hook JSON. If unavailable, should the hook subcommand use parent PID as a session proxy, generate its own UUID, or pass an empty string? Recommendation from ASS-014: use `session_id` from stdin JSON if present, fall back to parent PID (available via `std::os::unix::process::parent_id()` on nightly or `/proc/self/stat` parsing on Linux), then fall back to empty string. The architect should decide the exact fallback chain.

2. **Hook subcommand runtime**: Should the hook subcommand use synchronous `std::os::unix::net::UnixStream` (avoiding tokio entirely) or a minimal tokio runtime? Synchronous is simpler and avoids ~1-3ms tokio startup overhead, but limits future extensibility. The specification recommends synchronous; the architect should confirm.

3. **Engine extraction: `search.rs` and `query.rs` granularity**: Should these be empty stubs with only type signatures, or should they include the interface contracts (trait definitions, function signatures with documentation) without implementation bodies? Recommendation: include documented function signatures with `todo!()` bodies behind a `#[cfg(feature = "full")]` gate that is not enabled in col-006.
