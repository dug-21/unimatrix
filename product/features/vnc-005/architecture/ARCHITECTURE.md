# vnc-005: Daemon Mode Architecture

## System Overview

vnc-005 transforms Unimatrix from a per-session stdio process into a persistent background
daemon. The binary's default invocation becomes a thin stdio-to-UDS bridge that connects
Claude Code's stdio pipe to a running daemon over a Unix Domain Socket. The daemon runs
the full server stack — store, vector index, embedding service, background tick — and
survives MCP client disconnection.

This feature is the prerequisite for all Wave 1 intelligence features: the background tick
loop, confidence refresh, co-access cleanup, and adaptation state all require a process
that outlives a single session. Without daemon mode, those systems initialize and
immediately tear down.

### Binary Modes After vnc-005

| Invocation | Mode | Who uses it |
|---|---|---|
| `unimatrix` (no subcommand) | Bridge — connects to daemon, auto-starts if absent | `.mcp.json` / Claude Code |
| `unimatrix serve --daemon` | Daemon — detaches, binds MCP UDS, runs forever | Auto-start from bridge; manual ops |
| `unimatrix serve --stdio` | Stdio server — blocking stdio transport, no fork | Development, integration tests |
| `unimatrix stop` | Stop client — SIGTERM to daemon via PID file | Manual ops |
| `unimatrix hook <event>` | Hook client — sync UDS to hook IPC socket | Claude Code hooks (unchanged) |

The `.mcp.json` command stays `"unimatrix"` with no args. No change to external tooling.

---

## Component Breakdown

### Component 1: Daemonizer (`infra/daemon.rs`)

Responsibility: detach the process from the calling terminal and start a fresh Tokio
runtime in the child. This is the resolution to SR-02 (tokio+fork ordering).

The daemonizer runs entirely in synchronous Rust before any async runtime is started.
It calls `std::process::Command::new(current_exe)` to spawn a new child process with
`--daemon-child` as an internal flag. The launcher then exits. The child starts Tokio
fresh with no inherited runtime state.

This is the "spawn-new-process" pattern: no `fork(2)` call after Tokio is running.
No UB. No inherited thread pools. See ADR-001.

Key behaviors:
- Launcher redirects child stdout/stderr to the log file before spawning.
- Child opens `/dev/null` as stdin.
- Child uses `nix::unistd::setsid()` to create a new session (detaches from terminal).
  `setsid` is called synchronously before Tokio starts.
- Launcher writes nothing to the PID file — the child's `PidGuard::acquire` does that.
- Launcher polls the MCP socket path for up to 5 seconds, exits 0 when socket appears.

### Component 2: MCP Session Acceptor (`uds/mcp_listener.rs`)

Responsibility: bind the MCP UDS socket, accept concurrent client connections, and
spawn a per-session task for each one.

```
~/.unimatrix/{hash}/unimatrix-mcp.sock   (new, 0600)
~/.unimatrix/{hash}/unimatrix.sock       (existing hook IPC — unchanged)
```

The acceptor runs as a tokio task. It holds an `Arc<UnimatrixServer>` that it clones
into each session task. Each session task calls `server.serve(stream_transport)` on the
`UnixStream` wrapped as an rmcp `transport-async-rw` transport. When the client
disconnects, only that task exits. The daemon continues.

MCP socket cleanup follows the existing pattern from ADR-004: unconditional unlink at
daemon startup, `SocketGuard` RAII on bind.

### Component 3: UnimatrixServer Clone Model (`server.rs` refactor)

`UnimatrixServer` already derives `Clone` (confirmed in source: `#[derive(Clone)]` at
line 95 of server.rs). All fields are `Arc`-wrapped. Clone is a cheap reference copy.

No semantic change is needed. The server is constructed once in the daemon startup path
and cloned into each session task. All state mutations go through the existing
`Arc<Mutex<_>>` and `Arc<RwLock<_>>` internals.

One structural change: `UnimatrixServer::new` currently creates its own `ServiceLayer`
internally. The daemon startup path must pass the externally-constructed `ServiceLayer`
into the server (or extract the `ServiceLayer` after construction as today) so that
`LifecycleHandles` gets the same `ServiceLayer` that the server holds. This prevents
the `Arc::try_unwrap(store)` failure on shutdown (the existing vnc-006/#92 fix pattern
must be preserved).

### Component 4: Shutdown Signal Router (`infra/shutdown.rs` extension)

Responsibility: decouple session-end from daemon-end. This is the resolution to SR-07.

The daemon startup spawns a `tokio_util::sync::CancellationToken` (the "daemon token").
A signal handler task watches SIGTERM/SIGINT and cancels this token when triggered.
The MCP acceptor loop selects on this token — when cancelled, it stops accepting new
connections and notifies all active session tasks.

Session tasks do NOT hold a reference to the daemon token. Session end is purely the
rmcp `waiting().await` call returning `QuitReason::Closed`. When a session task exits
it drops its cloned `UnimatrixServer` — which drops no owned resources (all fields are
`Arc`). `graceful_shutdown` is called exactly once, after the daemon token fires and all
session tasks have been joined.

The `LifecycleHandles` struct gains a new `mcp_socket_guard: Option<SocketGuard>` field
for the MCP socket, alongside the existing `socket_guard` for the hook IPC socket.

### Component 5: Feature-Cycle Accumulator (`server.rs` — `PendingEntriesAnalysis`)

Responsibility: accumulate `EntryAnalysis` across multiple sessions keyed by
`feature_cycle`.

The current `PendingEntriesAnalysis` is a single `HashMap<u64, EntryAnalysis>` (keyed
by entry ID). For daemon mode it becomes a two-level structure:

```rust
pub struct PendingEntriesAnalysis {
    // Outer key: feature_cycle string (e.g. "vnc-005")
    // Inner key: entry_id u64
    pub buckets: HashMap<String, HashMap<u64, EntryAnalysis>>,
    pub created_at: u64,
}
```

Protected by the existing `Arc<Mutex<PendingEntriesAnalysis>>`. Mutex is correct here
(not RwLock) because writes and reads are both short-duration and writes are not rare
— every hook stop event writes. RwLock would save nothing and add upgrade complexity.
See ADR-004.

Eviction: a bucket is eligible for eviction when `context_cycle` is called for that
`feature_cycle` (the cycle tool is the authoritative "this feature is done" signal).
Additionally, the background tick can evict buckets older than 72 hours with zero
entries. See ADR-004 for eviction policy.

### Component 6: Bridge Client (`bridge.rs` or `main.rs` no-subcommand path)

Responsibility: connect stdio to the daemon's MCP socket, bidirectionally copy bytes.

The bridge is the new default no-subcommand path. It:
1. Reads `paths.mcp_socket_path` from `ProjectPaths`.
2. Attempts `tokio::net::UnixStream::connect`. If ok → bridge.
3. If connect fails: reads PID file, calls `is_unimatrix_process(pid)` (stale check).
   If stale: spawns daemon via `std::process::Command::new(current_exe).arg("serve").arg("--daemon")`.
   Polls socket for up to 5 seconds (100ms intervals). If socket appears → bridge.
   If timeout: write error to stderr, exit 1.
4. Bridge loop: `tokio::io::copy_bidirectional(stdin, uds_stream)` until either side
   closes. Exit 0.

The bridge uses Tokio (async) because `copy_bidirectional` is async. The existing hook
path is untouched — `Command::Hook` is a separate match arm with no tokio runtime.

### Component 7: `unimatrix stop` Subcommand (`main.rs`)

Responsibility: send SIGTERM to the running daemon and confirm exit.

Implementation:
1. Read PID file via `pidfile::read_pid_file`.
2. If no PID file: print "no daemon running", exit 1.
3. Call `is_unimatrix_process(pid)`. If false: print "stale PID, no daemon", exit 1.
4. Call `pidfile::terminate_and_wait(pid, 10s)`.
5. If daemon exited: exit 0. If not: print "daemon did not stop within timeout", exit 2.

`terminate_and_wait` already exists in `pidfile.rs` and handles SIGKILL escalation.
No new logic needed beyond the subcommand wiring.

---

## Component Interactions

```
                   .mcp.json invocation
                         |
                         v
              ┌─────────────────────┐
              │   Bridge Client     │  (Component 6)
              │  (stdio ↔ UDS copy) │
              └────────┬────────────┘
                       │  UnixStream connect
                       v
              ┌─────────────────────┐        SIGTERM/SIGINT
              │  MCP Acceptor       │◄──────────────────────
              │  (Component 2)      │            │
              │  unimatrix-mcp.sock │      Signal Handler
              └────────┬────────────┘       (shutdown token)
                       │ spawn session task (per connection)
                       v
              ┌─────────────────────┐
              │  Session Task       │
              │  server.clone()     │
              │  .serve(stream)     │
              └────────┬────────────┘
                       │ all state via Arc
                       v
              ┌─────────────────────────────────────────────┐
              │         UnimatrixServer (shared)             │
              │  Arc<Store> + Arc<VectorIndex>               │
              │  Arc<Mutex<PendingEntriesAnalysis>>          │  (Component 5)
              │  ServiceLayer (Arc<...> internals)           │
              │  Arc<SessionRegistry>                        │
              └──────────────────────┬──────────────────────┘
                                     │
              ┌──────────────────────┴──────────────────────┐
              │        Background Tick (Component 4)         │
              │  15-minute interval                          │
              │  Runs for daemon lifetime                    │
              └─────────────────────────────────────────────┘

              ┌─────────────────────┐
              │  Hook IPC Socket    │  (unchanged)
              │  unimatrix.sock     │
              └─────────────────────┘
```

### Session Task Lifecycle

```
accept() → clone(server) → spawn task {
    running = server.serve(uds_transport).await
    // signal handler NOT connected to session tasks
    running.waiting().await  // blocks until client disconnects
    // task exits, Arc<UnimatrixServer> clone drops
    // no graceful_shutdown here — daemon still running
}
```

### Daemon Shutdown Sequence

```
SIGTERM/SIGINT
  → signal_task cancels daemon_token
  → acceptor loop breaks
  → all active session tasks: cancellation_token().cancel() [optional notification]
  → join all session task handles (with timeout)
  → graceful_shutdown(lifecycle_handles)
      → abort uds_handle (hook IPC)
      → drop mcp_socket_guard (new)
      → drop socket_guard (existing hook socket)
      → abort tick_handle
      → vector dump
      → adapt save
      → drop ServiceLayer, registry, audit, vector_index
      → Arc::try_unwrap(store) → compact
  → PidGuard drops → PID file removed
  → process exits
```

---

## Technology Decisions

See individual ADR files. Summary:

| Decision | Choice | ADR |
|---|---|---|
| Daemonization pattern | spawn-new-process via `std::process::Command` | ADR-001 |
| Session/daemon lifetime separation | per-session task, daemon CancellationToken | ADR-002 |
| UnimatrixServer sharing model | Clone (already derives Clone) + Arc internals | ADR-003 |
| Accumulator key structure | `HashMap<feature_cycle, HashMap<entry_id, _>>` + Mutex | ADR-004 |
| MCP accept task topology | single acceptor task + per-connection spawned tasks | ADR-005 |
| `unimatrix stop` signal delivery | SIGTERM via existing `terminate_and_wait` | ADR-006 |

---

## Integration Points

### Existing components touched

| Component | Change | Risk |
|---|---|---|
| `main.rs` | Add `serve` + `stop` subcommands; change no-subcommand to bridge | SR-04: behavioral change |
| `infra/shutdown.rs` | Add `mcp_socket_guard` to `LifecycleHandles`; session-end no longer calls shutdown | SR-07: critical refactor |
| `server.rs` `PendingEntriesAnalysis` | Add outer `feature_cycle` key | SR-05: eviction policy |
| `unimatrix-engine/src/project.rs` `ProjectPaths` | Add `mcp_socket_path` field | Additive |
| `uds/listener.rs` | `handle_stale_socket` reused for MCP socket | Additive |

### New modules

| Module | Location | Purpose |
|---|---|---|
| `infra/daemon.rs` | `unimatrix-server` crate | Spawn-new-process daemonization |
| `uds/mcp_listener.rs` | `unimatrix-server` crate | MCP UDS accept loop |
| `bridge.rs` | `unimatrix-server` crate (or inline in `main.rs`) | stdio↔UDS copy |

---

## Integration Surface

| Integration Point | Type / Signature | Source |
|---|---|---|
| `ProjectPaths::mcp_socket_path` | `PathBuf` — new field, value `data_dir.join("unimatrix-mcp.sock")` | `unimatrix-engine/src/project.rs` |
| `LifecycleHandles::mcp_socket_guard` | `Option<SocketGuard>` — new field | `infra/shutdown.rs` |
| `LifecycleHandles::mcp_acceptor_handle` | `Option<tokio::task::JoinHandle<()>>` — new field | `infra/shutdown.rs` |
| `LifecycleHandles::session_task_handles` | `Vec<tokio::task::JoinHandle<()>>` — new field | `infra/shutdown.rs` |
| `PendingEntriesAnalysis::buckets` | `HashMap<String, HashMap<u64, EntryAnalysis>>` | `server.rs` |
| `PendingEntriesAnalysis::upsert(feature_cycle, analysis)` | `fn upsert(&mut self, feature_cycle: &str, analysis: EntryAnalysis)` | `server.rs` |
| `PendingEntriesAnalysis::drain_for(feature_cycle)` | `fn drain_for(&mut self, feature_cycle: &str) -> Vec<EntryAnalysis>` | `server.rs` |
| `start_mcp_uds_listener(path, server, shutdown_token)` | `async fn(...) -> Result<(JoinHandle<()>, SocketGuard), ServerError>` | `uds/mcp_listener.rs` |
| `run_bridge(mcp_socket_path)` | `async fn(path: &Path) -> Result<(), ServerError>` | `bridge.rs` |
| `Cli::Command::Serve { daemon: bool, stdio: bool }` | New clap subcommand variant | `main.rs` |
| `Cli::Command::Stop` | New clap subcommand variant | `main.rs` |
| `--daemon-child` internal flag | `bool` clap arg, hidden, used only by daemon self-spawn | `main.rs` |

### rmcp transport surface

`UnixStream` implements `AsyncRead + AsyncWrite`. The `transport-async-rw` feature of
rmcp (already activated by the `server` feature) provides `IntoTransport` for any
`(AsyncRead, AsyncWrite)` pair. Wrapping is:

```rust
use rmcp::transport::io::duplex;
let transport = duplex(read_half, write_half);
server.clone().serve(transport).await
```

No new Cargo.toml changes needed. No new rmcp features. The `server` feature already
pulls in `transport-async-rw`.

---

## Security

- MCP socket (`unimatrix-mcp.sock`) created with `0600` permissions, same pattern as
  hook IPC socket (see `uds/listener.rs` line 186).
- Bridge path: no new capability escalation. The daemon enforces the same auth as today.
- Auto-start stale check: `is_unimatrix_process(pid)` before spawning a new daemon
  (existing function in `pidfile.rs`, no change).
- `--daemon-child` flag is hidden in `clap` help output. It is not a security boundary
  (any user with filesystem access can call it directly), but it is not a new attack
  surface — the binary already requires filesystem access to the data directory.

---

## Socket Path Length Budget

UDS paths are limited to 104 bytes (macOS) / 108 bytes (Linux). The longest expected
path:

```
/home/username/.unimatrix/1234567890abcdef/unimatrix-mcp.sock
^---- 5 ----^^--- 14 ---^^-- 16 hex chars-^^---- 20 ----------^
= ~56 bytes on a typical system (55-70 bytes with longer usernames)
```

This fits within 104 bytes for usernames up to ~33 characters. Very long home paths
(e.g., NFS-mounted `/home/department/team/username/...`) may approach the limit.
Implementation note: validate path length at socket bind time and fail fast with a
clear error if it exceeds 103 bytes (one byte margin).

---

## Open Questions

1. **Connection cap (SR-09)**: The architecture does not specify a maximum number of
   concurrent MCP sessions. Unbounded task spawning is safe for typical dev workspace
   usage (1-2 Claude Code windows) but could be capped at 16 or 32 without meaningful
   constraint. Recommend documenting as "unbounded, revisit if reconnect storms reported."

2. **Bridge timeout behavior (SR-03)**: When the 5-second auto-start wait expires, the
   bridge should print a clear error to stderr and exit 1. The exact error message and
   whether to fall back to stdio mode is a UX decision left to the implementor.

3. **Session task cancellation notification**: When the daemon receives SIGTERM, should
   in-flight session tasks be allowed to finish their current tool call, or cancelled
   immediately? The architecture above does not cancel active session tasks — they drain
   naturally as clients disconnect. This is conservative and correct for short tool calls
   but adds latency to daemon shutdown if a long operation (e.g., `context_store` with
   embedding) is in flight. A 30-second join timeout on session handles is a reasonable
   implementation-time decision.

4. **Log rotation**: The daemon logs to `~/.unimatrix/{hash}/unimatrix.log`. No rotation
   is specified for this feature. The file grows indefinitely until a future
   `nan-XXX` logging feature addresses it.
