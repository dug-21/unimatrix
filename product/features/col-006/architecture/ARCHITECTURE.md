# col-006: Hook Transport Layer — Architecture

## System Overview

col-006 adds a second communication channel to the Unimatrix MCP server: a Unix domain socket (UDS) listener that accepts hook-driven requests from ephemeral Claude Code hook processes. This is the foundation for all automatic knowledge delivery (col-007 through col-011).

The architecture is forced by a single hard constraint: redb v3.1.x acquires an exclusive file lock on `Database::create()`. No second process can open the database while the MCP server is running. Every hook process must communicate with the server via IPC. This constraint shapes every component boundary, lifecycle decision, and error path in this feature.

### Position in the System

```
                     Claude Code Process
                            |
           spawns hook process per lifecycle event
                            |
                            v
                 +---------------------------+
                 |  unimatrix-server hook     |  Ephemeral process (~5-15ms)
                 |  <EVENT>                   |  Reads JSON from stdin
                 |                            |  Writes injection to stdout
                 |  (same binary, hook subcmd) |  Writes diagnostics to stderr
                 +------------+--------------+
                              |
               Unix domain socket (unimatrix.sock)
               Length-prefixed JSON, sync req/resp
                              |
                              v
                 +---------------------------+
                 |  unimatrix-server          |  Long-running MCP server
                 |                            |
                 |  +----------+ +----------+ |
                 |  | stdio    | |   UDS    | |  Two listeners:
                 |  | (MCP)    | | (hooks)  | |  - stdio for MCP tools (existing)
                 |  +----+-----+ +-----+----+ |  - UDS for hook requests (new)
                 |       |             |       |
                 |       v             v       |
                 |  +------------------------+ |
                 |  | unimatrix-engine       | |  Shared business logic
                 |  | (confidence, coaccess, | |  (extracted from server)
                 |  |  project)              | |
                 |  +-----------+------------+ |
                 |              |               |
                 |  +-----------v------------+ |
                 |  |  redb (14 tables)      | |  Single-writer, exclusive lock
                 |  |  Knowledge tier         | |  (unchanged by col-006)
                 |  +------------------------+ |
                 +---------------------------+
```

### Crate Dependency Graph (After col-006)

```
unimatrix-embed     (ONNX pipeline, EmbeddingProvider trait — unchanged)
       |
       v
unimatrix-store     (redb Store, EntryRecord, schema — unchanged)
       |
       v
unimatrix-vector    (HNSW index, persistence — unchanged)
       |
       v
unimatrix-core      (traits, adapters, async wrappers — unchanged)
       |
       v
unimatrix-engine    (NEW: shared business logic)
  |-- confidence.rs     (moved from server)
  |-- coaccess.rs       (moved from server)
  |-- project.rs        (moved from server)
  |-- wire.rs           (NEW: wire protocol types)
  |-- transport.rs      (NEW: Transport trait + LocalTransport)
  |-- auth.rs           (NEW: peer credential verification)
  |-- event_queue.rs    (NEW: graceful degradation queue)
       |
       v
unimatrix-server    (MCP handler — depends on engine)
  |-- server.rs         (UnimatrixServer, now imports from engine)
  |-- tools.rs          (MCP tool implementations — unchanged logic)
  |-- uds_listener.rs   (NEW: UDS accept loop + handler dispatch)
  |-- hook.rs           (NEW: hook subcommand dispatch)
  |-- main.rs           (extended with hook subcommand + UDS startup)
  |-- shutdown.rs       (extended with socket cleanup)
  |-- ... (remaining modules unchanged)
```

---

## Component Breakdown

### Component 1: unimatrix-engine Crate (Shared Business Logic)

**Responsibility:** Hold business logic that both the MCP tool handlers and the UDS hook handlers need: confidence computation, co-access boost computation, project discovery, and the wire protocol types used for IPC.

**Modules extracted from unimatrix-server:**

| Module | Current Location | What Moves | What Stays in Server |
|--------|-----------------|------------|---------------------|
| `confidence.rs` | `server::confidence` | All functions and constants: `compute_confidence`, `rerank_score`, `co_access_affinity`, weight constants, component score functions | Nothing — entire module moves |
| `coaccess.rs` | `server::coaccess` | All functions and constants: `generate_pairs`, `compute_search_boost`, `compute_briefing_boost`, internal `co_access_boost`, `compute_boost_internal`, all constants | Nothing — entire module moves |
| `project.rs` | `server::project` | All functions and types: `ProjectPaths`, `detect_project_root`, `compute_project_hash`, `ensure_data_directory` | Nothing — entire module moves |

**New modules in unimatrix-engine:**

| Module | Responsibility |
|--------|---------------|
| `wire.rs` | Wire protocol types: `HookRequest`, `HookResponse`, `HookEvent`, serde models for length-prefixed JSON |
| `transport.rs` | `Transport` trait and `LocalTransport` (UDS client implementation) |
| `auth.rs` | Peer credential extraction and UID verification |
| `event_queue.rs` | Local event queue for graceful degradation |

**Extraction strategy (ADR-001):** Move modules one at a time in strict order: `project.rs` first (no internal dependencies), then `confidence.rs` (depends only on `unimatrix-core::EntryRecord`), then `coaccess.rs` (depends on `confidence.rs` for `co_access_affinity` reference and on `unimatrix-store::Store`). After each move, `unimatrix-server` re-exports from `unimatrix-engine` to maintain backward compatibility for integration tests. Full 1199-test suite runs after each move.

**Re-export pattern:**
```rust
// In unimatrix-server/src/lib.rs, after extraction:
pub use unimatrix_engine::confidence;
pub use unimatrix_engine::coaccess;
pub use unimatrix_engine::project;
```

This ensures all existing integration tests that import `unimatrix_server::confidence` continue to compile without modification.

### Component 2: UDS Listener

**Responsibility:** Accept Unix domain socket connections from hook processes, authenticate them, parse length-prefixed JSON requests, dispatch to handlers, and return responses.

**Location:** `unimatrix-server/src/uds_listener.rs`

**Integration point:** The UDS listener runs as a tokio task spawned during server startup, alongside the existing stdio MCP transport. It shares the same `Arc<Store>`, `Arc<VectorIndex>`, and `Arc<EmbedServiceHandle>` as the MCP server. It does NOT share the rmcp router — it has its own request dispatch logic.

**Concurrency model:** One tokio task per accepted connection. No connection limit for col-006 (the expected concurrency during swarm runs is 5-10 connections; tokio handles thousands). Future features may add backpressure if needed.

**Connection lifecycle:**
1. `UnixListener::bind()` on `~/.unimatrix/{project_hash}/unimatrix.sock`
2. `listener.accept()` loop spawns a task per connection
3. Each task: authenticate (peer cred) -> read framed request -> dispatch -> write framed response -> close
4. Connection is single-request: one request, one response, then close. No pipelining for col-006.

### Component 3: Hook Subcommand

**Responsibility:** Parse Claude Code hook JSON from stdin, discover the running server instance, connect via UDS, send the request, receive the response (or fire-and-forget), write to stdout, and exit.

**Location:** `unimatrix-server/src/hook.rs` (handler logic), integrated into `main.rs` via clap subcommand.

**Execution path (minimal initialization):**
1. Clap parses `hook <EVENT>` subcommand (no tokio runtime needed in hook path)
2. Read stdin to string (blocking I/O)
3. Parse JSON with `serde_json` (defensive: `#[serde(default)]`, unknown fields ignored)
4. Compute project hash from `cwd` field (or fall back to current working directory)
5. Construct socket path: `~/.unimatrix/{project_hash}/unimatrix.sock`
6. Connect `std::os::unix::net::UnixStream` with timeout
7. Write length-prefixed JSON request
8. For synchronous hooks: read length-prefixed JSON response, write to stdout
9. For fire-and-forget hooks: write request, exit immediately
10. Exit code 0 (always — errors go to stderr)

**No tokio runtime:** The hook process uses blocking `std::os::unix::net::UnixStream` with `SO_RCVTIMEO`/`SO_SNDTIMEO` for timeouts. This eliminates the ~1-3ms tokio runtime initialization cost, keeping the total under 50ms (ADR-002).

### Component 4: Wire Protocol

**Responsibility:** Define the framing format and message types for IPC between hook processes and the UDS listener.

**Location:** `unimatrix-engine/src/wire.rs`

**Framing:** Length-prefixed JSON. Each message is:
- 4 bytes: big-endian `u32` payload length
- N bytes: JSON payload (UTF-8)

**Message types:**

```
HookRequest (type-tagged enum):
  Ping                                    -- connectivity test
  SessionRegister { session_id, cwd, ... }  -- register new session
  SessionClose { session_id, ... }          -- close session
  RecordEvent { event_type, payload }       -- fire-and-forget telemetry
  (future: ContextSearch, CompactPayload, etc. for col-007+)

HookResponse (type-tagged enum):
  Pong { server_version }                 -- ping response
  Ack                                     -- fire-and-forget acknowledgment
  Error { code, message }                 -- error response
  (future: Entries, Briefing for col-007+)
```

**Versioning:** The `Ping` request returns the server version in `Pong`. No explicit protocol version negotiation for col-006 — the bundled subcommand guarantees version alignment (same binary). If a future remote transport needs version negotiation, it adds a `Handshake` request type.

### Component 5: Transport Trait + LocalTransport

**Responsibility:** Abstract the IPC mechanism so future transports (TCP, named pipes) can be added without changing hook dispatch logic.

**Location:** `unimatrix-engine/src/transport.rs`

**Trait design (ADR-002):**
```
Transport (sync public API):
  fn connect(&mut self) -> Result<(), TransportError>
  fn request(&mut self, req: &HookRequest, timeout: Duration) -> Result<HookResponse, TransportError>
  fn fire_and_forget(&mut self, req: &HookRequest) -> Result<(), TransportError>
  fn disconnect(&mut self)
  fn is_connected(&self) -> bool
```

`LocalTransport` implements this over `std::os::unix::net::UnixStream`:
- `connect()`: opens UDS connection to socket path, sets `SO_RCVTIMEO`/`SO_SNDTIMEO`
- `request()`: writes length-prefixed JSON, reads length-prefixed JSON response
- `fire_and_forget()`: writes length-prefixed JSON, does not wait for response
- `disconnect()`: drops the `UnixStream`
- `is_connected()`: checks if the stream is open

### Component 6: Layered Authentication

**Responsibility:** Verify that connecting hook processes are authorized to communicate with the server.

**Location:** `unimatrix-engine/src/auth.rs`

**Three layers (ADR-003):**

| Layer | Mechanism | Platform | Provides |
|-------|-----------|----------|----------|
| 1. Filesystem | Socket mode `0o600` | All Unix | Only owner can connect |
| 2. Kernel credentials | `SO_PEERCRED` / `getpeereid` | Linux / macOS | UID of connecting process |
| 3. Process lineage | `/proc/{pid}/cmdline` check | Linux only | Verify binary is unimatrix-server |

**Implementation:**
- On `accept()`, extract peer credentials from the accepted socket
- Verify UID matches server's UID (`std::process::id()` -> `getuid()`)
- On Linux: verify PID's cmdline contains "unimatrix-server" (reuses existing `is_unimatrix_process()` pattern from `pidfile.rs`)
- On macOS: Layer 3 unavailable (no PID from `getpeereid`), accept with Layer 1 + 2
- On auth failure: close connection immediately, log to stderr

**No shared secret for col-006.** Deferred to future feature if shared-environment deployment becomes a requirement. Filesystem permissions + UID verification provide sufficient security for single-user local development.

### Component 7: Graceful Degradation (Event Queue)

**Responsibility:** When the server is unavailable, queue fire-and-forget events for later replay. For synchronous queries, produce no output (silent skip).

**Location:** `unimatrix-engine/src/event_queue.rs`

**Queue location:** `~/.unimatrix/{project_hash}/event-queue/`

**Queue format:** JSONL files named `pending-{unix_timestamp_ms}.jsonl`. One JSON object per line containing the serialized `HookRequest` plus a timestamp.

**Size management:**
- Max 1000 events per file (rotate to new file at limit)
- Max 10 files in queue directory
- 7-day pruning: files older than 7 days deleted on queue write or replay
- Total bounded at ~10,000 events, ~5-10 MB

**Queue replay:** On successful connection to server, check for pending queue files. Replay events oldest-first. Delete files after successful replay. Replay is best-effort: if any event fails to send, skip it and continue.

**Degradation ladder:**
1. Server available: send via UDS (normal path)
2. Server unavailable + fire-and-forget: queue event to JSONL
3. Server unavailable + synchronous query: skip (exit 0, no stdout)
4. Queue full: drop oldest events, log warning

---

## Component Interactions

### Startup Sequence

```
main.rs:
  1. Parse CLI args (Cli struct with Hook subcommand variant)
  2. If subcommand == Hook:
       -> hook::run(event, stdin) (sync path, no tokio)
       -> exit
  3. Else (serve mode, existing path):
       a. Initialize tracing
       b. Initialize project data directory (project::ensure_data_directory)
       c. Handle stale PID file (pidfile::handle_stale_pid_file)
       d. Handle stale socket file (NEW: uds_listener::handle_stale_socket)
       e. Open database with retry (open_store_with_retry)
       f. Acquire PID guard (pidfile::PidGuard::acquire)
       g. Initialize vector index, embed handle, registry, audit, etc.
       h. Bind UDS listener (NEW: uds_listener::bind)
       i. Build UnimatrixServer
       j. Start UDS accept loop (tokio::spawn)
       k. Serve MCP over stdio
       l. Wait for session close or signal
       m. Graceful shutdown
```

**Key ordering invariant:** PidGuard acquired (step f) before UDS bind (step h). This ensures the PidGuard's mutual exclusion has been established before the socket becomes connectable. Socket cleanup (step d) happens before PidGuard acquisition, using the same stale-detection logic: if the socket exists and no process answers on it, remove it.

### Shutdown Sequence

```
graceful_shutdown (extended):
  1. Stop accepting new UDS connections (drop listener)
  2. Drain active UDS request tasks (with 1s timeout)
  3. Remove socket file (uds_listener::cleanup_socket)
  4. Replay queued events (best-effort, from event-queue/)
  5. [existing] Dump vector index
  6. [existing] Save adaptation state
  7. [existing] Drop all Arc<Store> holders
  8. [existing] Compact database
  9. [existing] PidGuard::drop removes PID file
```

**Socket cleanup on crash:** If the server crashes (SIGKILL), the socket file persists. On next startup, `handle_stale_socket()` detects and removes it. The detection strategy is unconditional unlink after PidGuard establishes mutual exclusion (ADR-004). This is safe because PidGuard already ensures we are the only server instance.

### Hook Process to Server Flow

```
Hook process (sync, no tokio):
  stdin JSON -> serde_json::from_str<HookInput>
  -> project::compute_project_hash(cwd)
  -> socket_path = ~/.unimatrix/{hash}/unimatrix.sock
  -> LocalTransport::connect(socket_path, timeout=40ms)
  -> match event:
       SessionStart | Stop -> transport.fire_and_forget(SessionRegister/SessionClose)
       Ping               -> transport.request(Ping) -> stdout Pong
  -> exit 0

Server UDS handler (async, tokio task):
  accept() -> authenticate(peer_cred)
  -> read_framed_request()
  -> match request:
       Ping             -> HookResponse::Pong { version }
       SessionRegister  -> log session (col-006 is transport-only; col-010 adds SESSIONS table)
       SessionClose     -> log close
       RecordEvent      -> log event
  -> write_framed_response()
  -> close connection
```

### Data Flow Between Components

```
unimatrix-engine:
  wire.rs ---------> HookRequest/HookResponse types
       |                    |
  transport.rs              |
  (LocalTransport) -------> serializes/deserializes via wire.rs
       |                    |
  auth.rs                   |
  (PeerCredentials) ------> used by uds_listener on accept
       |
  event_queue.rs
  (EventQueue) -----------> serializes HookRequest to JSONL

unimatrix-server:
  hook.rs ------> uses transport.rs (LocalTransport) to send requests
  uds_listener.rs -> uses auth.rs to verify, wire.rs to parse, dispatches internally
  main.rs -------> orchestrates startup order (PidGuard -> socket -> accept loop)
  shutdown.rs ---> orchestrates cleanup order (stop accept -> remove socket -> compact)
  server.rs -----> UnimatrixServer unchanged; MCP tools use engine::confidence, engine::coaccess
  tools.rs ------> imports confidence/coaccess from engine (via re-export or direct)
```

---

## Integration Surface

| Integration Point | Type/Signature | Source | Notes |
|-------------------|---------------|--------|-------|
| `confidence::compute_confidence` | `fn(entry: &EntryRecord, now: u64) -> f64` | `unimatrix-engine/src/confidence.rs` (moved from server) | All callers updated to import from engine |
| `confidence::rerank_score` | `fn(similarity: f64, confidence: f64) -> f64` | `unimatrix-engine/src/confidence.rs` | Used by search re-ranking |
| `confidence::co_access_affinity` | `fn(partner_count: usize, avg_partner_confidence: f64) -> f64` | `unimatrix-engine/src/confidence.rs` | Cross-references `coaccess::MAX_MEANINGFUL_PARTNERS` |
| `coaccess::compute_search_boost` | `fn(anchor_ids: &[u64], result_ids: &[u64], store: &Store, staleness_cutoff: u64) -> HashMap<u64, f64>` | `unimatrix-engine/src/coaccess.rs` | Depends on `Store` (from unimatrix-store) |
| `coaccess::compute_briefing_boost` | `fn(anchor_ids: &[u64], result_ids: &[u64], store: &Store, staleness_cutoff: u64) -> HashMap<u64, f64>` | `unimatrix-engine/src/coaccess.rs` | Same dependency |
| `project::ProjectPaths` | struct with `project_root`, `project_hash`, `data_dir`, `db_path`, `vector_dir`, `pid_path`, `socket_path` (NEW field) | `unimatrix-engine/src/project.rs` | Extended with `socket_path: PathBuf` |
| `project::ensure_data_directory` | `fn(override_dir: Option<&Path>) -> io::Result<ProjectPaths>` | `unimatrix-engine/src/project.rs` | Now also computes `socket_path` |
| `project::compute_project_hash` | `fn(project_root: &Path) -> String` | `unimatrix-engine/src/project.rs` | Used by hook subcommand for instance discovery |
| `HookRequest` | `enum { Ping, SessionRegister{..}, SessionClose{..}, RecordEvent{..} }` | `unimatrix-engine/src/wire.rs` | Serde-tagged JSON enum |
| `HookResponse` | `enum { Pong{..}, Ack, Error{..} }` | `unimatrix-engine/src/wire.rs` | Serde-tagged JSON enum |
| `Transport` | `trait { connect, request, fire_and_forget, disconnect, is_connected }` | `unimatrix-engine/src/transport.rs` | Sync public API |
| `LocalTransport` | `struct { socket_path, stream, timeout }` | `unimatrix-engine/src/transport.rs` | Implements `Transport` over UDS |
| `TransportError` | `enum { Unavailable, Timeout, Rejected, Codec(String), Io(io::Error) }` | `unimatrix-engine/src/transport.rs` | |
| `PeerCredentials` | `struct { uid: u32, gid: u32, pid: Option<u32> }` | `unimatrix-engine/src/auth.rs` | Platform-abstracted |
| `authenticate_connection` | `fn(stream: &UnixStream) -> Result<PeerCredentials, AuthError>` | `unimatrix-engine/src/auth.rs` | Extracts and verifies peer creds |
| `EventQueue` | `struct { queue_dir: PathBuf }` | `unimatrix-engine/src/event_queue.rs` | |
| `EventQueue::enqueue` | `fn(&self, request: &HookRequest) -> io::Result<()>` | `unimatrix-engine/src/event_queue.rs` | Append to JSONL |
| `EventQueue::replay` | `fn(&self, transport: &mut dyn Transport) -> io::Result<usize>` | `unimatrix-engine/src/event_queue.rs` | Returns count of replayed events |
| `EventQueue::prune` | `fn(&self) -> io::Result<()>` | `unimatrix-engine/src/event_queue.rs` | Delete files older than 7 days |
| `LifecycleHandles` | Extended with `socket_path: Option<PathBuf>` | `unimatrix-server/src/shutdown.rs` | Socket cleanup during shutdown |
| `HookInput` | `struct { hook_event_name, session_id, cwd, ... }` | `unimatrix-engine/src/wire.rs` | Claude Code stdin JSON format |
| `handle_stale_socket` | `fn(socket_path: &Path) -> io::Result<()>` | `unimatrix-server/src/uds_listener.rs` | Called before bind |
| `bootstrap_defaults` | Extended to include `cortical-implant` agent | `unimatrix-server/src/registry.rs` | Internal trust, Read+Search caps |

---

## Technology Decisions

### Wire Protocol: Length-Prefixed JSON (ADR-005)

Length-prefixed JSON over UDS. Not JSON-RPC: the internal protocol does not need the full JSON-RPC envelope (id, jsonrpc version, method name in every message). Type-tagged serde enums provide routing. Not binary (bincode): JSON is debuggable with standard tools, and serialization overhead (~0.2ms) is negligible within the 50ms budget.

### Hook Process Runtime: Blocking std I/O (ADR-002)

The hook process uses `std::os::unix::net::UnixStream` with `SO_RCVTIMEO`/`SO_SNDTIMEO` for timeouts. No tokio runtime initialization. This saves ~1-3ms of startup cost. The hook process performs a single synchronous request-response cycle — async provides no benefit.

### Socket Lifecycle: Unconditional Unlink After PidGuard (ADR-004)

On server startup, after PidGuard establishes mutual exclusion, unconditionally unlink the socket file before binding. This is simpler and safer than connect-to-detect-stale because PidGuard already guarantees no other server is running. On shutdown, remove the socket file before database compaction.

### Engine Extraction: Re-export for Backward Compatibility (ADR-001)

After extracting modules to `unimatrix-engine`, `unimatrix-server` re-exports them so existing integration tests continue to compile. This allows incremental extraction without modifying any test imports.

---

## Open Questions

1. **`search.rs` and `query.rs` placement:** The SCOPE.md mentions these as potential col-006 stubs in the engine. However, col-006 does not implement search or query operations over UDS — only Ping, SessionRegister, and SessionClose. These modules should be deferred to col-007 which first needs them. col-006 should leave extension points in the `HookRequest`/`HookResponse` enums but not implement the handlers. The architect recommends deferral.

2. **Session identity when Claude Code does not provide `session_id`:** SCOPE.md Open Question 2 asks about parent PID as fallback. The `HookInput` struct should use `Option<String>` for `session_id` with fallback to parent PID (`std::os::unix::process::parent_id()`). The hook process computes a deterministic session proxy as `ppid-{parent_pid}` when `session_id` is absent. This is a client-side decision (in `hook.rs`), not a wire protocol concern.

3. **Queue replay timing:** Should queue replay happen on server startup (blocking), during graceful shutdown, or as a background task after UDS listener is ready? The architect recommends: replay as a background tokio task spawned after the UDS listener is accepting connections, so queued events are processed when the server is fully operational. Not during shutdown (time-sensitive). Not at startup (blocks acceptance of new connections).
