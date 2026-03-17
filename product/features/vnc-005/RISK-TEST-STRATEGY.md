# Risk-Based Test Strategy: vnc-005

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `Arc::try_unwrap(store)` fails at graceful shutdown because a session task clone was not joined before shutdown runs | High | Med | Critical |
| R-02 | Race between daemon SIGTERM delivery and in-flight session task spawning allows a session to be spawned after the accept loop breaks, creating an untracked Arc clone | High | Med | Critical |
| R-03 | `graceful_shutdown` called on session EOF (old code path not fully removed) shuts down daemon on first client disconnect | High | Med | Critical |
| R-04 | Daemonization launcher spawns second daemon on a system where `is_unimatrix_process` returns false for a stale PID on macOS (no `/proc`) causing double-daemon | High | Med | Critical |
| R-05 | `PendingEntriesAnalysis::drain_for` called concurrently with `upsert` from a hook stop event on a different session; Mutex not held across the caller's read-modify — entries from one session silently lost | High | Low | High |
| R-06 | MCP socket created without `0600` permission — permission bits inherited from umask rather than explicitly set, allowing other-user access | High | Low | High |
| R-07 | `CallerId::UdsSession` rate-limit exemption applied to a future HTTP transport caller because the exemption has no code-comment boundary marker | High | Low | High |
| R-08 | Bridge auto-start spawns daemon but daemon fails to create socket within 5 seconds on a loaded system; bridge exits 1 with no user-actionable error message (no log path in stderr) | Med | Med | High |
| R-09 | Stale `unimatrix-mcp.sock` from a crashed daemon blocks new daemon bind because `handle_stale_socket` is not applied to the MCP socket path | Med | Med | High |
| R-10 | Session handle `Vec` grows unboundedly on a long-running daemon with repeated connect/disconnect cycles because the `retain(is_finished)` sweep is not implemented or fires only at shutdown | Med | Med | High |
| R-11 | 33rd connection is dropped silently (stream closed without MCP error response) — the client sees a connection-closed, retries in a tight loop, hitting the cap repeatedly | Med | Med | Med |
| R-12 | `unimatrix serve --stdio` (the regression path) no longer exits when stdin closes because the `QuitReason::Closed` → `graceful_shutdown` path was restructured for daemon mode | High | Med | Critical |
| R-13 | `unimatrix hook` invocation path inadvertently initializes a Tokio runtime because bridge-path async code was introduced into main.rs above the `Command::Hook` match arm | Med | Med | Med |
| R-14 | Socket path exceeds 104 bytes on macOS for a user with a long home directory; bind silently truncates or fails with a cryptic OS error | Med | Low | Med |
| R-15 | Per-bucket 1000-entry cap evicts entries by lowest `rework_flag_count` while concurrent `upsert` is writing; cap enforcement reads the inner HashMap while another thread is inserting, producing incorrect eviction under concurrent load | Med | Low | Med |
| R-16 | `LifecycleHandles::mcp_socket_guard` not dropped before `graceful_shutdown` returns, leaving `unimatrix-mcp.sock` on disk after daemon exit, causing the next auto-start to fail stale-check | Med | Med | Med |
| R-17 | `--daemon-child` hidden flag is not hidden in clap help output, confusing operators who see it and invoke it directly, triggering daemon init without proper parent polling | Low | Low | Low |
| R-18 | TTL eviction in background tick removes a bucket while `context_retrospective` drain is in progress; double-free or missing entries if the eviction lock window overlaps | Med | Low | Med |

---

## Risk-to-Scenario Mapping

### R-01: Arc::try_unwrap(store) fails at graceful shutdown
**Severity**: High
**Likelihood**: Med
**Impact**: Daemon panics at shutdown; DB compaction never runs; vector dump not written; data written during the session is lost.

**Evidence**: Entry #81 / #33 (ADR-005) establishes `Arc::try_unwrap` as the shutdown correctness invariant. Entry #312 (bugfix #92) shows this exact failure occurred during vnc-006–009 when a refactor left a stale Arc clone.

**Test Scenarios**:
1. Start daemon; open 4 concurrent MCP sessions; send SIGTERM; assert all session tasks exit before graceful_shutdown is invoked; assert shutdown completes without panic.
2. Start daemon; open 1 session; close it (bridge stdin EOF); immediately send SIGTERM; assert graceful_shutdown succeeds (session clone fully dropped before try_unwrap).
3. Inspect Arc strong_count on store immediately before `graceful_shutdown` call in a test harness — assert count == 1.

**Coverage Requirement**: Shutdown must succeed with N sessions (N=1, N=4) and with sessions closed before and during SIGTERM delivery.

---

### R-02: Race between SIGTERM and session spawn
**Severity**: High
**Likelihood**: Med
**Impact**: A session task spawned after the accept loop breaks holds an Arc clone that is never joined; `Arc::try_unwrap(store)` fails.

**Test Scenarios**:
1. Inject a synthetic delay between `daemon_token.cancelled()` firing and the accept loop breaking; concurrently trigger a new connection; assert the connection is either (a) refused before spawning a task or (b) the task is included in the join set.
2. Stress test: fire SIGTERM simultaneously with a rapid accept burst (10 connections in <10ms); assert graceful_shutdown completes without panic.

**Coverage Requirement**: The accept loop's select! branch ordering guarantees that once `cancelled()` wins the select, no further `listener.accept()` result is processed. Test must confirm this ordering under concurrent load.

---

### R-03: Session EOF triggers graceful shutdown (old code path)
**Severity**: High
**Likelihood**: Med
**Impact**: First client disconnect kills the daemon and all background state. Indistinguishable from correct behavior in a quick smoke test — only visible when a second session attempts to connect.

**Test Scenarios**:
1. (AC-04) Start daemon; connect bridge A; send `initialize`; close bridge A stdin; assert daemon PID still alive 2 seconds later; connect bridge B; assert bridge B receives a valid `initialize` response.
2. Start daemon in stdio mode (`serve --stdio`); close stdin; assert daemon exits (negative test — stdio path MUST still exit on transport close).
3. Search main.rs for all `graceful_shutdown` call sites after implementation; assert exactly one call site exists and it is reached only from the daemon token cancellation path, not from `QuitReason::Closed`.

**Coverage Requirement**: Must test daemon survival after N session disconnects (N=1, N=3). Must test stdio path still exits correctly (regression).

---

### R-04: Double-daemon on macOS stale PID
**Severity**: High
**Likelihood**: Med
**Impact**: Two daemon processes compete for the same SQLite database and socket path. SQLite WAL mode tolerates multiple readers but `Mutex<Connection>` in a second process creates a second lock instance — no cross-process serialization.

**Evidence**: ADR-006 notes the macOS `/proc` fallback gap. PidGuard's `flock` is the real enforcement, but `flock` behavior on NFS or certain macOS filesystem mounts is unreliable.

**Test Scenarios**:
1. Start daemon; write stale PID file with a recycled non-unimatrix PID; invoke bridge; assert bridge does NOT spawn a second daemon (flock prevents it); assert one daemon process.
2. (AC-07) Start daemon; invoke `unimatrix serve --daemon` a second time; assert exit code non-zero; assert exactly one daemon in process table.
3. On macOS: kill daemon with SIGKILL (bypasses graceful shutdown, leaves socket + PID file); invoke bridge; assert bridge detects stale PID via `is_process_alive` fallback and spawns fresh daemon; assert no double-daemon.

**Coverage Requirement**: Must cover healthy daemon present, stale PID with dead process, and stale PID with reused PID (non-unimatrix process).

---

### R-05: Concurrent drain/upsert data loss in accumulator
**Severity**: High
**Likelihood**: Low
**Impact**: Entries stored during a session silently absent from retrospective output. Silent data loss is worse than a visible error.

**Evidence**: Entry #731 (batched fire-and-forget) and #735 (spawn_blocking pool saturation) both show that short-duration Mutex contention on the DB write path has caused silent failures.

**Test Scenarios**:
1. Spawn 4 concurrent goroutines each calling `upsert` 250 times on the same `feature_cycle` bucket while a fifth goroutine repeatedly calls `drain_for` on the same bucket; assert total entries seen across all drains equals total entries inserted with no duplicates.
2. Unit test `PendingEntriesAnalysis` directly: concurrent upsert + drain under a Mutex; assert no panic and correct entry count.
3. (AC-17, AC-18) Multi-session accumulation test: session A upserts 2 entries, closes; session B upserts 1 entry; `drain_for` returns exactly 3 entries; second drain returns 0.

**Coverage Requirement**: Both unit-level (direct struct methods) and integration-level (via MCP tool calls) concurrency coverage required.

---

### R-06: MCP socket permissions not 0600
**Severity**: High
**Likelihood**: Low
**Impact**: Other users on the same system can connect to the MCP socket and issue tool calls without UID-based auth (UID check is the existing layer 2 guard, but socket-level ownership is layer 1).

**Evidence**: Entry #300 (UDS Fixed Capability Set) and entry #244 (Layered Authentication Without Shared Secret) both treat socket-level ownership as the outer auth boundary.

**Test Scenarios**:
1. (AC-02) Start daemon; `stat unimatrix-mcp.sock`; assert mode bits are exactly `0600` (no group-read, no other-read).
2. Assert socket permissions are set BEFORE the accept loop starts (no window where socket is created with wrong permissions and a connection arrives). Implementation review: `std::fs::set_permissions` or `SocketAddr::from_pathname` + `bind` + `chmod` must complete before `accept` is called.
3. Verify the hook IPC socket (`unimatrix.sock`) permissions are unaffected — still `0600` — after adding the second socket.

**Coverage Requirement**: Permission check must be a test assertion, not an implementation comment. Both sockets must be independently verified.

---

### R-07: CallerId::UdsSession exemption boundary missing
**Severity**: High
**Likelihood**: Low
**Impact**: When HTTP transport is introduced in W2-2, the exemption is naively inherited, allowing remote callers to bypass rate limiting. Silent security regression introduced months after vnc-005 ships.

**Test Scenarios**:
1. Code review / static assertion: the `CallerId::UdsSession` match arm in the rate-limit enforcement code contains a comment referencing C-07 / W2-2 boundary constraint.
2. Unit test: confirm `CallerId::HttpSession` (or equivalent future variant) does NOT match the UdsSession arm; assert rate-limiting is applied.

**Coverage Requirement**: Comment presence is a gate requirement (C-07). Test must assert that only UDS callers receive the exemption — not all callers.

---

### R-08: Bridge auto-start timeout with no actionable error
**Severity**: Med
**Likelihood**: Med
**Impact**: Claude Code session silently fails to start with an opaque "exit 1" — developer cannot diagnose without knowing the log file location.

**Test Scenarios**:
1. (AC-15) Simulate a daemon that never creates `unimatrix-mcp.sock`; invoke bridge; assert exit code 1 within 7 seconds; assert stderr contains the log file path string.
2. Verify the stderr message includes the word "timeout" or equivalent human-readable explanation.
3. Verify that after a timeout, no orphaned daemon process remains (the launched daemon stub is not left running).

**Coverage Requirement**: Error message format is a testable requirement. Exact log path in stderr is required by AC-15.

---

### R-09: Stale MCP socket blocks new daemon bind
**Severity**: Med
**Likelihood**: Med
**Impact**: Auto-start after a daemon crash fails with a bind error. Developer is stuck until manually removing the socket file.

**Evidence**: Entry #245 (ADR-004 from vnc-004) establishes unconditional unlink as the correct pattern.

**Test Scenarios**:
1. (AC-16) Create `unimatrix-mcp.sock` as a plain file (no listening daemon); invoke `unimatrix serve --daemon`; assert launcher exits 0; assert new socket is bound and accepts connections.
2. Create stale socket owned by a different UID (simulate NFS scenario); assert daemon either unlinks and re-binds, or fails with a clear error message (not a silent hang).
3. Assert `handle_stale_socket` is applied to `unimatrix-mcp.sock` AT daemon startup, not only when `serve --daemon` is invoked explicitly (auto-start child must also do the stale check).

**Coverage Requirement**: Must test stale socket unlink before each startup path: explicit `serve --daemon`, bridge-triggered auto-start, and `--daemon-child` internal flag path.

---

### R-10: Session handle Vec grows unboundedly
**Severity**: Med
**Likelihood**: Med
**Impact**: Long-running daemon (weeks of continuous operation) accumulates thousands of completed `JoinHandle` entries. Memory grows monotonically. Background tick is unaffected but daemon memory footprint grows until OOM.

**Test Scenarios**:
1. Simulate 100 sequential connect/disconnect cycles against the acceptor; assert `session_handles.len()` after all disconnects is bounded (≤ 1 or ≤ configured sweep granularity, not 100).
2. Assert `retain(|h| !h.is_finished())` sweep runs on each accept loop iteration, not only at shutdown.
3. Measure daemon RSS before and after 50 connect/disconnect cycles; assert RSS delta is under 1 MB.

**Coverage Requirement**: Vec growth must be bounded by the retain sweep. Test must cover the sweep-per-accept behavior, not just total count.

---

### R-11: 33rd connection silently dropped — client retry loop
**Severity**: Med
**Likelihood**: Med
**Impact**: A Claude Code instance reconnecting after a network-like blip sees immediate close and retries in a fast loop, consuming file descriptors without ever getting an error it can display to the user.

**Test Scenarios**:
1. (AC-20) Open 32 concurrent sessions; attempt 33rd; assert 33rd stream receives connection-close without panic in daemon; assert the 32 existing sessions are unaffected.
2. Simulate rapid reconnect from 33rd client (10 attempts in 1 second); assert daemon does not panic, does not log excessive errors, and does not exhaust file descriptors.
3. Assert the daemon logs a `warn!` or `info!` level message for each dropped connection (not silent).

**Coverage Requirement**: Cap enforcement test (AC-20) plus reconnect stress test.

---

### R-12: `unimatrix serve --stdio` no longer exits on stdin close
**Severity**: High
**Likelihood**: Med
**Impact**: Existing pre-vnc-005 behavior broken. CI/CD pipelines that use `unimatrix` directly (without daemon) hang indefinitely.

**Test Scenarios**:
1. (AC-12) Invoke `unimatrix serve --stdio`; pipe MCP `initialize`; close stdin; assert process exits within 5 seconds; assert exit code 0; assert `unimatrix-mcp.sock` was NOT created.
2. Invoke `unimatrix serve --stdio`; send SIGTERM; assert graceful_shutdown runs and process exits (same as daemon behavior — SIGTERM must still work on stdio mode).
3. Invoke `unimatrix serve --stdio` with a pre-existing running daemon; assert the two processes do not conflict (they share no sockets in stdio mode).

**Coverage Requirement**: Stdio mode must be tested as a first-class path, not just as a fallback comment. This is the primary regression gate.

---

### R-13: Hook path accidentally initializes Tokio runtime
**Severity**: Med
**Likelihood**: Med
**Impact**: `unimatrix hook SessionStart` exceeds 50ms budget; hook events are delayed; background orchestration misfires.

**Evidence**: Entry #243 (ADR-002 vnc-001: Hook Process Uses Blocking std I/O) establishes the no-Tokio invariant for hook paths.

**Test Scenarios**:
1. Time `unimatrix hook SessionStart --feature test-001` against a running daemon; assert wall clock under 50ms.
2. Trace process startup with `strace`/`dtrace` or equivalent; assert no futex contention from Tokio thread-pool initialization.
3. Code review: the `Command::Hook` match arm must be reached without any `#[tokio::main]` or `Runtime::new()` above it in `main.rs`.

**Coverage Requirement**: Timing regression test plus code-level gate that no tokio init precedes the hook dispatch.

---

### R-14: Socket path exceeds 104-byte macOS limit
**Severity**: Med
**Likelihood**: Low
**Impact**: Daemon fails to bind on macOS with an OS error that is not surfaced clearly, causing silent auto-start failure.

**Test Scenarios**:
1. (FR-20) Construct a `ProjectPaths` with a home directory of exactly 70 chars; assert the path validation at bind time triggers a fatal error with a human-readable message.
2. Assert the validation is performed before `UnixListener::bind` is called (not after an OS error).
3. Verify that a path of 103 bytes passes validation and binds successfully.

**Coverage Requirement**: Unit test `validate_socket_path_length` with boundary inputs: 103 bytes (pass), 104 bytes (pass on Linux, borderline macOS), 107 bytes (fail on both).

---

### R-15: Per-bucket entry cap eviction under concurrent upsert
**Severity**: Med
**Likelihood**: Low
**Impact**: Eviction reads the inner HashMap while another session is inserting; count comparison is stale; cap is either over-applied (entries dropped that should be kept) or not applied (bucket grows past 1000).

**Test Scenarios**:
1. Unit test: fill a `FeatureBucket` to 999 entries; concurrently upsert 2 more entries and trigger eviction from a second thread (while Mutex is held by first); assert final bucket size ≤ 1000 and no panic.
2. Assert eviction only runs inside the Mutex critical section, not before acquiring it.

**Coverage Requirement**: Eviction correctness test at cap boundary (999, 1000, 1001 entries).

---

### R-16: mcp_socket_guard not dropped before graceful_shutdown returns
**Severity**: Med
**Likelihood**: Med
**Impact**: `unimatrix-mcp.sock` persists on disk after daemon exit. Next auto-start calls `handle_stale_socket`, succeeds anyway (unconditional unlink) — so the symptom is mild. However, if `handle_stale_socket` is NOT applied to the MCP socket path (R-09), this becomes a startup blocker.

**Test Scenarios**:
1. Start daemon; send SIGTERM; wait for exit; assert `unimatrix-mcp.sock` does NOT exist after exit.
2. Assert `unimatrix.sock` (hook IPC) also does not exist after exit (both guards must drop).
3. Start daemon; kill with SIGKILL (skips graceful shutdown); assert both socket files exist (expected); assert next daemon start cleans them up via `handle_stale_socket`.

**Coverage Requirement**: Both SocketGuard drops must be independently verified in the graceful shutdown path.

---

### R-17: `--daemon-child` visible in help output
**Severity**: Low
**Likelihood**: Low
**Impact**: Operator invokes `unimatrix serve --daemon --daemon-child` manually and initializes a daemon child without the parent's socket polling loop, leaving a daemon that is alive but whose presence was never confirmed to the caller.

**Test Scenarios**:
1. Run `unimatrix --help` and `unimatrix serve --help`; assert `--daemon-child` does not appear in either help output.
2. Assert `#[arg(hide = true)]` attribute is present in source on the `daemon_child` field.

**Coverage Requirement**: Help output snapshot test.

---

### R-18: TTL eviction races with drain_for
**Severity**: Med
**Likelihood**: Low
**Impact**: Background tick evicts a bucket at the same instant `context_retrospective` calls `drain_for`; one of the callers gets an empty result when it should have had entries, or a panic from a concurrent HashMap mutation.

**Test Scenarios**:
1. Unit test: acquire the Mutex in a test; confirm both `evict_stale` and `drain_for` can only be called while holding the lock; assert no other eviction path bypasses the Mutex.
2. Simulate concurrent `evict_stale` and `drain_for` on the same bucket under the test Mutex harness; assert no entries are double-counted or lost.

**Coverage Requirement**: Both eviction paths (TTL + drain) must hold the Mutex for the full duration of their operation.

---

## Integration Risks

**Bridge ↔ Daemon session lifecycle**: The bridge is a dumb byte copier. Any error in the bridge's `copy_bidirectional` loop (e.g., the daemon closes the stream due to hitting the session cap) must result in the bridge exiting with a non-zero code, not silently hanging. The bridge must propagate stream close from the daemon side to Claude Code's stdin/stdout.

**Session task ↔ CancellationToken**: The child token pattern (ADR-002) requires each session task to spawn an inner monitoring task that bridges `CancellationToken::cancelled()` to `rmcp::cancellation_token().cancel()`. If this inner task is omitted, SIGTERM does not propagate to session tasks, and the 30-second join timeout fires — causing a delayed daemon shutdown every time.

**Accumulator drain ↔ retrospective**: `context_retrospective` drains via `drain_for`, which removes the bucket. If the caller's MCP request handler panics after drain but before returning the result, the bucket is permanently lost. The drain-then-respond pattern has no rollback. This must be documented as an accepted trade-off.

**PidGuard ↔ SocketGuard drop ordering**: Shutdown drops PidGuard (removes PID file) before or after SocketGuard drops (removes socket files). If PID file is removed first, a concurrent bridge's stale-check returns false and it attempts to spawn a new daemon before the old one's sockets are cleaned up — interleaving two daemon startups. Drop order must be: sockets dropped first, PID file last.

**ServiceLayer construction ↔ Arc::try_unwrap**: ADR-003 explicitly warns that constructing a second `ServiceLayer` per session creates divergent `Arc<Store>` clones. The single-construction guarantee must be enforced at code review — no `ServiceLayer::new()` call inside the session task spawn closure.

---

## Edge Cases

**Bridge started with no home directory writable**: `ProjectPaths` computation fails before socket connect; bridge exits with a path error rather than attempting auto-start. Must produce a clear error.

**Daemon started as root vs. normal user**: Socket permissions `0600` owned by root. Another user's bridge cannot connect. This is correct behavior but should be documented.

**MCP `initialize` arrives before daemon completes socket bind**: The bridge polls for socket existence (250ms intervals, 5s timeout). A connection attempt between "file exists" and "listen() called" receives ECONNREFUSED. Bridge must retry connection, not treat ECONNREFUSED as a permanent failure during the poll window.

**SIGTERM during `context_store` with embedding in flight**: `spawn_blocking` for embedding is in flight when the cancellation token fires. The session task's `waiting()` returns via the child token. The `spawn_blocking` task may still be running in the blocking thread pool. `graceful_shutdown` must not abort the thread pool before blocking tasks complete — or accept that in-flight embeddings are dropped.

**`unimatrix stop` while graceful shutdown already in progress**: Bridge auto-start spawns a daemon while a previous daemon is in its 30-second session-join window. PidGuard `flock` prevents the new daemon from acquiring the lock — but `unimatrix stop` is the caller of `terminate_and_wait`, which polls the PID file. If PID file is removed by the exiting daemon before the new daemon writes it, `unimatrix stop` falsely reports success.

**Empty feature_cycle string as accumulator key**: `context_store` called without a `feature_cycle` value. `upsert("", ...)` creates a bucket with an empty string key. `drain_for("")` drains it. This is technically correct but produces confusing retrospective output. Spec does not prohibit it — should be a validated constraint.

**Socket path with unicode or special characters in home directory path**: `sun_path` is a C string; paths with non-UTF8 bytes fail. `ProjectPaths` uses `PathBuf::to_string_lossy()` for logging but `as_os_str()` for bind. Unicode-safe, but paths with null bytes fail with an OS error. Length validation (FR-20) should also check for null bytes.

---

## Security Risks

**Untrusted input surface**: The MCP JSON-RPC stream from a bridge connection is the primary untrusted input surface. While the bridge is owner-only (socket `0600`), the bridge process can be invoked by any code with filesystem access to the data directory. The daemon must treat all MCP input as potentially malformed.

**MCP JSON-RPC injection via feature_cycle field**: `feature_cycle` strings from `context_store` requests become HashMap keys stored in memory. A maliciously long string (e.g., 100MB key) triggers memory allocation but no SQL injection risk (in-memory only). Cap key length to a reasonable bound (e.g., 256 bytes) and return a validation error for oversized keys.

**Socket file as TOCTOU target**: Between `handle_stale_socket` (unlink) and `bind` (create), a malicious local process could place a symlink at the socket path, causing `bind` to bind to an attacker-controlled path. Mitigation: set restrictive permissions on the parent directory `~/.unimatrix/{hash}/` (already `0700` from vnc-004). No additional mitigation needed for dev-workspace scope.

**Blast radius of a compromised session task**: Each session task runs with full `UnimatrixServer` access — all 12 tools including `context_store`, `context_correct`, and `context_deprecate`. A compromised session could corrupt knowledge base entries. Blast radius is limited to the Unimatrix knowledge base for the current project. No filesystem access beyond the data directory; no network calls from tool handlers. This is acceptable for local dev scope.

**Rate-limit exemption boundary (C-07)**: `CallerId::UdsSession` is exempt from rate limiting because UDS is filesystem-gated. If this exemption is not explicitly documented in code with a reference to C-07 / W2-2, a future developer adding HTTP transport will silently inherit the exemption. This is R-07 above — security materialization is delayed but real.

**`--daemon-child` flag as privilege escalation vector**: Not a meaningful vector — the flag causes the process to call `setsid()` and initialize a server, both of which require the invoking user to already have filesystem access to the data directory. No privilege escalation beyond what direct file access permits.

---

## Failure Modes

**Bridge timeout (auto-start fails)**: Bridge exits 1 with message including log file path. Claude Code session fails to start. User must check log file and restart manually. Daemon log should contain the initialization error that caused the socket to never appear.

**Daemon shutdown timeout (graceful_shutdown exceeds 30s)**: NFR-06 specifies daemon logs warning and exits anyway. DB compaction may be incomplete; next startup runs WAL checkpoint. Vector index may not be dumped — HNSW rebuilds from stored data on next start (degraded first-query latency).

**`Arc::try_unwrap(store)` failure at shutdown**: Daemon panics. DB compaction does not run. `unimatrix-mcp.sock` and PID file may remain (depending on panic hook). Next auto-start must handle both stale files. This is the most severe failure mode — tracked as R-01.

**Session task panic**: An unhandled panic in a session task propagates through `tokio::spawn`; the handle's `join().await` returns `Err(JoinError)`. The accept loop must not propagate this error to the daemon token — one session panic must not kill other sessions. The accept loop should log the error and continue.

**Hook IPC socket accept error**: An error on `unimatrix.sock` accept should not propagate to the daemon's main error channel. Hook IPC errors are already self-contained (each hook process is a short-lived client). No change needed.

**`unimatrix stop` when daemon is mid-tick**: SIGTERM arrives during the 15-minute tick maintenance cycle. The tick loop must check the cancellation token between tick phases. If the tick holds the `Mutex<Connection>` when SIGTERM fires, the session join timeout may elapse before the tick releases the lock. Implementation must ensure tick respects cancellation between DB operations.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (transport-async-rw untested in codebase) | — | Resolved in architecture: ADR-003 confirms `UnixStream` wrapping via `transport-async-rw`; `#[derive(Clone)]` already exists. No architecture-level risk remains; implementation must prototype before full rollout. |
| SR-02 (tokio+fork UB) | — | Fully resolved by ADR-001: spawn-new-process pattern; no fork after runtime init. R-04 captures the residual macOS stale-PID edge case. |
| SR-03 (5s auto-start timeout) | R-08 | Architecture specifies 250ms polling intervals, log-path in error message. R-08 tests timeout failure message completeness. |
| SR-04 (default invocation breaking change) | R-12, R-13 | R-12 tests `serve --stdio` regression; R-13 tests hook path Tokio init regression. |
| SR-05 (accumulator eviction policy undefined) | R-05, R-15, R-18 | ADR-004 defines three eviction triggers and TTL. R-05 covers concurrent drain/upsert; R-15 covers cap enforcement; R-18 covers TTL race. |
| SR-06 (server clone + shutdown decoupling as coordinated refactor) | R-01, R-03 | ADR-002 and ADR-003 treat these as a single coordinated change. R-01 tests Arc::try_unwrap post-join; R-03 tests session EOF does not trigger shutdown. |
| SR-07 (graceful_shutdown decoupling) | R-01, R-02, R-03 | ADR-002: CancellationToken model; single shutdown call site. R-01/R-02/R-03 cover the three failure modes of incorrect decoupling. |
| SR-08 (stale MCP socket handling) | R-09, R-16 | Architecture extends `handle_stale_socket` to `unimatrix-mcp.sock`. R-09 tests stale socket unlink at startup; R-16 tests socket cleanup at shutdown. |
| SR-09 (concurrent session safety, MAX_CONCURRENT_SESSIONS=32) | R-02, R-05, R-10, R-11 | ADR-005: counter-based cap. R-02 covers spawn race; R-05 covers accumulator concurrency; R-10 covers Vec growth; R-11 covers cap enforcement. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 5 (R-01, R-02, R-03, R-04, R-12) | 14 scenarios minimum |
| High | 7 (R-05, R-06, R-07, R-08, R-09, R-10, R-11) | 18 scenarios minimum |
| Med | 5 (R-13, R-14, R-15, R-16, R-18) | 10 scenarios minimum |
| Low | 1 (R-17) | 2 scenarios minimum |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection daemon UDS socket" — found #245 (unconditional unlink pattern), #211 (vnc-004 outcome), #300 (UDS capability boundary). All directly applicable.
- Queried: `/uni-knowledge-search` for "outcome rework graceful shutdown Arc try_unwrap" — found #81/#33 (ADR-005: shutdown via Arc::try_unwrap), #312 (bugfix #92 regression from same invariant). Elevated R-01 to Critical based on #312 historical failure.
- Queried: `/uni-knowledge-search` for risk patterns on concurrent session/cancellation — found #1367 (spawn_blocking timeout pattern), #731 (batched DB writes), #735 (spawn_blocking pool saturation). Used to inform R-05 severity and R-13 evidence.
- Queried: `/uni-knowledge-search` for SQLite concurrent access — found #735 (spawn_blocking pool saturation from unbatched writes) and #328 (Mutex<Connection>). Confirmed SQLite risk is already serialized; no new risk.
- Stored: nothing novel to store — patterns observed are codebase-specific to vnc-005 design decisions, not cross-feature generalizations visible across 2+ features at this time.
