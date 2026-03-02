# col-006: Scope Risk Assessment

**Feature:** col-006 — Hook Transport Layer ("Cortical Implant")
**Assessed by:** Design Leader (Phase 1b)
**Date:** 2026-03-02
**Input:** SCOPE.md (approved), ASS-014 synthesis (D14-1 through D14-7), PRODUCT-VISION.md
**Risk ID prefix:** SR (Scope Risk)

---

## Risk Summary

col-006 is the most architecturally significant feature in M5. It adds a second communication channel (UDS) to the server, a new binary entry point (hook subcommand), and a new crate (unimatrix-engine). The redb exclusive lock constraint forces the IPC architecture -- this is certain and non-negotiable. The primary risk cluster is around the engine extraction (breaking existing tests) and UDS lifecycle management (stale sockets, concurrent connections, PidGuard coordination). Secondary risks involve platform-specific authentication differences and latency budget compliance.

| ID | Risk | Severity | Likelihood | Category |
|----|------|----------|------------|----------|
| SR-01 | Engine extraction breaks existing MCP tools | Critical | Medium | Technical |
| SR-02 | UDS socket lifecycle conflicts with PidGuard | High | Medium | Technical |
| SR-03 | Hook process startup + IPC exceeds 50ms budget | High | Low | Performance |
| SR-04 | SO_PEERCRED unavailable on macOS; authentication degrades | Medium | Certain (macOS) | Platform |
| SR-05 | Claude Code hook JSON format changes break parser | Medium | Low | External dependency |
| SR-06 | Concurrent UDS connections during swarm runs cause contention | Medium | Medium | Scalability |
| SR-07 | Event queue corruption on process crash | Low | Low | Data integrity |
| SR-08 | Binary size growth from bundled subcommand | Low | Low | Distribution |
| SR-09 | Stale socket file blocks server restart | Medium | Medium | Operational |
| SR-10 | redb exclusive lock is architectural constraint, not risk | N/A | Certain | Architectural |

---

## SR-01: Engine Extraction Breaks Existing MCP Tools

**Severity:** Critical
**Likelihood:** Medium
**Category:** Technical — crate restructuring

### Description

col-006 extracts `confidence.rs`, `coaccess.rs`, and `project.rs` from `unimatrix-server` into a new `unimatrix-engine` crate. This changes the dependency graph: `unimatrix-server` now depends on `unimatrix-engine`, which depends on `unimatrix-core`. The extraction touches the import paths, module visibility, and potentially function signatures of code used by every MCP tool.

The existing test suite (1025 unit + 174 integration tests) is the safety net. If the extraction introduces a regression that tests do not catch, it could silently break MCP tool behavior in production (confidence scores wrong, co-access boost missing, project hash mismatch).

### Why Medium Likelihood

- The modules being extracted (`confidence.rs`, `coaccess.rs`, `project.rs`) are relatively self-contained with clear boundaries
- Prior crate extractions in the project (nxs-004 core traits) succeeded without incident
- However, `confidence.rs` is called from multiple points in the server (tool handlers, usage recording, maintenance), and any missed call site could produce a compilation error or, worse, a silent behavior change if a local copy is left behind

### Impact

- All 10 MCP tools could produce incorrect confidence scores if the extraction drops a call path
- The `context_search` re-ranking formula depends on confidence + co-access boost; incorrect extraction breaks search quality
- `project.rs` extraction could change the project hash computation, causing the hook subcommand to look for the socket in the wrong directory

### Mitigation

1. **Incremental extraction**: Move one module at a time. Run the full 1199-test suite after each move. Do not batch-move all three modules.
2. **Compilation as first gate**: Rust's type system catches most extraction errors at compile time. Missing imports, wrong module paths, and visibility errors are compile failures.
3. **Integration test coverage**: The 174 integration tests exercise the full MCP tool pipeline including confidence computation, co-access boost, and project discovery. These are the primary regression gate.
4. **No behavioral changes**: The extraction is purely structural. No function signatures, algorithms, or constants should change. Any refactoring (renaming, restructuring) is explicitly out of scope during extraction.

### Architect Attention

The architect must define the exact module boundary for `unimatrix-engine`: which functions move, which stay, and how the server's tool handlers call into the engine. The boundary must preserve the existing `spawn_blocking` pattern for async wrappers.

---

## SR-02: UDS Socket Lifecycle Conflicts with PidGuard

**Severity:** High
**Likelihood:** Medium
**Category:** Technical — process lifecycle management

### Description

The MCP server currently manages a single lifecycle artifact: the PID file (`unimatrix.pid`) via `PidGuard` (RAII with flock). col-006 adds a second lifecycle artifact: the UDS socket (`unimatrix.sock`). Both live in `~/.unimatrix/{project_hash}/`. The startup and shutdown sequences must coordinate:

- **Startup order**: PidGuard acquisition must succeed before socket binding. If PidGuard detects a stale server and terminates it, the stale server's socket must also be cleaned up.
- **Shutdown order**: Socket must be removed before database compaction (which takes an exclusive write lock and may be slow). If shutdown is interrupted between socket removal and compaction, the next startup sees no stale socket but may find a stale PID.
- **Crash recovery**: If the server crashes, both the PID file and socket file may be left behind. The existing stale PID detection (`handle_stale_pid_file()` + `is_unimatrix_process()`) handles the PID file. A parallel mechanism is needed for stale sockets.

### Why Medium Likelihood

- The PidGuard pattern is well-tested and stable (vnc-004), but adding a second lifecycle artifact introduces new ordering dependencies
- Server crashes during shutdown are infrequent but do occur (observed during extended sessions per vnc-004 bug report)
- The socket and PID file have different cleanup semantics: PID file uses flock (kernel-managed), socket uses filesystem operations (application-managed)

### Impact

- Stale socket blocks new server from binding the UDS listener (address already in use)
- Stale socket accepted by hook process leads to connection refused (no listener) or, worse, connection to wrong process
- Incorrect shutdown order could leave hooks connecting to a socket with no server behind it during the compaction window

### Mitigation

1. **Unified lifecycle manager**: Extend `LifecycleHandles` (vnc-004) to include the socket alongside PidGuard. Both are created in startup, both are cleaned up in shutdown, in defined order.
2. **Stale socket detection**: On startup, before binding, try `connect()` to existing socket. If connection refused, the socket is stale -- remove it. If connection succeeds, another server is running -- abort with error (same as PidGuard `DatabaseAlreadyOpen`).
3. **Startup order**: PidGuard -> socket bind -> accept connections. Shutdown order: stop accepting -> drain active requests -> remove socket -> compact -> release PidGuard.
4. **Crash recovery test**: Integration test that simulates server crash (leave both PID and socket files), then verifies next startup cleans up both.

### Architect Attention

The architect should define the `LifecycleHandles` extension and the exact startup/shutdown ordering as part of the architecture document. The socket lifecycle should follow PidGuard patterns (RAII where possible, explicit cleanup on drop).

---

## SR-03: Hook Process Startup + IPC Exceeds 50ms Budget

**Severity:** High
**Likelihood:** Low
**Category:** Performance

### Description

The 50ms latency budget for synchronous hooks (UserPromptSubmit, PreCompact) covers the full chain: OS process fork+exec, Rust binary initialization, clap argument parsing, project hash computation, socket discovery, UDS connect, request serialization, server processing, response deserialization, stdout write, and process exit.

ASS-014 estimates:
- Process startup: ~3ms (compiled Rust binary, no ONNX)
- Socket connect: ~0.5ms (UDS to running server)
- Request/response serialization: ~0.2ms (JSON)
- Server processing: ~2-10ms (depends on operation)
- Total: ~6-14ms for a simple operation

The budget appears comfortable. However, the estimates are analytical (from ASS-014 research), not measured. Real-world factors could increase latency: filesystem latency for project hash computation (reading `.git/` metadata), tokio runtime initialization in the hook process, or server-side contention from concurrent MCP requests.

### Why Low Likelihood

- The per-component estimates are conservative and based on well-understood costs
- col-006 smoke tests (Ping/Pong) have minimal server-side processing, so the 50ms budget is ample
- The latency-critical operations (search, briefing) are in col-007/col-008, not col-006
- Similar architectures (claude-flow's hook router) operate well within this budget

### Impact

- If Ping/Pong exceeds 50ms, the entire hook architecture is invalidated
- Future features (col-007 search at 12-36ms, col-008 compaction at 5-15ms) have tighter margins
- Claude Code may have its own timeout for hook processes (undocumented), which could be shorter than 50ms

### Mitigation

1. **Prototype early**: Build the Ping/Pong round-trip first. Measure end-to-end latency before building the rest of the feature.
2. **Latency instrumentation**: Add timing markers at each stage (startup, connect, serialize, server process, deserialize) to identify bottlenecks.
3. **Minimize hook binary startup**: The hook subcommand should avoid initializing anything it does not need (no ONNX, no redb, no HNSW). Clap argument parsing and project hash computation should be the only startup costs.
4. **Connection reuse consideration**: If latency is marginal, investigate connection pooling or a keep-alive socket per session. This adds complexity but eliminates the per-hook connect overhead.

### Architect Attention

The architect should specify the minimum viable hook process initialization path and identify what can be deferred or skipped. Tokio runtime initialization in the hook process deserves attention -- if the hook only does synchronous I/O (UDS connect, read, write), a tokio runtime may be unnecessary.

---

## SR-04: SO_PEERCRED Unavailable on macOS; Authentication Degrades

**Severity:** Medium
**Likelihood:** Certain (on macOS)
**Category:** Platform compatibility

### Description

The layered authentication model uses SO_PEERCRED (Linux) or getpeereid (macOS) to verify that the connecting hook process runs as the same user as the server. On Linux, SO_PEERCRED provides the full `ucred` struct (PID, UID, GID). On macOS, `getpeereid()` provides only UID and GID -- no PID. This means the process lineage check (`/proc/{pid}/cmdline` verification) is unavailable on macOS.

Without PID-based lineage verification, authentication on macOS falls back to:
- Layer 1: Filesystem permissions (socket mode 0o600, same user owns the socket file)
- Layer 2: UID verification via getpeereid (same user)
- Layer 3: Unavailable (no PID to inspect)

### Why Certain on macOS

This is a platform difference, not a probability. macOS does not expose PID in UDS peer credentials, and `/proc/` does not exist on macOS.

### Impact

- On macOS, any process running as the same user can connect to the UDS and send requests
- The threat model (ASS-014 RQ-4) considers this acceptable for local development: same-user processes are trusted in the local threat model
- The risk increases in shared environments (multi-user systems, CI runners with shared UIDs), but these are not the primary deployment target

### Mitigation

1. **Accept the degradation**: For local development, UID-only verification is sufficient. The socket is only accessible to the owner (mode 0o600), and same-user processes are within the trust boundary.
2. **Shared secret fallback**: For environments requiring stronger authentication, use a shared secret file (`~/.unimatrix/{hash}/auth.token`, 32 random bytes, mode 0o600). The hook reads the token and includes it in the request. The server validates it. This is optional and only needed in shared environments.
3. **Document the difference**: The architecture document should explicitly state the authentication level per platform and the conditions under which the shared secret is recommended.

### Architect Attention

The architect should decide whether the shared secret fallback is in scope for col-006 or deferred to a future feature. Recommendation: defer to post-col-006 unless shared-environment deployment is an immediate need.

---

## SR-05: Claude Code Hook JSON Format Changes Break Parser

**Severity:** Medium
**Likelihood:** Low
**Category:** External dependency

### Description

The hook subcommand parses JSON from stdin provided by Claude Code. The JSON includes `hook_event_name`, `session_id`, `cwd`, `transcript_path`, and event-specific fields. Anthropic documents the hook interface but has not made explicit stability guarantees. A Claude Code update could:
- Rename fields (e.g., `session_id` -> `sessionId`)
- Add required fields that the parser does not expect
- Change field types (e.g., `session_id` from string to integer)
- Remove fields that the parser depends on

### Why Low Likelihood

- The hook API has been stable across multiple Claude Code releases
- The core fields (`hook_event_name`, `session_id`, `cwd`) are fundamental to hook operation -- changing them would break all hook integrations, not just Unimatrix
- Anthropic has incentive to maintain backward compatibility for the hook ecosystem

### Impact

- Parser failure causes the hook to exit with error, printing nothing to stdout
- For synchronous hooks, this means no context injection (graceful degradation)
- For fire-and-forget hooks, events are lost (but queued if the event queue is working)
- The failure is silent to the user unless they check stderr

### Mitigation

1. **Defensive parsing**: Use `#[serde(default)]` on all fields. Use `#[serde(flatten)]` to capture unknown fields. Use `Option<T>` for fields that may be absent.
2. **Graceful failure**: If parsing fails, log to stderr and exit 0 (non-blocking). Never exit non-zero on parse failure -- this would show an error to the user.
3. **Version detection**: Check for a version field in the hook JSON if one exists. Log a warning if the version is newer than expected.
4. **Integration test against schema**: Maintain test fixtures with the documented Claude Code hook JSON format. Update fixtures when Claude Code releases new versions.

### Architect Attention

The architect should define the serde model for hook event parsing with maximum forward compatibility. The parser should never fail on unknown fields or missing optional fields.

---

## SR-06: Concurrent UDS Connections During Swarm Runs

**Severity:** Medium
**Likelihood:** Medium
**Category:** Scalability

### Description

During swarm runs, Claude Code spawns multiple subagents. Each subagent triggers hooks independently. If 5-10 subagents are active simultaneously, 5-10 hook processes may connect to the UDS concurrently. Each connection requires the server to accept, authenticate, parse, process, and respond.

The server uses tokio for async I/O. Each UDS connection is handled by a spawned task. The concern is not the connection handling itself (tokio handles thousands of concurrent connections) but the server-side processing: redb write transactions are serialized (single-writer), and HNSW searches may contend with MCP tool requests on the stdio transport.

### Why Medium Likelihood

- Swarm runs with 5-10 concurrent agents are the standard workflow in the Unimatrix project
- Each agent fires SessionStart, Stop, and potentially other hooks concurrently
- col-006 smoke tests (SessionStart, Stop) are fire-and-forget, so contention is lower than it would be for synchronous queries (col-007)

### Impact

- Increased latency for individual hook responses due to server-side queuing
- redb write serialization means fire-and-forget events queue behind each other
- If response latency exceeds 50ms for synchronous hooks, the hook process may time out
- No data loss (events are processed in order), but throughput may degrade

### Mitigation

1. **Fire-and-forget batching**: For col-006 smoke tests, all hooks are fire-and-forget. The hook process sends and disconnects immediately. Server processes asynchronously.
2. **Connection-per-request model**: Each hook opens a new connection, sends one request, and closes. No connection pooling (which would require hook-side state). This is simple and isolates failures.
3. **Server-side write batching**: Future optimization -- batch multiple fire-and-forget events into a single redb write transaction. Not needed for col-006 (only Ping and session events).
4. **Backpressure monitoring**: Instrument the UDS listener with a pending connection counter. If the backlog exceeds a threshold (e.g., 100), reject new connections with a specific error code.

### Architect Attention

The architect should define the concurrency model for the UDS listener: one task per connection, maximum concurrent connections (if any limit), and the interaction between UDS request processing and stdio MCP request processing (shared thread pool? separate tokio tasks?).

---

## SR-07: Event Queue Corruption on Process Crash

**Severity:** Low
**Likelihood:** Low
**Category:** Data integrity

### Description

When the MCP server is unavailable, the hook subcommand queues fire-and-forget events to a local JSONL file (`~/.unimatrix/{hash}/event-queue/pending-{ts}.jsonl`). If the hook process crashes mid-write (e.g., killed by the OS, disk full, power loss), the JSONL file may contain a partial line.

### Why Low Likelihood

- Hook processes are short-lived (~10ms); the window for a crash during a single line write is extremely small
- JSONL files are append-only, so corruption affects at most the last line
- The event queue is best-effort telemetry, not critical data

### Impact

- A partial line in the JSONL file causes the server to skip that line during queue replay (standard JSONL resilience: skip malformed lines)
- At most one event is lost per crash
- No cascading failures -- the queue file remains usable for all other lines

### Mitigation

1. **Write-then-flush**: Write the full JSON line, then flush. Atomic at the filesystem level for lines under ~4KB (typical for event records).
2. **Skip malformed lines**: Queue replay parser skips lines that do not parse as valid JSON, with a warning log.
3. **File rotation**: Rotate queue files at 1000 events. If one file is corrupted, only that file's partial line is affected; other files are intact.

### Architect Attention

No special attention needed. Standard JSONL resilience patterns apply. The architect should confirm that queue replay uses line-by-line parsing with skip-on-error semantics.

---

## SR-08: Binary Size Growth from Bundled Subcommand

**Severity:** Low
**Likelihood:** Low
**Category:** Distribution

### Description

The hook subcommand adds code to the existing `unimatrix-server` binary: clap argument parsing for the `hook` subcommand, UDS client code, JSON serialization for the transport protocol, and event dispatch logic. The current binary is ~17 MB (Linux aarch64, release, dynamically linked against ONNX Runtime).

### Why Low Likelihood

- ASS-014 estimates the hook dispatch logic at ~50-100 KB of compiled code
- The UDS client, JSON serialization, and clap dependencies are already in the binary (server uses tokio-net, serde_json, and clap)
- No new heavy dependencies are introduced (no ONNX, no redb, no HNSW -- these are already linked)

### Impact

- Binary grows from ~17 MB to ~17.1 MB -- negligible
- No impact on distribution, installation, or startup time

### Mitigation

1. **Monitor in CI**: Add binary size tracking to CI. Alert if binary grows by more than 1 MB.
2. **No action needed for col-006**: The growth is within noise.

### Architect Attention

None. This risk is included for completeness per the consolidated risk register (R-16 in synthesis.md is about macOS auth, not binary size -- but the feature-scoping document mentions binary size as a consideration).

---

## SR-09: Stale Socket File Blocks Server Restart

**Severity:** Medium
**Likelihood:** Medium
**Category:** Operational

### Description

If the MCP server crashes or is killed (SIGKILL), the UDS socket file (`~/.unimatrix/{hash}/unimatrix.sock`) persists on the filesystem. When the server restarts, `bind()` on the same path fails with `EADDRINUSE` unless the stale socket is detected and removed first.

This is analogous to the stale PID file problem solved by PidGuard (vnc-004), but sockets have different semantics: a PID file's staleness is verified by checking whether the PID's process exists. A socket file's staleness is verified by attempting a connection.

### Why Medium Likelihood

- Server crashes do occur (vnc-004 was specifically about process lifecycle reliability)
- SIGKILL bypasses all cleanup handlers (RAII Drop, signal handlers)
- The scenario is reproducible: kill the server process, then try to restart

### Impact

- Server fails to start the UDS listener, falling back to stdio-only mode
- All hook processes fail to connect (no socket), triggering graceful degradation (skip injection, queue events)
- The user sees no error in the MCP client (stdio works), but hooks silently fail
- Manual intervention required: delete the socket file

### Mitigation

1. **Stale socket detection on startup**: Before `bind()`, check if the socket file exists. If yes, attempt `connect()`. If connection refused (no listener), the socket is stale -- `unlink()` and proceed. If connection succeeds, another server is running -- abort.
2. **Unconditional unlink before bind**: Simpler alternative -- always `unlink()` the socket path before `bind()`. Safe because: if another server is running, PidGuard will have already detected it and either terminated it or aborted. By the time we reach socket bind, we know we are the only server.
3. **RAII socket cleanup**: Implement a `SocketGuard` (analogous to `PidGuard`) that unlinks the socket on Drop. This handles graceful shutdown. Crash recovery is handled by startup detection.

### Architect Attention

The architect should choose between stale detection (connect test) and unconditional unlink (relying on PidGuard for mutual exclusion). Recommendation: unconditional unlink is simpler and safe given PidGuard ordering.

---

## SR-10: redb Exclusive Lock Is Architectural Constraint

**Severity:** N/A (constraint, not risk)
**Likelihood:** Certain
**Category:** Architectural

### Description

redb v3.1.x acquires an exclusive file lock on `Database::create()` / `Database::open()`. No second process can open the database while the MCP server is running. This is confirmed by the existing test `test_open_already_open_returns_database_error()` in `crates/unimatrix-store/src/db.rs`.

This is not a risk to be mitigated -- it is a constraint that shapes the entire architecture. The IPC model (hook processes communicate with the server via UDS) is the direct consequence of this constraint.

### Why Included

- Every downstream design decision (Transport trait, UDS listener, engine extraction, authentication, graceful degradation) traces back to this constraint
- The architect and specification writer must treat this as a given, not a risk to evaluate alternatives for
- If redb adds concurrent access in a future version, the architecture could simplify -- but col-006 must not depend on this

### Architect Attention

The architecture document should state this constraint prominently and trace all design decisions back to it. This is the "why" behind the entire col-006 design.

---

## Risk Priority Matrix

| Priority | Risks | Rationale |
|----------|-------|-----------|
| Must address in architecture | SR-01 (engine extraction), SR-02 (socket lifecycle), SR-10 (redb constraint) | These shape component boundaries and lifecycle management |
| Must address in specification | SR-03 (latency budget), SR-05 (JSON parsing), SR-06 (concurrent connections) | These define acceptance criteria and test scenarios |
| Address in risk strategy | SR-04 (macOS auth), SR-07 (queue corruption), SR-09 (stale socket) | These require test scenarios and degradation paths |
| Monitor only | SR-08 (binary size) | Low impact, low likelihood |

---

## Top 3 Risks for Architect Attention

1. **SR-01 (Engine Extraction)**: The `unimatrix-engine` crate boundary is the highest-risk design decision. The architect must define exactly which modules move, in what order, and how the server's call sites are updated. The incremental extraction strategy (one module at a time, full test suite after each) is the primary mitigation.

2. **SR-02 (Socket Lifecycle)**: The PidGuard + socket lifecycle coordination requires a clear startup/shutdown ordering specification. The architect should extend `LifecycleHandles` to include the socket and define crash recovery behavior (stale socket detection, unconditional unlink, or both).

3. **SR-06 (Concurrent Connections)**: The architect must define the concurrency model for the UDS listener, especially the interaction between UDS request processing and stdio MCP request processing. During swarm runs, 5-10 concurrent hook connections are expected alongside the active MCP session.
