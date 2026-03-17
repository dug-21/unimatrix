# vnc-005 Pseudocode Overview

## Component Inventory

| Component | File | Crate | Wave |
|---|---|---|---|
| Daemonizer | `infra/daemon.rs` (new) | unimatrix-server | 1 |
| Shutdown Signal Router | `infra/shutdown.rs` (modify) | unimatrix-server | 1 |
| server.rs Refactor (Clone + Accumulator) | `server.rs` (modify) | unimatrix-server | 1 |
| MCP Session Acceptor | `uds/mcp_listener.rs` (new) | unimatrix-server | 2 |
| Bridge Client | `bridge.rs` (new) | unimatrix-server | 2 |
| Stop Subcommand | `main.rs` (modify) | unimatrix-server | 2 |
| ProjectPaths addition | `project.rs` (modify) | unimatrix-engine | 1 |

---

## Wave Dependency Order

### Wave 1 — Foundation (build first, no inter-wave deps)

1. **`project.rs`** — add `mcp_socket_path` field. Everything else depends on `ProjectPaths`.
2. **`server.rs` refactor** — `PendingEntriesAnalysis` two-level structure + `UnimatrixServer`
   clone verification. No new runtime dependencies.
3. **`infra/shutdown.rs`** — add `mcp_socket_guard` + `mcp_acceptor_handle` fields; decouple
   graceful_shutdown from transport close. Required by Wave 2 daemon startup.
4. **`infra/daemon.rs`** — daemonizer uses only `ProjectPaths` and `nix`; no tokio.

### Wave 2 — Runtime components (depend on Wave 1)

5. **`uds/mcp_listener.rs`** — depends on `UnimatrixServer` clone (Wave 1 server.rs) and
   `LifecycleHandles` additions (Wave 1 shutdown.rs).
6. **`bridge.rs`** — depends on `ProjectPaths::mcp_socket_path` (Wave 1 project.rs).
7. **`main.rs`** — integrates all of the above; modified last.

**C-04 joint gate**: `server.rs` clone verification and `shutdown.rs` decoupling are reviewed
as a single unit. Neither may be merged without the other (IMPLEMENTATION-BRIEF C-04).

---

## Data Flow Between Components

```
.mcp.json invocation
    |
    v
[main.rs: no subcommand]
    |--- bridge path (async) --> [bridge.rs: run_bridge]
    |       |
    |       v
    |   ProjectPaths::mcp_socket_path
    |       |
    |       +--(connect OK)--> tokio::io::copy_bidirectional(stdin, uds_stream)
    |       |
    |       +--(ECONNREFUSED)--> auto-start sequence
    |               |
    |               v
    |           [infra/daemon.rs: run_daemon_launcher]
    |               |
    |               v
    |           spawn child --daemon-child
    |               |
    |               v
    |           [main.rs: --daemon-child path]
    |               |
    |           prepare_daemon_child() -- setsid() BEFORE tokio init
    |               |
    |           tokio_main_daemon()
    |               |
    |           [uds/mcp_listener.rs: start_mcp_uds_listener]
    |               |
    |           accept loop (CancellationToken)
    |               |--- per session ---> server.clone().serve(duplex(r,w))
    |
    |--- stop subcommand (sync) --> [main.rs: run_stop]
    |       |
    |       v
    |   pidfile::terminate_and_wait(pid, 15s)
    |
    |--- serve --stdio --> tokio_main (stdio, unchanged lifecycle)
    |
    |--- serve --daemon --> run_daemon_launcher + poll socket
    |
    |--- hook subcommand --> sync path (unchanged, C-10)
```

---

## Shared Types Introduced or Modified

### `ProjectPaths` (unimatrix-engine/src/project.rs)

New field added:
```
pub mcp_socket_path: PathBuf  -- data_dir.join("unimatrix-mcp.sock")
```

### `LifecycleHandles` (infra/shutdown.rs)

New fields added:
```
pub mcp_socket_guard: Option<SocketGuard>
pub mcp_acceptor_handle: Option<tokio::task::JoinHandle<()>>
```

### `PendingEntriesAnalysis` (server.rs) — REFACTORED

Before (flat):
```
pub entries: HashMap<u64, EntryAnalysis>
```

After (two-level, authoritative per ADR-004 / WARN-01 resolution):
```
pub buckets: HashMap<String, FeatureBucket>
```

### `FeatureBucket` (server.rs) — NEW

```
pub struct FeatureBucket {
    pub entries: HashMap<u64, EntryAnalysis>,
    pub last_updated: u64,  // unix seconds, for TTL eviction
}
```

---

## Critical Shared Invariants

### Drop Order (must be enforced in graceful_shutdown)

```
1. mcp_socket_guard  -- drops unimatrix-mcp.sock
2. socket_guard      -- drops unimatrix.sock
3. All Arc<Store> holders:
   - mcp_acceptor_handle.abort() (releases session task Arc clones)
   - uds_handle.abort()
   - tick_handle.abort()
   - services (ServiceLayer internal Arc<Store> clones)
   - adapt_service, registry, audit, vector_index
4. Arc::try_unwrap(store) for compaction
5. PidGuard drops last (RAII in main.rs caller)
```

### Graceful Shutdown Call Site Audit (C-05)

Current codebase: exactly ONE call site at `main.rs` line 405 (after `running.waiting()`).
After refactor: exactly ONE call site, reachable only from daemon token cancellation path.
The `QuitReason::Closed` path (session EOF in daemon mode) MUST NOT call `graceful_shutdown`.
The `QuitReason::Closed` path in stdio mode (`serve --stdio`) MUST still exit the process.

### C-07 Exemption Boundary

`CallerId::UdsSession` rate-limit exemption is at one site in `server.rs` (or the tool
pipeline). That site MUST carry the comment:
```
// C-07: UDS is filesystem-gated (0600 socket) — rate-limit exemption is
// local-only. When HTTP transport is introduced (W2-2), the HTTP CallerId
// variant MUST NOT inherit this exemption.
```

---

## Sequencing Constraints

- `ensure_data_directory` in `project.rs` must return `mcp_socket_path` before anything else
  can reference it. This is the first code change.
- `server.rs` refactor must be complete before `mcp_listener.rs` is written — the listener
  calls `server.clone()`.
- `shutdown.rs` must accept `mcp_socket_guard` and `mcp_acceptor_handle` before `main.rs`
  can build the `LifecycleHandles` struct.
- `infra/daemon.rs` is pure sync; can be written in parallel with `server.rs` refactor.
- `bridge.rs` and `stop_cmd` in `main.rs` can be written in parallel after Wave 1 is complete.
