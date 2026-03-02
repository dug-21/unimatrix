# Implementation Brief: col-006 Hook Transport Layer ("Cortical Implant")

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/col-006/SCOPE.md |
| Scope Risk Assessment | product/features/col-006/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/col-006/architecture/ARCHITECTURE.md |
| Specification | product/features/col-006/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-006/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-006/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| engine-extraction | pseudocode/engine-extraction.md | test-plan/engine-extraction.md |
| uds-listener | pseudocode/uds-listener.md | test-plan/uds-listener.md |
| hook-subcommand | pseudocode/hook-subcommand.md | test-plan/hook-subcommand.md |
| wire-protocol | pseudocode/wire-protocol.md | test-plan/wire-protocol.md |
| transport | pseudocode/transport.md | test-plan/transport.md |
| authentication | pseudocode/authentication.md | test-plan/authentication.md |
| event-queue | pseudocode/event-queue.md | test-plan/event-queue.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Establish the transport infrastructure that connects Claude Code lifecycle hooks to the running Unimatrix MCP server via Unix domain socket IPC. This is the foundation for all automatic knowledge delivery (col-007 through col-011): every subsequent hook-driven feature depends on this transport layer. col-006 delivers a UDS listener in the server, a `hook` subcommand on the existing binary, a `Transport` trait abstraction, shared business logic extraction into a `unimatrix-engine` crate, layered zero-configuration authentication, graceful degradation with event queuing, and end-to-end validation via SessionStart/Stop smoke tests.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Engine extraction boundary | Move project.rs, confidence.rs, coaccess.rs one at a time in dependency order; server re-exports via `pub use unimatrix_engine::*` for backward compatibility | Architecture | architecture/ADR-001-engine-extraction-boundary.md |
| Hook process runtime | Blocking std I/O only (no tokio runtime); early branch in main.rs before any async initialization | Architecture | architecture/ADR-002-hook-process-sync-runtime.md |
| Authentication model | 3-layer (filesystem permissions + UID verification + process lineage); no shared secret; Layer 3 advisory on Linux only | Architecture | architecture/ADR-003-layered-auth-without-shared-secret.md |
| Socket lifecycle | Unconditional unlink after PidGuard establishes mutual exclusion; SocketGuard RAII for cleanup | Architecture | architecture/ADR-004-socket-lifecycle-unconditional-unlink.md |
| Wire protocol | Length-prefixed JSON (4-byte BE u32 + JSON payload); serde-tagged enums for routing; 1 MiB max payload | Architecture | architecture/ADR-005-wire-protocol-length-prefixed-json.md |
| Hook stdin parsing | Maximum defensive serde: `#[serde(default)]` on all fields, `#[serde(flatten)]` for unknown fields, `Option<T>` everywhere; session_id fallback to parent PID | Architecture | architecture/ADR-006-hook-stdin-defensive-parsing.md |
| Schema version | No schema v4 migration in col-006; no new redb tables; session handlers log-and-ack only | Architecture | architecture/ADR-007-no-schema-v4-in-col-006.md |

## Build Order

The components have strict dependency ordering. Build in this sequence:

### Wave 1: Foundation (no dependencies on other col-006 components)

1. **engine-extraction** -- Extract `project.rs`, then `confidence.rs`, then `coaccess.rs` from `unimatrix-server` into new `crates/unimatrix-engine/`. Run the full 1199-test suite after each module move. This is the highest-risk change (SR-01, R-01) and must be validated before any other work proceeds.

2. **wire-protocol** -- Define `HookRequest`, `HookResponse`, `HookInput`, `ImplantEvent`, `TransportError` types in `unimatrix-engine/src/wire.rs`. These types have no runtime dependencies and can be built in parallel with extraction (though they land in the same crate).

### Wave 2: Transport (depends on wire-protocol)

3. **transport** -- Implement `Transport` trait and `LocalTransport` in `unimatrix-engine/src/transport.rs`. Depends on wire types for `HookRequest`/`HookResponse`. Uses `std::os::unix::net::UnixStream`.

4. **authentication** -- Implement `PeerCredentials`, `authenticate_connection()` in `unimatrix-engine/src/auth.rs`. Platform-specific with `#[cfg(target_os)]` blocks.

5. **event-queue** -- Implement `EventQueue` in `unimatrix-engine/src/event_queue.rs`. JSONL file management with rotation, pruning, and replay.

### Wave 3: Server Integration (depends on Waves 1-2)

6. **uds-listener** -- Add `uds_listener.rs` to `unimatrix-server`. Spawns tokio task for UDS accept loop. Uses auth module for connection verification. Dispatches via wire protocol types. Integrates into server startup/shutdown sequence.

7. **hook-subcommand** -- Add `hook.rs` to `unimatrix-server` and extend `main.rs` with clap subcommand. Uses `LocalTransport` for UDS communication. Reads stdin, parses `HookInput`, dispatches to server.

### Wave 4: Integration and Validation

8. **SocketGuard + LifecycleHandles extension** -- Wire up socket lifecycle into the existing server lifecycle (PidGuard coordination, shutdown ordering, stale socket cleanup).

9. **cortical-implant bootstrap** -- Add `cortical-implant` agent to `bootstrap_defaults()` in registry.rs.

10. **Smoke tests** -- End-to-end SessionStart/Stop round-trip tests. Ping/Pong latency benchmark.

## Risk Hotspots (Top 5)

| Priority | Risk | What to Watch | Mitigation |
|----------|------|---------------|------------|
| 1 | R-01/SR-01: Engine extraction breaks MCP tools | Every existing integration test must pass after each module move. Watch for stale local copies of extracted modules. | Incremental extraction, re-export for backward compatibility, full 1199-test suite gate |
| 2 | R-03/SR-02: Socket lifecycle ordering violated | PidGuard must be acquired BEFORE socket bind. Socket must be removed BEFORE PidGuard release. Never reorder startup/shutdown sequence. | Unconditional unlink after PidGuard (ADR-004), SocketGuard RAII |
| 3 | R-07: Wire protocol framing error | Partial reads, truncated payloads, broken pipes must not crash the server handler or leak resources. | Timeout on reads, EOF detection, per-connection error isolation via separate tokio tasks |
| 4 | R-14: UDS connection failure leaks resources | Rapid connect-disconnect cycles (100+ connections) must not cause fd exhaustion or unbounded tokio task growth. | Connection-per-request model, proper error handling in accept loop, fd count validation |
| 5 | R-19/SR-01: UDS listener task crashes server | A panic in any UDS handler must not take down the stdio MCP transport. | `tokio::spawn` per connection (panic isolation), catch_unwind in accept loop |

## Files to Create/Modify

### New Crate

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-engine/Cargo.toml` | Create | New crate: deps on unimatrix-core, unimatrix-store, serde, serde_json, sha2, dirs, tracing |
| `crates/unimatrix-engine/src/lib.rs` | Create | Module declarations, `#![forbid(unsafe_code)]` |
| `crates/unimatrix-engine/src/confidence.rs` | Create | Moved from unimatrix-server (compute_confidence, rerank_score, co_access_affinity) |
| `crates/unimatrix-engine/src/coaccess.rs` | Create | Moved from unimatrix-server (generate_pairs, compute_search_boost, compute_briefing_boost) |
| `crates/unimatrix-engine/src/project.rs` | Create | Moved from unimatrix-server (ProjectPaths extended with socket_path, compute_project_hash) |
| `crates/unimatrix-engine/src/wire.rs` | Create | HookRequest, HookResponse, HookInput, ImplantEvent, TransportError types |
| `crates/unimatrix-engine/src/transport.rs` | Create | Transport trait (5 methods) and LocalTransport (UDS implementation) |
| `crates/unimatrix-engine/src/auth.rs` | Create | PeerCredentials, authenticate_connection(), platform-specific peer cred extraction |
| `crates/unimatrix-engine/src/event_queue.rs` | Create | EventQueue struct with enqueue, replay, prune, rotation logic |

### Modified in unimatrix-server

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/Cargo.toml` | Modify | Add unimatrix-engine dependency |
| `crates/unimatrix-server/src/lib.rs` | Modify | Remove local confidence/coaccess/project modules; add `pub use unimatrix_engine::{confidence, coaccess, project}` re-exports |
| `crates/unimatrix-server/src/main.rs` | Modify | Add clap Hook subcommand; early branch before tokio init; add UDS listener startup/shutdown |
| `crates/unimatrix-server/src/hook.rs` | Create | Hook subcommand handler: stdin parse, LocalTransport connect, request dispatch |
| `crates/unimatrix-server/src/uds_listener.rs` | Create | UDS accept loop (tokio), connection authentication, request dispatch, response framing |
| `crates/unimatrix-server/src/shutdown.rs` | Modify | Add SocketGuard to LifecycleHandles; socket cleanup before compaction |
| `crates/unimatrix-server/src/registry.rs` | Modify | Add cortical-implant to bootstrap_defaults() |

### Workspace

| File | Action | Summary |
|------|--------|---------|
| `Cargo.toml` (workspace root) | Modify | Add unimatrix-engine to workspace members |

## Data Structures

### HookInput (Claude Code stdin JSON)

```rust
#[derive(Deserialize)]
pub struct HookInput {
    #[serde(default)]
    pub hook_event_name: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub transcript_path: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}
```

### HookRequest (IPC wire protocol)

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HookRequest {
    Ping,
    SessionRegister { session_id: String, cwd: String, agent_role: Option<String>, feature: Option<String> },
    SessionClose { session_id: String, outcome: Option<String>, duration_secs: u64 },
    RecordEvent(ImplantEvent),
    RecordEvents(Vec<ImplantEvent>),
    // Stubs for future features (col-007+)
    #[allow(dead_code)] ContextSearch { query: String, role: Option<String>, task: Option<String>, feature: Option<String>, k: Option<u32>, max_tokens: Option<u32> },
    #[allow(dead_code)] Briefing { role: String, task: String, feature: Option<String>, max_tokens: Option<u32> },
    #[allow(dead_code)] CompactPayload { session_id: String, injected_entry_ids: Vec<u64>, role: Option<String>, feature: Option<String>, token_limit: Option<u32> },
}
```

### HookResponse (IPC wire protocol)

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HookResponse {
    Pong { server_version: String },
    Ack,
    Error { code: i32, message: String },
    #[allow(dead_code)] Entries { items: Vec<EntryPayload>, total_tokens: u32 },
    #[allow(dead_code)] BriefingContent { content: String, token_count: u32 },
}
```

### ImplantEvent

```rust
#[derive(Serialize, Deserialize)]
pub struct ImplantEvent {
    pub event_type: String,
    pub session_id: String,
    pub timestamp: u64,
    pub payload: serde_json::Value,
}
```

### TransportError

```rust
pub enum TransportError {
    Unavailable(String),
    Timeout(Duration),
    Rejected { code: i32, message: String },
    Codec(String),
    Transport(String),
}
```

### PeerCredentials

```rust
pub struct PeerCredentials {
    pub uid: u32,
    pub gid: u32,
    pub pid: Option<u32>,  // Linux only via SO_PEERCRED; None on macOS
}
```

### SocketGuard

```rust
pub struct SocketGuard {
    path: PathBuf,
}
// Drop impl removes socket file (ignores NotFound)
```

### ProjectPaths (extended)

```rust
pub struct ProjectPaths {
    pub project_root: PathBuf,
    pub project_hash: String,
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub vector_dir: PathBuf,
    pub pid_path: PathBuf,
    pub socket_path: PathBuf,  // NEW: ~/.unimatrix/{hash}/unimatrix.sock
}
```

## Function Signatures

### Transport Trait

```rust
pub trait Transport: Send + Sync {
    fn connect(&mut self) -> Result<(), TransportError>;
    fn request(&mut self, req: &HookRequest, timeout: Duration) -> Result<HookResponse, TransportError>;
    fn fire_and_forget(&mut self, req: &HookRequest) -> Result<(), TransportError>;
    fn disconnect(&mut self);
    fn is_connected(&self) -> bool;
}
```

### LocalTransport

```rust
impl LocalTransport {
    pub fn new(socket_path: PathBuf, timeout: Duration) -> Self;
}
```

### Authentication

```rust
pub fn extract_peer_credentials(stream: &UnixStream) -> Result<PeerCredentials, AuthError>;
pub fn authenticate_connection(stream: &UnixStream, server_uid: u32) -> Result<PeerCredentials, AuthError>;
```

### Event Queue

```rust
impl EventQueue {
    pub fn new(queue_dir: PathBuf) -> Self;
    pub fn enqueue(&self, request: &HookRequest) -> io::Result<()>;
    pub fn replay(&self, transport: &mut dyn Transport) -> io::Result<usize>;
    pub fn prune(&self) -> io::Result<()>;
}
```

### Wire Protocol Framing

```rust
pub fn write_frame(writer: &mut impl Write, payload: &[u8]) -> io::Result<()>;
pub fn read_frame(reader: &mut impl Read, max_size: usize) -> Result<Vec<u8>, TransportError>;
```

### Hook Subcommand

```rust
pub fn run(event: String) -> Result<(), Box<dyn std::error::Error>>;
```

### UDS Listener

```rust
pub async fn start_uds_listener(
    socket_path: &Path,
    store: Arc<Store>,
    // ... shared resources
) -> io::Result<(JoinHandle<()>, SocketGuard)>;

pub fn handle_stale_socket(socket_path: &Path) -> io::Result<()>;
```

## Constraints

### Hard Constraints
- **redb exclusive file lock**: Hook processes cannot open the database; all data access through IPC to the running server
- **50ms latency budget**: End-to-end hook execution (process start to exit) under 50ms for synchronous hooks
- **Zero regression**: All 1025 unit + 174 integration tests pass without modification after engine extraction
- **Single binary**: Hook subcommand is part of `unimatrix-server`, not a separate binary
- **`#![forbid(unsafe_code)]`**: On `unimatrix-engine` crate

### Soft Constraints
- Linux + macOS only (UDS transport); no Windows support
- Manual hook configuration (`.claude/settings.json`); automated setup is alc-003 scope
- No new redb tables; schema version stays at v3
- No schema migration; no EntryRecord field additions
- Edition 2024, MSRV 1.89

## Dependencies

### Existing (no new external crates needed)

| Crate | Purpose |
|-------|---------|
| `serde` + `serde_json` | Wire protocol serialization |
| `clap` | Hook subcommand argument parsing |
| `sha2` | Project hash computation (moves to engine) |
| `dirs` | Home directory resolution (moves to engine) |
| `fs2` | Advisory file locking (SocketGuard pattern) |
| `tokio` | UDS listener in server (server-side only) |
| `tracing` | Diagnostic logging |
| `tempfile` | Test infrastructure (already in workspace) |

### Feature Dependencies

| Feature | Relationship |
|---------|-------------|
| vnc-001 through vnc-004 | col-006 extends the existing server |
| nxs-001 through nxs-004 | Foundation crates |
| crt-001 through crt-005 | Confidence/co-access logic extracted to engine |
| alc-002 | Agent enrollment; cortical-implant added to bootstrap |

### Downstream (depend on col-006)

| Feature | What It Needs from col-006 |
|---------|---------------------------|
| col-007 | UDS transport for UserPromptSubmit injection |
| col-008 | UDS transport for PreCompact knowledge preservation |
| col-009 | UDS transport for PostToolUse confidence feedback |
| col-010 | Wire protocol types, event queue format; adds SESSIONS table and schema v4 |
| col-011 | UDS transport for agent routing |

## NOT in Scope

- Context injection logic (col-007)
- Compaction resilience (col-008)
- Confidence feedback loops (col-009)
- Session lifecycle persistence, SESSIONS table, schema v4 (col-010)
- Agent routing (col-011)
- Telemetry tables (SESSIONS, INJECTION_LOG, SIGNAL_QUEUE)
- Queue replay implementation (queue provides durable storage; replay is col-010 scope)
- Daemon architecture (future optimization; col-006 uses ephemeral hook processes)
- Remote/HTTPS transport (future centralized deployment)
- Windows named pipe transport
- `unimatrix init` auto-configuration (alc-003 scope)
- Replacing existing col-002 observation hooks (coexist)
- `search.rs` and `query.rs` full implementation (stubs only; col-007 scope)
- Connection pooling or persistent connections (one-connection-per-request for simplicity)
- Shared secret fallback authentication

## Test Strategy Summary

From RISK-TEST-STRATEGY.md: 23 risks mapped to 79 test scenarios across 13 test categories.

| Category | Estimated Tests | Key Risks Covered |
|----------|----------------|-------------------|
| Unit: wire protocol | 12-15 | R-07 (framing), R-08 (oversized payload), R-09 (malformed JSON) |
| Unit: transport trait | 8-10 | R-06 (latency), R-18 (no tokio init) |
| Unit: authentication | 8-10 | R-10 (UID bypass), R-11 (lineage false negative) |
| Unit: event queue | 10-12 | R-15 (corruption), R-16 (size limits), R-17 (pruning) |
| Unit: hook input parsing | 7-10 | R-12 (format changes) |
| Unit: SocketGuard | 3-5 | R-05 (drop failure) |
| Unit: cortical-implant bootstrap | 2-3 | R-20 (idempotency) |
| Unit: ProjectPaths extension | 2-3 | R-21 (socket_path field) |
| Integration: UDS listener | 8-12 | R-03 (lifecycle ordering), R-14 (fd leak), R-19 (crash isolation) |
| Integration: hook subcommand | 6-8 | R-06 (latency), R-12 (stdin parsing) |
| Integration: lifecycle | 4-6 | R-04 (stale socket), R-03 (ordering) |
| Integration: engine extraction | 0 new (existing 1199) | R-01 (extraction breaks tools), R-02 (re-export divergence) |
| Benchmark: latency | 2-3 | R-06 (50ms budget) |
| **Total new** | **~70-95** | |

### Test Infrastructure Needs

| Helper | Location | Purpose |
|--------|----------|---------|
| `TestUdsServer` | unimatrix-engine or unimatrix-server tests | Spawns UDS listener in tempdir for integration tests |
| `TestHookProcess` | unimatrix-server tests | Spawns hook subcommand as child process with controlled stdin |
| `RawUdsClient` | unimatrix-engine tests | Low-level UDS client for malformed input and framing error tests |
| `EventQueueFixture` | unimatrix-engine tests | Controlled event queue directory with timestamps for rotation/pruning tests |

## Acceptance Criteria Checklist

- [ ] AC-01: UDS listener starts on `~/.unimatrix/{project_hash}/unimatrix.sock` with permissions 0o600
- [ ] AC-02: UDS handles concurrent connections without blocking stdio MCP transport
- [ ] AC-03: `unimatrix-server hook <EVENT>` reads stdin JSON, connects to UDS, dispatches event
- [ ] AC-04: Ping/Pong round-trip under 50ms end-to-end (p95 over 10 iterations)
- [ ] AC-05: Transport trait defined with 5 methods; LocalTransport implements over UDS
- [ ] AC-06: unimatrix-engine crate exists with confidence, coaccess, project modules; all 174+ integration tests pass
- [ ] AC-07: UDS authenticates via UID verification; rejects different-user connections
- [ ] AC-08: Hook exits 0 when server unavailable; queues fire-and-forget events
- [ ] AC-09: Stale socket files cleaned up on server startup
- [ ] AC-10: Socket cleanup on graceful shutdown; socket file does not persist
- [ ] AC-11: cortical-implant agent pre-enrolled as Internal trust in bootstrap_defaults()
- [ ] AC-12: Event queue respects size limits (1000 events/file, 10 files max, 7-day pruning)
- [ ] AC-13: SessionStart and Stop hooks round-trip through full chain

## Alignment Status

From ALIGNMENT-REPORT.md: **5 PASS, 2 WARN. Zero VARIANCE. Zero FAIL.**

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Establishes the "Hooks" leg of the three-leg boundary |
| Milestone Fit | PASS | Correctly scoped to M5 Collective phase |
| Scope Gaps | WARN | Minor: search.rs/query.rs stubs deferred (architecture recommends deferral, spec includes them); queue replay deferred to col-010 |
| Scope Additions | PASS | No scope additions detected |
| Architecture Consistency | PASS | Consistent with existing PidGuard, spawn_blocking, crate graph patterns |
| Risk Completeness | WARN | RISK-TEST-STRATEGY.md authored after alignment review; now complete with 23 risks |

The two WARN items are administrative, not substantive. No variances require human approval.

## Hook Configuration Reference

For `.claude/settings.json` (manual setup, automated in alc-003):

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

## Startup/Shutdown Ordering

### Startup (extended)

1. Parse CLI args (clap) -- branch to hook path or server path
2. Initialize tracing
3. Detect project root, compute hash, ensure data directory
4. Handle stale PID file
5. Open database (with retry)
6. Acquire PidGuard
7. Remove stale socket file (unconditional unlink)
8. Bind UDS listener, spawn accept loop
9. Initialize vector index, embed handle, registry, audit
10. Bootstrap defaults (including cortical-implant agent)
11. Build UnimatrixServer
12. Serve MCP over stdio

### Shutdown (extended)

1. MCP session close or signal received
2. Stop UDS accept loop
3. Drain in-flight UDS requests (1s timeout)
4. SocketGuard drops (remove socket file)
5. Dump vector index
6. Save adaptation state
7. Drop Arc holders
8. Compact database
9. PidGuard drops (remove PID file)

### Error Codes (UDS protocol)

| Code | Meaning |
|------|---------|
| -32001 | UID mismatch (authentication failure) |
| -32002 | Process lineage verification failed |
| -32003 | Unknown request type |
| -32004 | Invalid request payload |
| -32005 | Internal server error |
