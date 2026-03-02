# Risk-Based Test Strategy: col-006

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Engine extraction breaks existing MCP tool behavior (confidence scoring, co-access boost, or project hash changes silently) | Critical | Medium | Critical |
| R-02 | Re-export path divergence: server retains a stale local copy of an extracted module alongside the re-export, causing ambiguous resolution | Critical | Low | High |
| R-03 | Socket lifecycle ordering violated: socket bound before PidGuard acquired, or socket unlinked while another server's socket is active | High | Medium | High |
| R-04 | Stale socket file blocks server restart after crash (SIGKILL leaves socket, next bind() fails with EADDRINUSE) | High | Medium | High |
| R-05 | SocketGuard drop fails to remove socket file (permission change, readonly fs, or path invalidated) | Medium | Low | Medium |
| R-06 | Hook process exceeds 50ms latency budget for Ping/Pong round-trip (tokio init leak, slow project hash, filesystem latency) | High | Low | Medium |
| R-07 | Wire protocol framing error: partial read of 4-byte length prefix or payload truncation on UDS (broken pipe, client kill mid-write) | High | Medium | High |
| R-08 | Length prefix deserialization produces payload size > 1 MiB, causing unbounded memory allocation | High | Low | Medium |
| R-09 | Malformed JSON in wire protocol payload causes server panic or unhandled error (non-UTF8, truncated JSON, wrong enum tag) | Medium | Medium | Medium |
| R-10 | UID verification fails to reject connections from different users (platform-specific SO_PEERCRED / getpeereid inconsistency) | High | Low | Medium |
| R-11 | Process lineage check (/proc/cmdline) false negative: binary installed via symlink, wrapper script, or cargo alias does not contain "unimatrix-server" | Medium | Medium | Medium |
| R-12 | Hook stdin JSON parsing failure on Claude Code format change (renamed field, new required field, type change) | Medium | Low | Low |
| R-13 | Concurrent UDS connections during swarm runs (5-10 simultaneous hooks) cause request queuing that pushes individual response times beyond 50ms | Medium | Medium | Medium |
| R-14 | UDS connection failure (broken pipe, client disconnect mid-request) leaks file descriptors or tokio tasks in the server | High | Medium | High |
| R-15 | Event queue file corruption from hook process crash mid-write produces unrecoverable JSONL file | Low | Low | Low |
| R-16 | Event queue size limits not enforced: >1000 events per file or >10 files accumulate, consuming disk space | Medium | Low | Low |
| R-17 | Event queue pruning deletes files younger than 7 days due to clock skew or timestamp format error | Medium | Low | Low |
| R-18 | Hook subcommand accidentally initializes tokio runtime, ONNX, or redb on the hook code path, violating the startup budget | High | Low | Medium |
| R-19 | UDS listener task panic or error crashes the entire server process (not isolated from stdio MCP transport) | Critical | Low | High |
| R-20 | cortical-implant agent bootstrap is not idempotent: repeated bootstrap_defaults() calls modify the agent record | Low | Low | Low |
| R-21 | ProjectPaths extension (adding socket_path) changes serialization or equality semantics, breaking downstream consumers | Medium | Low | Low |
| R-22 | Fire-and-forget requests (SessionRegister, RecordEvent) silently dropped by server under contention with no client-visible indication | Medium | Medium | Medium |
| R-23 | Graceful shutdown drain timeout (1s) too short: in-flight UDS handler holds redb write lock past timeout, blocking compaction | Medium | Low | Low |

## Risk-to-Scenario Mapping

### R-01: Engine Extraction Breaks MCP Tool Behavior
**Severity**: Critical
**Likelihood**: Medium
**Impact**: All 10 MCP tools could produce incorrect confidence scores, wrong search re-ranking, or mismatched project hashes. Silent data quality regression affecting every agent interaction.

**Test Scenarios**:
1. After each incremental module extraction (project -> confidence -> coaccess), run the full 1199-test suite. All tests pass without modification.
2. Verify `compute_confidence` produces identical output when called from `unimatrix_engine::confidence` vs the pre-extraction baseline (same EntryRecord, same timestamp, same score).
3. Verify `rerank_score` with known inputs produces identical ranking order pre- and post-extraction.
4. Verify `compute_search_boost` and `compute_briefing_boost` return identical boost maps for a seeded database pre- and post-extraction.
5. Verify `compute_project_hash` returns identical hashes for the same canonical path from both the engine and the server re-export.

**Coverage Requirement**: The existing 174 integration tests are the primary regression gate. No new tests needed if all pass without modification. If any test requires modification, that test change must be reviewed as a potential behavior change.

### R-02: Re-Export Path Divergence
**Severity**: Critical
**Likelihood**: Low
**Impact**: A stale local module file left behind during extraction causes the server to use outdated logic while the engine has the current version. Compilation would succeed because both paths exist, but runtime behavior diverges.

**Test Scenarios**:
1. After extraction, verify no `confidence.rs`, `coaccess.rs`, or `project.rs` source files remain in `crates/unimatrix-server/src/` (only re-exports in `lib.rs`).
2. Verify `unimatrix_server::confidence::compute_confidence` and `unimatrix_engine::confidence::compute_confidence` resolve to the same function (call both, assert identical results).
3. Verify the re-export in `lib.rs` uses `pub use unimatrix_engine::confidence;` (module-level re-export, not individual function re-exports that could miss new additions).

**Coverage Requirement**: Build verification (Rust compiler catches most cases). One integration test that imports from both paths and asserts identity.

### R-03: Socket Lifecycle Ordering Violated
**Severity**: High
**Likelihood**: Medium
**Impact**: Socket bound before PidGuard means two servers could both bind and create conflicting sockets. Socket unlinked before PidGuard means a legitimate server's socket is destroyed.

**Test Scenarios**:
1. Startup sequence test: PidGuard is acquired before socket bind. If PidGuard fails, no socket file is created.
2. Shutdown sequence test: socket file is removed before PidGuard is released (socket cleanup precedes compaction).
3. Concurrent startup test: two server instances targeting the same project hash. Second instance fails at PidGuard, never reaches socket bind.

**Coverage Requirement**: Integration test that verifies startup ordering invariants. Integration test with two processes targeting the same data directory.

### R-04: Stale Socket Blocks Server Restart
**Severity**: High
**Likelihood**: Medium
**Impact**: Server fails to start UDS listener, falls back to stdio-only. All hook processes fail to connect, triggering graceful degradation (silent skip). Users see no MCP error but hooks do not work.

**Test Scenarios**:
1. Create a stale socket file at the bind path (simulate crash). Start server. Verify stale socket is removed and new socket is bound successfully.
2. Create a stale socket file AND a stale PID file. Start server. Verify both are cleaned up and server starts normally.
3. Verify `handle_stale_socket` after PidGuard acquisition always removes the socket (unconditional unlink per ADR-004).

**Coverage Requirement**: Integration test that creates filesystem artifacts simulating a crashed server, then verifies clean startup.

### R-05: SocketGuard Drop Fails to Remove Socket
**Severity**: Medium
**Likelihood**: Low
**Impact**: Socket file persists after shutdown. Next startup handles it via unconditional unlink (R-04 mitigation), so impact is limited to a single startup cycle.

**Test Scenarios**:
1. SocketGuard drop when socket file exists: file is removed.
2. SocketGuard drop when socket file was already removed: no panic (NotFound handled).
3. SocketGuard drop when path is invalid: warning logged, no panic.

**Coverage Requirement**: Unit tests for SocketGuard analogous to PidGuard drop tests in `pidfile.rs`.

### R-06: Hook Process Exceeds 50ms Latency
**Severity**: High
**Likelihood**: Low
**Impact**: Entire hook architecture invalidated. Future features (col-007 search at 12-36ms) have no latency margin.

**Test Scenarios**:
1. Benchmark: spawn `unimatrix-server hook Ping` with a running server. Measure wall-clock time from process fork to exit. Assert < 50ms.
2. Run 10 iterations. Assert p95 < 50ms.
3. Verify the hook code path does not import or initialize tokio, ONNX, redb, or HNSW (R-18 overlap).

**Coverage Requirement**: Benchmark test (not a standard unit test). Can be `#[ignore]` for CI but must be runnable manually and in performance validation.

### R-07: Wire Protocol Framing Error
**Severity**: High
**Likelihood**: Medium
**Impact**: Server reads wrong number of bytes, interprets data as wrong message, or hangs waiting for bytes that never arrive. Could crash the handler task or corrupt the response.

**Test Scenarios**:
1. Send a valid 4-byte length prefix followed by a valid JSON payload. Verify correct parsing.
2. Send a 4-byte length prefix claiming N bytes, but close connection after sending fewer than N bytes (partial payload). Server handler must timeout or detect EOF, not hang.
3. Send a 4-byte length prefix of 0 (empty payload). Server rejects with Codec error.
4. Send only 2 bytes then close connection (partial length prefix). Server detects incomplete read.
5. Send a valid request but with trailing garbage bytes after the JSON payload. Server ignores trailing bytes (connection closes after one request).
6. Send two requests on the same connection (pipelining). Server processes only the first (single-request-per-connection model).
7. Send a payload with valid length prefix but invalid UTF-8 bytes. Server returns Codec error.

**Coverage Requirement**: Unit tests for the framing read/write functions. Integration tests for error paths using raw UDS connections (bypassing `LocalTransport`).

### R-08: Oversized Payload Memory Exhaustion
**Severity**: High
**Likelihood**: Low
**Impact**: A crafted or accidental message with length prefix > 1 MiB causes the server to allocate unbounded memory, potentially crashing the process.

**Test Scenarios**:
1. Send a length prefix indicating 1,048,577 bytes (1 MiB + 1). Verify server rejects before allocating.
2. Send a length prefix indicating u32::MAX bytes. Verify rejection.
3. Send a length prefix of exactly 1,048,576 bytes with valid payload. Verify acceptance (boundary).

**Coverage Requirement**: Unit test for the frame reader's size validation.

### R-09: Malformed JSON Payload
**Severity**: Medium
**Likelihood**: Medium
**Impact**: If serde deserialization panics or produces an unhandled error, the handler task crashes. If the error is caught, the server sends an Error response and closes the connection cleanly.

**Test Scenarios**:
1. Valid length prefix, but payload is `{}` (empty JSON object). Deserialize fails on missing `type` tag. Server returns Error response.
2. Valid payload but unknown `type` tag (e.g., `{"type":"FutureVariant"}`). Server returns Error with code -32003.
3. Payload with correct `type` but missing required fields (e.g., `{"type":"SessionRegister"}` without `session_id`). Serde default handles missing fields.
4. Payload is valid JSON but not an object (e.g., `[1,2,3]`). Server returns Codec error.
5. Payload is not valid JSON (e.g., `{broken`). Server returns Codec error.

**Coverage Requirement**: Unit tests for deserialization of each `HookRequest` variant including malformed cases.

### R-10: UID Verification Bypass
**Severity**: High
**Likelihood**: Low
**Impact**: Unauthorized process connects to UDS and sends arbitrary requests. In the local-dev threat model, the blast radius is limited (read knowledge entries, register fake sessions), but the authentication layer must work correctly.

**Test Scenarios**:
1. Connect from same user: verify authentication succeeds (UID matches server UID).
2. Construct a `PeerCredentials` with a different UID: verify `authenticate_connection` rejects.
3. On Linux: verify SO_PEERCRED extraction returns correct UID, GID, and PID for a real UDS connection.
4. On macOS: verify getpeereid extraction returns correct UID and GID, with PID as None.
5. Verify auth failure closes connection immediately with no response sent.
6. Verify auth failure is logged to stderr with peer UID (and PID if available).

**Coverage Requirement**: Unit tests for `authenticate_connection` with mocked credentials. Integration test with a real UDS connection verifying same-user succeeds.

### R-11: Process Lineage False Negative
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Layer 3 auth check fails for legitimate hook processes. Per ADR-003, Layer 3 failure is advisory (warning, not rejection), so the connection proceeds. The risk is that the warning noise reduces the signal value of the check.

**Test Scenarios**:
1. cmdline containing `/usr/bin/unimatrix-server hook SessionStart`: lineage passes.
2. cmdline containing `target/release/unimatrix-server hook Ping`: lineage passes (handles full path).
3. cmdline containing `some-other-binary`: lineage fails (warning logged, connection NOT rejected).
4. cmdline empty (kernel thread or zombie): lineage fails gracefully.
5. /proc/{pid}/cmdline does not exist (process exited between accept and check): lineage fails gracefully.
6. cmdline containing `not-unimatrix-server`: lineage correctly rejects (not a substring match of the filename).

**Coverage Requirement**: Unit tests reusing the `is_unimatrix_process` pattern from `pidfile.rs`. Linux-only tests gated behind `#[cfg(target_os = "linux")]`.

### R-12: Hook stdin JSON Parsing Failure
**Severity**: Medium
**Likelihood**: Low
**Impact**: Hook exits 0 silently (graceful degradation). Session events lost for that invocation but no user-visible error.

**Test Scenarios**:
1. Minimal valid JSON: `{"hook_event_name":"SessionStart"}`. Parses successfully; `session_id` defaults to None; `cwd` falls back to process cwd.
2. Full JSON with all known fields: parses all fields correctly.
3. JSON with unknown fields: `{"hook_event_name":"Ping","new_field":42}`. Unknown field captured in `extra`, no parse error.
4. Empty stdin: parse fails, hook exits 0, logs to stderr.
5. Non-JSON stdin (binary data): parse fails, hook exits 0.
6. JSON with `session_id` as integer (type change): `Option<String>` defaults to None via serde, hook falls back to parent PID proxy.
7. JSON missing `hook_event_name`: defaults to empty string, dispatcher treats as unknown event, exits 0.

**Coverage Requirement**: Unit tests for `HookInput` deserialization covering all defensive parsing cases per ADR-006.

### R-13: Concurrent UDS Connection Contention
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Individual hook response times exceed budget during swarm runs. col-006 smoke tests are fire-and-forget (low contention), but the risk informs col-007+ design.

**Test Scenarios**:
1. 5 concurrent Ping requests on separate UDS connections. All receive Pong. None exceeds 50ms.
2. 10 concurrent fire-and-forget SessionRegister requests. All acknowledged. Server remains responsive on stdio.
3. Concurrent UDS and stdio MCP requests: a MCP tool call executes while UDS requests are in flight. MCP latency is not measurably affected.

**Coverage Requirement**: Integration test with concurrent connections (tokio JoinSet or thread pool). Latency measurement is advisory, not a hard test gate for col-006 (becomes a gate in col-007).

### R-14: UDS Connection Failure Leaks Resources
**Severity**: High
**Likelihood**: Medium
**Impact**: File descriptor exhaustion or unbounded tokio task growth eventually crashes the server process.

**Test Scenarios**:
1. Client connects then immediately closes (no data sent). Server handler task completes without error. No fd leak.
2. Client sends length prefix then closes (partial request). Server handler detects EOF, cleans up.
3. Client sends full request but disconnects before reading response. Server handler completes write (broken pipe error), cleans up.
4. Rapid connect-disconnect cycle (100 connections in 1 second). Server remains operational with stable fd count.
5. Verify tokio task count returns to baseline after all connections close.

**Coverage Requirement**: Integration tests using raw UDS connections. Fd leak detection via `/proc/self/fd` count (Linux) or lsof.

### R-15: Event Queue File Corruption
**Severity**: Low
**Likelihood**: Low
**Impact**: At most one event lost per crash. Queue file remains usable for all other lines.

**Test Scenarios**:
1. Write 10 events, then append a partial line (simulating crash). Replay skips the partial line, processes the other 10.
2. Write to queue, verify file is flushed (fsync or flush after each line).

**Coverage Requirement**: Unit test for queue replay with malformed lines.

### R-16: Event Queue Size Limits Not Enforced
**Severity**: Medium
**Likelihood**: Low
**Impact**: Unbounded disk consumption in `~/.unimatrix/{hash}/event-queue/`.

**Test Scenarios**:
1. Write 1001 events. Verify file rotation: first file has 1000 events, second file has 1.
2. Write events across 11 files. Verify the oldest file is deleted when the 11th is created.
3. Verify max capacity is bounded at ~10,000 events (10 files x 1000 events).

**Coverage Requirement**: Unit tests for `EventQueue` size management.

### R-17: Event Queue Pruning Error
**Severity**: Medium
**Likelihood**: Low
**Impact**: Events older than 7 days not pruned (disk accumulation) or events younger than 7 days deleted (data loss).

**Test Scenarios**:
1. Create queue files with timestamps 8 days old. Verify they are pruned on next hook invocation.
2. Create queue files with timestamps 6 days old. Verify they are NOT pruned.
3. Boundary: file with timestamp exactly 7 days old. Document whether it is pruned or kept.

**Coverage Requirement**: Unit test with controlled timestamps (mocked or injected clock).

### R-18: Hook Code Path Initializes Heavy Components
**Severity**: High
**Likelihood**: Low
**Impact**: 1-3ms tokio overhead + potential ONNX/redb init pushes hook well beyond 50ms. Defeats the purpose of ADR-002.

**Test Scenarios**:
1. Verify `main.rs` branches on `Command::Hook` before any tokio runtime initialization.
2. Verify `hook::run()` does not import or call any tokio, ONNX, redb, or HNSW symbols.
3. Static analysis: grep the hook module's dependency graph for banned imports (`tokio::`, `ort::`, `redb::`, `hnsw_rs::`).

**Coverage Requirement**: Code review + static analysis (grep-based CI check). Not a runtime test.

### R-19: UDS Listener Task Crashes Server
**Severity**: Critical
**Likelihood**: Low
**Impact**: A panic in the UDS accept loop or handler dispatch kills the entire tokio runtime, taking down the stdio MCP transport and all active tool calls.

**Test Scenarios**:
1. Send a request that triggers an error in the UDS handler. Verify the handler task terminates but the accept loop and stdio transport continue.
2. Send a request with an unknown variant. Verify error response, not panic.
3. If the accept loop itself encounters an error (e.g., too many open files), verify it logs and continues (or recovers), not panics.

**Coverage Requirement**: Integration test that exercises error paths in the UDS handler while verifying MCP tool calls still work on stdio.

### R-20: cortical-implant Bootstrap Idempotency
**Severity**: Low
**Likelihood**: Low
**Impact**: Repeated calls to `bootstrap_defaults()` could reset the agent's capabilities or enrollment timestamp.

**Test Scenarios**:
1. Call `bootstrap_defaults()`. Resolve `cortical-implant`. Record enrolled_at.
2. Call `bootstrap_defaults()` again. Resolve `cortical-implant`. Assert enrolled_at unchanged.
3. Verify trust level is `Internal` and capabilities are `[Read, Search]`.

**Coverage Requirement**: Unit test in `registry.rs`, following the pattern of `test_bootstrap_idempotent`.

### R-21: ProjectPaths Extension Breaks Downstream
**Severity**: Medium
**Likelihood**: Low
**Impact**: If code pattern-matches on `ProjectPaths` fields exhaustively, adding `socket_path` causes a compile error (which is detectable, not silent). If `ProjectPaths` is serialized/deserialized, new field could break compatibility.

**Test Scenarios**:
1. Verify `ensure_data_directory` returns a `ProjectPaths` with `socket_path` set to `data_dir.join("unimatrix.sock")`.
2. Verify `socket_path` is deterministic (same project root produces same socket path).

**Coverage Requirement**: Unit test extending existing `test_ensure_creates_dirs`.

### R-22: Fire-and-Forget Requests Silently Dropped
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Session events lost during high-contention periods. No client indication. col-006 logs server-side, but if the handler fails silently (e.g., logging panics), events vanish.

**Test Scenarios**:
1. Send fire-and-forget SessionRegister. Verify server log contains the session registration.
2. Send fire-and-forget under load (10 concurrent). Verify all 10 are logged.

**Coverage Requirement**: Integration test with log capture (tracing subscriber).

### R-23: Shutdown Drain Timeout
**Severity**: Medium
**Likelihood**: Low
**Impact**: In-flight UDS handler holds redb write lock past the 1s drain timeout. Shutdown proceeds to compaction, which requires exclusive store access. Compaction skipped ("outstanding Store references").

**Test Scenarios**:
1. Start a slow UDS handler (simulate with sleep). Initiate shutdown. Verify handler is cancelled after 1s timeout.
2. Verify socket file is removed even if drain timeout fires.
3. Verify compaction proceeds after drain timeout (handler task cancelled, Store Arc released).

**Coverage Requirement**: Integration test with controlled handler delay.

## Integration Risks

| Risk | Components | Scenario |
|------|-----------|----------|
| Engine crate dependency ordering | `unimatrix-engine` <-> `unimatrix-server` <-> `unimatrix-core` <-> `unimatrix-store` | Engine depends on core and store. Server depends on engine. Circular dependency would be a compile error. But if coaccess.rs references a server-only type during extraction, compilation fails. |
| PidGuard + SocketGuard lifecycle coordination | `pidfile.rs` <-> `uds_listener.rs` <-> `shutdown.rs` | PidGuard acquired before SocketGuard. SocketGuard dropped before PidGuard. If LifecycleHandles does not enforce this order, drop order is struct field order (last field drops first in Rust). |
| Shared Arc resources between stdio and UDS handlers | `server.rs` <-> `uds_listener.rs` | Both transports share `Arc<Store>`, `Arc<VectorIndex>`, `Arc<EmbedServiceHandle>`. UDS handler must not hold exclusive access that blocks MCP tool execution. Both must use `spawn_blocking` for redb ops. |
| Hook subcommand bypassing server initialization | `main.rs` <-> `hook.rs` | The hook path must branch before tokio runtime init. If clap parsing is inside the tokio main, the hook pays for runtime init even though it does not use it. |
| Event queue file locking | `event_queue.rs` <-> concurrent hook processes | Two hook processes writing to the same queue file simultaneously could interleave JSONL lines. File append is atomic up to PIPE_BUF (4096 bytes on Linux) for single-line writes, but multi-line writes are not atomic. |
| Socket permissions and umask | `uds_listener.rs` <-> OS | `std::fs::set_permissions(path, 0o600)` sets permissions after bind. Between bind and set_permissions, the socket has the default umask permissions. On most systems with umask 022, the socket would briefly be world-readable. |

## Edge Cases

| Edge Case | Expected Behavior |
|-----------|-------------------|
| Socket path > 108 bytes (Unix socket path limit) | `bind()` fails with ENAMETOOLONG. Error propagated; server starts without UDS (stdio-only). This can happen with very long home directory paths or very long project hashes. |
| Home directory does not exist (container, CI) | `ensure_data_directory` fails. Both server and hook subcommand exit with error. |
| Two hook processes connect at exactly the same instant | Server's `accept()` loop handles them sequentially. Each gets its own tokio task. No contention at the accept level. |
| Hook receives empty stdin (no pipe from Claude Code) | `stdin().read_to_string()` returns empty string. JSON parse fails. Hook exits 0 (graceful degradation per ADR-006). |
| Server shuts down while hook is mid-request | Hook receives broken pipe or EOF on read. `LocalTransport::request()` returns `TransportError::Transport`. Hook exits 0. |
| Project hash collision (two different projects produce same hash prefix) | Extremely unlikely (16 hex chars = 64 bits). Both projects share the same socket and database. Not handled; documented as accepted risk. |
| Event queue directory does not exist on first hook invocation | Hook creates `event-queue/` directory with `create_dir_all`. First write succeeds. |
| JSONL line exceeds OS pipe buffer size (>4096 bytes) | Single event payloads should be well under 4096 bytes. If they exceed it, concurrent writes could interleave. EventQueue should use file-level locking or exclusive open for safety. |
| Clock jumps backward (NTP correction) during event queue operation | Queue file timestamps could be non-monotonic. Pruning uses file modification time, not filename timestamp. Both should be checked. |
| Hook invoked with unknown EVENT argument | Dispatcher does not recognize the event name. Hook exits 0 (no error). Event is not queued (only known fire-and-forget events are queued). |

## Security Risks

| Component | Untrusted Input | Potential Damage | Blast Radius | Mitigation |
|-----------|----------------|------------------|--------------|------------|
| Hook subcommand (stdin) | Claude Code hook JSON | Malformed JSON could cause parse failure; oversized stdin could consume memory | Hook process only (ephemeral, ~15ms lifetime) | Defensive serde parsing (ADR-006). stdin size bounded by Claude Code. Exit 0 on any failure. |
| UDS listener (wire protocol) | Length-prefixed JSON from connecting process | Oversized length prefix causes OOM. Malformed JSON causes handler error. Unauthorized connections send arbitrary requests. | Server process (affects all MCP tools) | 1 MiB payload limit. UID verification. Process lineage check. Connection closed on auth failure. |
| Socket file (filesystem) | File permissions, symlink attacks | Attacker replaces socket with symlink to another socket -> server binds to wrong location. Attacker connects via same-user process. | Local session data | Mode 0o600. Unconditional unlink before bind (breaks symlink attack). UID verification for all connections. |
| Event queue files (filesystem) | Injected JSONL lines | Attacker with write access to queue directory injects malicious event records. On replay, server processes injected events. | Session telemetry data | Queue directory permissions (inherits from `~/.unimatrix/{hash}/` which is user-only). Events are telemetry, not commands -- replay cannot modify knowledge entries. |
| Project hash (derived from path) | Filesystem path | Path traversal in project root could cause data directory creation in unexpected location | Data directory location | `canonicalize()` resolves symlinks before hashing. Project root detection walks up to `.git/`, not arbitrary paths. |

## Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| Server starts, UDS bind fails (permissions, path too long) | Server logs warning, continues with stdio-only. All MCP tools work. Hooks degrade gracefully (queue or skip). |
| Hook connects, auth fails | Connection closed immediately. No response sent. Hook receives connection reset, exits 0. |
| Hook connects, server processing fails | Server sends `Error { code, message }` response. Hook logs error to stderr, exits 0. |
| Server out of file descriptors | UDS accept() returns error. Accept loop logs and continues (does not crash). New connections rejected until fds freed. Existing connections unaffected. |
| Hook cannot find socket file | Graceful degradation: fire-and-forget events queued, synchronous queries skipped. Exit 0. |
| Server crashes mid-request | Hook receives broken pipe. Exit 0. Socket file persists. Next server startup cleans it via unconditional unlink. |
| Event queue full (10 files, 10000 events) | Oldest file deleted. New events still written. Warning logged. |
| redb write contention during concurrent UDS + MCP | Tokio tasks queue behind spawn_blocking serialization. Response latency increases but correctness maintained. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (engine extraction breaks MCP tools) | R-01, R-02 | Incremental extraction (ADR-001), re-export for backward compatibility, full 1199-test suite after each move |
| SR-02 (socket lifecycle conflicts with PidGuard) | R-03, R-04, R-05 | Unconditional unlink after PidGuard (ADR-004), SocketGuard RAII, startup/shutdown ordering in architecture |
| SR-03 (latency budget 50ms) | R-06, R-18 | Blocking std I/O in hook process (ADR-002), no tokio init on hook path, benchmark test |
| SR-04 (macOS auth degrades) | R-10, R-11 | Layer 3 advisory-only (ADR-003), UID verification sufficient for local-dev threat model |
| SR-05 (Claude Code JSON format changes) | R-12 | Defensive serde parsing (ADR-006): `#[serde(default)]`, `#[serde(flatten)]`, `Option<T>` |
| SR-06 (concurrent UDS connections) | R-13, R-22 | One tokio task per connection, fire-and-forget for col-006 smoke tests, contention monitoring |
| SR-07 (event queue corruption) | R-15 | JSONL with skip-on-error replay, write-then-flush, file rotation |
| SR-08 (binary size growth) | -- | Accepted. No architecture risk. Monitor in CI (< 200 KB growth per NFR-07). |
| SR-09 (stale socket blocks restart) | R-04 | Unconditional unlink after PidGuard (ADR-004) |
| SR-10 (redb exclusive lock constraint) | -- | Not a risk -- architectural constraint. Shapes IPC architecture. All hook-to-server communication via UDS. |

## Test Infrastructure Needs

### New Test Helpers

| Helper | Location | Purpose |
|--------|----------|---------|
| `TestUdsServer` | `unimatrix-engine` or `unimatrix-server` test support | Spawns a UDS listener in a tempdir, accepts connections, provides request/response assertions. Uses the real wire protocol framing. |
| `TestHookProcess` | `unimatrix-server` test support | Spawns `unimatrix-server hook <EVENT>` as a child process with controlled stdin, captures stdout/stderr/exit code. |
| `TestSocketGuard` | `unimatrix-server` test support | Creates a socket file in a tempdir for lifecycle tests. Analogous to tempdir-based PidGuard tests. |
| `RawUdsClient` | `unimatrix-engine` test support | Low-level UDS client that sends raw bytes (not using LocalTransport) for testing malformed input, partial reads, and framing errors. |
| `EventQueueFixture` | `unimatrix-engine` test support | Creates an event queue directory in a tempdir with controlled file contents and timestamps for testing rotation, pruning, and replay. |

### Existing Test Infrastructure (Reused)

| Infrastructure | Source | Reused For |
|----------------|--------|-----------|
| `TestDb` | `unimatrix-store/src/test_helpers.rs` | Engine extraction tests that need a real redb database for confidence/coaccess computation |
| `TestEntry` + `seed_entries` | `unimatrix-store/src/test_helpers.rs` | Populating test databases for co-access boost verification |
| `tempfile::TempDir` | External crate (already in workspace) | Socket files, PID files, event queue directories |
| `PidGuard` tests pattern | `unimatrix-server/src/pidfile.rs` | SocketGuard tests follow the same RAII test pattern |

### Test Categories

| Category | Count (Estimated) | Scope |
|----------|-------------------|-------|
| Unit: wire protocol | 12-15 | Framing read/write, serialization/deserialization of all Request/Response variants, size limit enforcement |
| Unit: transport trait | 8-10 | LocalTransport connect/disconnect/request/fire_and_forget, timeout behavior, is_connected state |
| Unit: authentication | 8-10 | PeerCredentials extraction (per platform), UID verification, lineage check, auth failure handling |
| Unit: event queue | 10-12 | Enqueue, rotation at 1000 events, file limit at 10, pruning at 7 days, malformed line skip on replay |
| Unit: hook input parsing | 7-10 | HookInput deserialization: minimal, full, unknown fields, empty, non-JSON, type changes, missing hook_event_name |
| Unit: SocketGuard | 3-5 | Create, drop removes file, drop when already removed, drop on bad path |
| Unit: cortical-implant bootstrap | 2-3 | Bootstrap creates agent, idempotent, correct trust/capabilities |
| Unit: ProjectPaths extension | 2-3 | socket_path field present, deterministic, correct path |
| Integration: engine extraction regression | 0 (existing) | All 1199 existing tests pass without modification |
| Integration: UDS listener | 8-12 | Server startup creates socket, concurrent connections, auth rejection, error response, stdio independence |
| Integration: hook subcommand | 6-8 | SessionStart/Stop round-trip, Ping/Pong, server unavailable degradation, exit codes, stdin parsing |
| Integration: lifecycle | 4-6 | Startup with stale socket, shutdown cleanup, PidGuard+SocketGuard ordering, crash recovery |
| Benchmark: latency | 2-3 | Ping/Pong p95 < 50ms, process startup timing |
| **Total new tests** | **~70-95** | |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-19) | 11 scenarios |
| High | 7 (R-03, R-04, R-07, R-08, R-10, R-14, R-18) | 28 scenarios |
| Medium | 9 (R-05, R-06, R-09, R-11, R-12, R-13, R-16, R-17, R-22) | 30 scenarios |
| Low | 4 (R-15, R-20, R-21, R-23) | 10 scenarios |
| **Total** | **23** | **79 scenarios** |
