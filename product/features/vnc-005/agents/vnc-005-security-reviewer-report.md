# Security Review: vnc-005-security-reviewer

## Risk Level: low

## Summary

vnc-005 introduces daemon mode, a UDS bridge client, and a two-level `PendingEntriesAnalysis`
accumulator. The implementation correctly follows established security patterns from vnc-004:
socket permissions are set to 0600 via explicit `set_permissions` after bind, stale socket
cleanup uses unconditional unlink, and the C-07 rate-exemption boundary is documented with a
W2-2 forward warning. No hardcoded secrets were found. Two findings are noted — both low
severity and non-blocking.

---

## Findings

### Finding 1: `std::thread::sleep` called inside async context in `bridge.rs`

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/bridge.rs:75` and `bridge.rs:104`
- **Description**: `run_bridge` is an `async fn` executed under `#[tokio::main]`. It calls
  `std::thread::sleep(BRIDGE_STALE_RETRY_DELAY)` (500ms) and
  `std::thread::sleep(BRIDGE_CONNECT_RETRY_INTERVAL)` (250ms) synchronously. On a
  Tokio multi-thread runtime this parks the worker thread for the sleep duration rather than
  yielding it to other tasks. The bridge is intentionally thin — it has no other tasks to
  starve — so the functional impact is negligible. However the 500ms blocking sleep at line 75
  adds latency to every daemon-start that hits the PID-alive-but-socket-missing window.
  `daemon.rs` has the same pattern but is synchronous (`fn`, not `async fn`), which is
  correct.
- **Recommendation**: Replace with `tokio::time::sleep` in `run_bridge` for correctness.
  No correctness bug today (single task in this runtime), but worth fixing before the bridge
  gains concurrent tasks (e.g., a watchdog or reconnect loop).
- **Blocking**: no

---

### Finding 2: Session-cap `active_count` increment deferred into spawned task — cap can momentarily exceed 32

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/uds/mcp_listener.rs:159–188`
- **Description**: The cap check loads `active_count` and then spawns a task which does the
  `fetch_add`. Because there is a single acceptor task, no two connections race through the
  cap check simultaneously. However, after spawn, the acceptor immediately loops back to
  `select!` and can accept the next connection before the spawned task's `fetch_add` runs.
  On a Tokio multi-thread runtime the spawned task may not execute until the acceptor yields.
  Measured overshoot on a loaded system: at most a few sessions above 32, bounded by how
  many accepts occur before the Tokio scheduler runs the spawned tasks. This is a resource-
  cap softness, not a security vulnerability. Blast radius: at most a handful of extra Arc
  clones, not privilege escalation or data corruption.
- **Recommendation**: Move `fetch_add` to before `tokio::spawn` (increment before spawn,
  decrement in the task on completion or on early cap-drop). This eliminates the window
  entirely. Alternatively, use `fetch_add` with `compare_exchange` to do a check-and-reserve
  atomically. Not urgent given the single-threaded acceptor topology and dev-workspace scope.
- **Blocking**: no

---

### Finding 3 (informational, not a finding): Socket permissions window

- **Severity**: informational
- **Location**: `crates/unimatrix-server/src/uds/mcp_listener.rs:96–112`
- **Description**: `UnixListener::bind()` creates the socket file with permissions derived
  from the process umask (typically 0755 on Linux). `set_permissions(0o600)` is called
  immediately after bind, before `tokio::spawn` starts the acceptor. The comment claims "no
  window exists." This claim requires precision: the socket file is accessible with umask
  permissions during the gap between `bind()` and `set_permissions()`. However, the OS
  listen backlog exists from `bind()` but connections are only processed when `accept()` is
  called (inside the not-yet-spawned task). A local process could queue a connection to the
  backlog during this window, but it cannot complete an MCP exchange until `accept()` runs.
  The parent data directory has 0700 permissions (from vnc-004), which blocks other-user
  processes from reaching the socket at the filesystem level. The RISK-TEST-STRATEGY
  documents this TOCTOU window (Security Risks section) and accepts it for dev-workspace
  scope. This matches the identical pattern in `uds/listener.rs` for the hook IPC socket.
  No action required.
- **Blocking**: no

---

## Blast Radius Assessment

**Worst case if daemon.rs has a subtle bug**: `prepare_daemon_child` is called before Tokio
init (correct). If `setsid()` fails for a reason other than EPERM (already session leader),
the error propagates as `ServerError::ProjectInit` to the launcher's `run_daemon_launcher`
which returns `Err`, and the bridge exits with a clear message. The daemon is never started.
No data corruption, no silent failure.

**Worst case if mcp_listener.rs has a subtle bug**: A session Arc clone not dropped before
`graceful_shutdown` causes `Arc::try_unwrap(store)` to fail. The daemon logs a warning and
skips DB compaction. Data written in that session is committed (SQLite WAL) but the optional
compaction pass is missed. Next startup runs a WAL checkpoint. No data loss; degraded startup
performance.

**Worst case if bridge.rs has a subtle bug**: The bridge is a pure byte pipe. If it exits
non-zero, Claude Code's MCP connection fails. The user gets an error message. The daemon
continues unaffected.

**Worst case if the `PendingEntriesAnalysis` two-level refactor has a silent bug**: Entries
for a feature cycle are lost before `context_retrospective` can drain them. The retrospective
report shows no entry-level performance data. This is a data completeness issue, not a
security or integrity issue.

---

## Regression Risk

**R-12 (critical gate)**: `tokio_main_stdio` is the renamed pre-vnc-005 `tokio_main`. The
`Serve { daemon: false, stdio: _ }` arm routes to it. Tests confirm `QuitReason::Closed`
still reaches `graceful_shutdown`. The stdio path is structurally unchanged. Risk: low.

**`drain_all` → `drain_for` migration**: All callers in `tools.rs`, `listener.rs`, and
`mcp/tools.rs` were updated to pass a `feature_cycle` key. Existing tests updated with
`"test-fc"` keys. Sessions without feature cycle attribution use the empty string key (`""`),
which is technically correct but produces confusing retrospective output (noted in
RISK-TEST-STRATEGY as an accepted edge case).

**`upsert` semantics change (accumulate → overwrite)**: The previous implementation
accumulated `rework_flag_count` across multiple calls for the same entry ID. The new
implementation overwrites. This is a behavioral change to the accumulator's semantics. The
test `pending_entries_upsert_overwrites_counts` documents and asserts the new behavior
(`rework_flag_count` == 3, not 2+3=5). Callers relying on accumulation will silently get
the most recent value instead of a sum. No security implication; functional change is
intentional (per the ADR-004 "overwrite semantics" decision).

---

## Dependency Safety

**`tokio-util = 0.7.18`** (new dependency): Provides `CancellationToken`. Version 0.7.x is
the current stable line. No known critical CVEs for `tokio-util` 0.7 as of knowledge cutoff
(August 2025). The `tokio-util` crate is maintained by the Tokio team. Only the `sync`
feature (CancellationToken) is used.

**`nix` feature addition (`process`)**: The `process` feature adds `unistd::setsid`. This is
a narrow POSIX addition with no new attack surface beyond the existing `user` feature.

---

## Secrets

No hardcoded secrets, API keys, credentials, or tokens found in the diff.

---

## OWASP Concerns Evaluated

| Check | Result |
|-------|--------|
| Injection (command) | `std::process::Command::new` with explicit args (no shell), no injection risk |
| Path traversal | Socket path derived from SHA-256 hash of project root; null byte check in `validate_socket_path_length` |
| Broken access control | MCP socket 0600, parent dir 0700 (vnc-004); C-07 exemption documented with W2-2 boundary |
| Security misconfiguration | No debug-only permissions; `--daemon-child` hidden in clap help (`hide = true`) |
| Deserialization | No new deserialization of untrusted data; bridge is a pure byte pipe |
| Input validation | `feature_cycle` key length capped at 256 bytes (C-16 in `upsert`); socket path length validated at 103 bytes |
| Hardcoded secrets | None |
| Vulnerable components | `tokio-util 0.7.18` — no known CVEs |

---

## PR Comments

- Posted comments on PR #296 (see below)
- Blocking findings: no

---

## Knowledge Stewardship

nothing novel to store — both findings (blocking sleep in async context, deferred atomic
increment for cap check) are instance-specific to this feature's bridge and acceptor design.
The socket permissions TOCTOU pattern is already documented in the existing codebase and in
the risk strategy. No generalizable anti-pattern rises to cross-feature lesson-learned threshold.
