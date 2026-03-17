# vnc-005 Implementation Brief — Daemon Mode: Persistent Background Server via UDS MCP Transport

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-005/SCOPE.md |
| Architecture | product/features/vnc-005/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-005/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-005/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-005/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|------------|-----------|
| Daemonizer (`infra/daemon.rs` + `project.rs`) | pseudocode/daemonizer.md | test-plan/daemonizer.md |
| MCP Session Acceptor (`uds/mcp_listener.rs`) | pseudocode/mcp_listener.md | test-plan/mcp_listener.md |
| Server Refactor (`server.rs` — UnimatrixServer Clone + PendingEntriesAnalysis) | pseudocode/server_refactor.md | test-plan/server_refactor.md |
| Shutdown Signal Router (`infra/shutdown.rs`) | pseudocode/shutdown.md | test-plan/shutdown.md |
| Bridge Client (`bridge.rs`) | pseudocode/bridge.md | test-plan/bridge.md |
| Stop Subcommand + CLI routing (`main.rs`) | pseudocode/stop_cmd.md | test-plan/stop_cmd.md |

Note: UnimatrixServer Clone Refactor and Feature-Cycle Accumulator are combined into `server_refactor` — both modify `server.rs` and are implemented by one agent to avoid merge conflicts.

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | product/features/vnc-005/pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | product/features/vnc-005/test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Transform Unimatrix from a per-session stdio process into a long-lived background daemon that accepts multiple MCP connections over a Unix Domain Socket, ensuring that the background tick loop, confidence refresh, vector index, and all in-memory caches survive MCP client disconnection between Claude Code sessions. A thin stdio-to-UDS bridge process replaces the current single-shot stdio server as the default binary invocation, keeping the existing `.mcp.json` configuration unchanged while routing each session to the persistent daemon.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Daemonization pattern | Spawn-new-process via `std::process::Command::new(current_exe())` with hidden `--daemon-child` flag; no `fork()` after Tokio init; child calls `setsid()` then initializes runtime fresh | ARCH Component 1; SCOPE SR-02 | architecture/ADR-001-daemonization-pattern.md |
| Session vs. daemon lifetime | Per-session Tokio task; daemon `CancellationToken` propagated to sessions; `graceful_shutdown` called exactly once after all session handles joined with 30s timeout | ARCH Component 4; SCOPE SR-07 | architecture/ADR-002-session-daemon-lifetime-separation.md |
| UnimatrixServer sharing model | Clone directly (`#[derive(Clone)]` already present; all fields `Arc`-wrapped); server constructed once, cloned into each session task; never construct a new `ServiceLayer` per session | ARCH Component 3; SCOPE SR-06 | architecture/ADR-003-unimatrixserver-sharing-model.md |
| Accumulator key structure | Two-level `HashMap<String, HashMap<u64, EntryAnalysis>>` (outer key: `feature_cycle`, inner key: `entry_id`); protected by existing `Arc<Mutex<>>` | ARCH Component 5; SCOPE OQ-05 | architecture/ADR-004-feature-cycle-accumulator.md |
| MCP accept loop topology | Single acceptor task + per-connection spawned tasks; `MAX_CONCURRENT_SESSIONS = 32` counter-based cap; periodic `retain(is_finished)` sweep of handle Vec | ARCH Component 2; SCOPE SR-09 | architecture/ADR-005-mcp-accept-loop-topology.md |
| `unimatrix stop` subcommand | Synchronous (no Tokio); reads PID file, calls `is_unimatrix_process`, calls `terminate_and_wait(pid, 15s)`; exit codes: 0 stopped, 1 no daemon/stale, 2 timeout | ARCH Component 7; SCOPE OQ-06 | architecture/ADR-006-unimatrix-stop-subcommand.md |
| Socket design (two sockets) | Hook IPC stays on `unimatrix.sock`; MCP sessions use new `unimatrix-mcp.sock`; no multiplexer; W2-2 promotes MCP socket to HTTP | SCOPE OQ-01 | — |
| Default invocation | `unimatrix` (no subcommand) becomes bridge mode; explicit stdio requires `unimatrix serve --stdio`; `.mcp.json` unchanged | SCOPE OQ-02 | — |
| Accumulator inner type (WARN-01 resolved) | **Architecture's two-level form is authoritative**: `HashMap<String, HashMap<u64, EntryAnalysis>>`. SCOPE.md OQ-05 and SPECIFICATION.md §Domain Models both say `Vec<EntryRecord>` but ARCHITECTURE.md Component 5 uses the two-level map with entry ID as inner key (preserving upsert semantics). The two-level form is the correct design; the specification's domain model description was imprecise. All implementation and tests must use `HashMap<u64, EntryAnalysis>` as the inner type. | ALIGNMENT-REPORT WARN-01; ARCH Component 5 | architecture/ADR-004-feature-cycle-accumulator.md |

---

## Files to Create / Modify

### New Files

| Path | Purpose |
|------|---------|
| `crates/unimatrix-server/src/infra/daemon.rs` | Spawn-new-process daemonizer: launcher polls MCP socket, child calls `setsid()` and enters server startup |
| `crates/unimatrix-server/src/uds/mcp_listener.rs` | MCP UDS accept loop: binds `unimatrix-mcp.sock` (0600), accepts connections, spawns per-session tasks |
| `crates/unimatrix-server/src/bridge.rs` | stdio-to-UDS bridge: connects stdin/stdout to daemon's MCP socket; auto-start logic |

### Modified Files

| Path | Change |
|------|--------|
| `crates/unimatrix-server/src/main.rs` | Add `Serve { daemon: bool, stdio: bool }` and `Stop` subcommands; change no-subcommand path to bridge dispatch; add hidden `--daemon-child` arg; preserve hook path before any async code |
| `crates/unimatrix-server/src/infra/shutdown.rs` | Add `mcp_socket_guard: Option<SocketGuard>`, `mcp_acceptor_handle: Option<JoinHandle<()>>` to `LifecycleHandles`; decouple `graceful_shutdown` from transport close |
| `crates/unimatrix-server/src/server.rs` | Refactor `PendingEntriesAnalysis` to two-level structure; add `upsert`, `drain_for`, `evict_stale` methods; add `FeatureBucket` type |
| `crates/unimatrix-engine/src/project.rs` | Add `mcp_socket_path: PathBuf` field to `ProjectPaths`; value `data_dir.join("unimatrix-mcp.sock")` |
| `crates/unimatrix-server/src/uds/listener.rs` | Reuse `handle_stale_socket` for `unimatrix-mcp.sock`; no protocol change to existing hook IPC socket |

---

## Data Structures

### `PendingEntriesAnalysis` (refactored — WARN-01 resolved: architecture form is authoritative)

```rust
pub struct PendingEntriesAnalysis {
    /// Outer key: feature_cycle string (e.g., "vnc-005").
    /// Inner key: entry_id u64.
    pub buckets: HashMap<String, FeatureBucket>,
    pub created_at: u64,
}

pub struct FeatureBucket {
    pub entries: HashMap<u64, EntryAnalysis>,
    pub last_updated: u64,  // unix seconds — TTL eviction reference
}
```

### `LifecycleHandles` additions

```rust
// New fields added to existing struct:
pub mcp_socket_guard: Option<SocketGuard>,
pub mcp_acceptor_handle: Option<tokio::task::JoinHandle<()>>,
```

### `Cli` additions (clap)

```rust
// New subcommand variant:
Serve { daemon: bool, stdio: bool }
Stop

// New hidden flag:
#[arg(long, hide = true)]
daemon_child: bool
```

### `ProjectPaths` addition

```rust
// New field:
pub mcp_socket_path: PathBuf,  // data_dir.join("unimatrix-mcp.sock")
```

---

## Function Signatures

### `infra/daemon.rs`

```rust
/// Launcher path: spawns daemon child, polls MCP socket path, exits 0 when socket appears.
/// Called when `serve --daemon` is invoked WITHOUT `--daemon-child`.
pub fn run_daemon_launcher(paths: &ProjectPaths) -> Result<(), ServerError>;

/// Child path: called when `--daemon-child` flag is present.
/// Calls setsid() synchronously, then returns to let tokio_main proceed.
pub fn prepare_daemon_child() -> Result<(), ServerError>;
```

### `uds/mcp_listener.rs`

```rust
/// Bind the MCP UDS socket (0600), start the accept loop task.
/// Returns the JoinHandle for the acceptor task and the SocketGuard for cleanup.
pub async fn start_mcp_uds_listener(
    path: &Path,
    server: UnimatrixServer,
    shutdown_token: CancellationToken,
) -> Result<(tokio::task::JoinHandle<()>, SocketGuard), ServerError>;
```

### `bridge.rs`

```rust
/// Default no-subcommand path: connect to daemon MCP socket or auto-start daemon, then bridge.
pub async fn run_bridge(mcp_socket_path: &Path, log_path: &Path) -> Result<(), ServerError>;
```

### `PendingEntriesAnalysis` methods

```rust
pub fn upsert(&mut self, feature_cycle: &str, analysis: EntryAnalysis);
pub fn drain_for(&mut self, feature_cycle: &str) -> Vec<EntryAnalysis>;
pub fn evict_stale(&mut self, now_unix_secs: u64, ttl_secs: u64);
```

### `stop` subcommand (`main.rs` dispatch)

```rust
/// Synchronous; no Tokio runtime.
fn run_stop(paths: &ProjectPaths) -> i32;  // returns exit code
```

---

## Constraints

1. **Fork-before-runtime (C-01 / SR-02)**: `nix::unistd::setsid()` must be called before any Tokio runtime is initialized in the child process. No `fork()` call is made anywhere in the codebase — the spawn-new-process pattern (ADR-001) is used exclusively.
2. **rmcp pinned at `=0.16.0` (C-02)**: No version change. `transport-async-rw` is already activated by the `server` feature. UDS transport uses `rmcp::transport::io::duplex(read_half, write_half)`.
3. **`#![forbid(unsafe_code)]` on all crates (C-03)**: `nix` crate wrappers are used for `setsid()`. No `SO_PEERCRED` access (tracked separately).
4. **Server clone + shutdown decoupling are a joint gate requirement (C-04)**: Partial implementation of one without the other is a gate failure. They are reviewed together.
5. **Enumerate all `graceful_shutdown` call sites before implementation (C-05)**: There is exactly one call site in the current codebase (`main.rs` line 405). After refactor there must still be exactly one, reached only from the daemon token cancellation path.
6. **No capability escalation in bridge (C-06)**: Bridge carries no Unimatrix capabilities. Auth enforced by daemon's per-session handler.
7. **`CallerId::UdsSession` exemption boundary (C-07)**: The rate-limit exemption site must carry a code comment referencing C-07 and W2-2. HTTP transport callers must not inherit this exemption.
8. **Socket path length (C-08 / FR-20)**: Validate full absolute path of `unimatrix-mcp.sock` at startup; fail fast with clear error if path exceeds 103 bytes (one byte margin below the macOS 104-byte `sun_path` limit).
9. **SQLite single-writer (C-09)**: `Mutex<Connection>` already serializes all writes; no additional locking needed for multi-session concurrency.
10. **Hook subcommand remains synchronous (C-10)**: No Tokio runtime initialized before `Command::Hook` dispatch in `main.rs`.
11. **Two-socket design is final (C-13)**: No discriminator or multiplexer. `unimatrix-mcp.sock` and `unimatrix.sock` are permanently separate.
12. **Session handle Vec sweep**: `retain(|h| !h.is_finished())` must run on every accept loop iteration, not only at shutdown (R-10).
13. **`Arc::try_unwrap(store)` invariant**: All session task clones of `UnimatrixServer` must be dropped (handles joined) before `graceful_shutdown` calls `Arc::try_unwrap(store)` (R-01). Drop ordering: SocketGuards dropped before PidGuard.
14. **`ServiceLayer` constructed once**: Never construct a new `ServiceLayer` inside the session task spawn closure. Divergent `Arc<Store>` clones break `Arc::try_unwrap` (ADR-003 consequence).
15. **`MAX_CONCURRENT_SESSIONS = 32`**: Counter-based cap; connections beyond the cap are accepted from the OS queue then immediately dropped with a daemon-level `warn!` log (R-11).
16. **`feature_cycle` key length cap**: Cap `feature_cycle` keys at 256 bytes; return a validation error for oversized keys (Security Risks section, RISK-TEST-STRATEGY.md).

---

## Dependencies

### Existing Crates (No Version Change)

| Crate | Usage |
|-------|-------|
| `nix` (already in Cargo.toml) | `unistd::setsid()` for terminal detachment in daemon child |
| `rmcp = "=0.16.0"` features: `server`, `transport-io`, `macros` | `transport-async-rw` already active via `server` feature; `duplex(read, write)` for UDS transport |
| `tokio` | `UnixListener`, `UnixStream`, `copy_bidirectional`, `CancellationToken` (tokio-util), `JoinHandle` |
| `fs2` | Existing `flock` in `PidGuard` — unchanged |
| `nix::sys::signal` | `kill(pid, SIGTERM)` in `unimatrix stop` path |

### Existing Internal Components (Reused)

| Component | Change |
|-----------|--------|
| `PidGuard` (vnc-004) | Reused unchanged; one-daemon-per-project enforcement |
| `is_unimatrix_process(pid)` (vnc-004) | Reused unchanged; stale PID detection in bridge auto-start and stop subcommand |
| `handle_stale_socket` (vnc-004 pattern) | Extended: applied to `unimatrix-mcp.sock` at daemon startup in addition to `unimatrix.sock` |
| `SocketGuard` | Extended: daemon now holds two `SocketGuard` instances (one per socket); both must drop before `graceful_shutdown` returns |
| `terminate_and_wait(pid, timeout)` (vnc-004) | Reused in `unimatrix stop`; timeout parameter set to 15 seconds |
| Background tick loop (`background.rs`) | Runs for daemon lifetime; no change to tick logic or interval |
| `ServiceLayer`, `SessionRegistry`, `ConfidenceStateHandle`, `SupersessionStateHandle`, `AgentRegistry`, `AuditLog` | Accessed via existing `Arc`-wrapped references; no structural change |

---

## NOT in Scope

- HTTP transport (W2-2 product roadmap item; `unimatrix-mcp.sock` is the future HTTP surface)
- TLS, OAuth, or any network authentication
- Windows UDS support (`serve --daemon` and bridge mode exit non-zero on Windows with a clear message; `serve --stdio` continues to work)
- Multi-project daemon (one daemon per project hash)
- systemd, Launchd, or container service integration (Wave 2)
- `SO_PEERCRED`-based MCP session identity (tracked separately)
- Log file rotation (NFR-07; dev-workspace scope accepts manual log management)
- rmcp version upgrade (`=0.16.0` pinned)
- Hook IPC socket or `HookRequest`/`HookResponse` protocol changes
- `.mcp.json` changes (stays `"command": "unimatrix", "args": []`)
- Tick interval or tick logic changes

---

## Alignment Status

**Overall: PASS with one WARN (acknowledged and resolved below).**

Vision alignment: PASS — vnc-005 is the direct implementation of W0-0 from `PRODUCT-VISION.md`. All three W0-0 security requirements (0600 socket permissions, stale PID check, UdsSession exemption boundary) are covered. The feature correctly terminates at W0-0 scope.

Scope additions confirmed acceptable:
- FR-12 (session cap at 32): SR-09 resolution; correct architectural call
- FR-19 / C-07 (UdsSession exemption boundary as formal constraint): Conservative elevation; required code-comment gate; no human approval needed
- AC-13 through AC-20: All defensible expansions of SCOPE.md ACs

### WARN-01: Accumulator Inner Type — RESOLVED

**What**: SCOPE.md OQ-05 and SPECIFICATION.md §Domain Models describe `pending_entries_analysis` as `HashMap<feature_cycle, Vec<EntryRecord>>`. ARCHITECTURE.md Component 5 (ADR-004) defines it as `HashMap<String, HashMap<u64, EntryAnalysis>>` (two-level, preserving entry-ID upsert semantics).

**Resolution**: The **architecture's two-level form is authoritative** for this implementation. The SCOPE.md and specification domain-model descriptions were imprecise; the architecture struct definition is the correct design (avoids duplicate entry IDs across sessions, preserves rework-count merge semantics). All implementation and tests must use `HashMap<u64, EntryAnalysis>` as the inner type. The `FeatureBucket` wrapper struct (with `last_updated` timestamp) is the authoritative definition.

No variances requiring approval. No FAIL or VARIANCE classifications.

---

## Critical Implementation Notes

### Drop Ordering (RTS Integration Risks — not named in R-N register)

`LifecycleHandles` must drop in this order during `graceful_shutdown`:
1. `mcp_socket_guard` (drops `unimatrix-mcp.sock`)
2. `socket_guard` (drops `unimatrix.sock`)
3. All `Arc<Store>` holders (session clones, ServiceLayer, etc.)
4. `PidGuard` last (removes PID file)

If PidGuard drops before SocketGuard, a concurrent bridge's stale-check returns false and it attempts daemon spawn before old sockets are cleaned up.

### Graceful Shutdown Call Site

After refactor, there must be exactly one call to `graceful_shutdown` in the codebase, reachable only from the daemon token cancellation path. The `QuitReason::Closed` path (session EOF) must NOT call `graceful_shutdown`. Code review gate: grep for `graceful_shutdown` call sites after implementation.

### Stdio Regression Gate (R-12)

`unimatrix serve --stdio` must exit when stdin closes — identical to pre-vnc-005 behavior. This is the primary regression gate. The `QuitReason::Closed` → process exit path must remain intact for the stdio subcommand path while being removed from the daemon path.

### SR-01 Prototype Requirement

Before full implementation of the session acceptor, prototype `server.clone().serve(duplex(read, write))` with a real `UnixStream` in isolation to confirm `transport-async-rw` wrapping works as expected in this codebase. This is a low-effort validation but critical given rmcp is pinned at `=0.16.0` with no upgrade path.
