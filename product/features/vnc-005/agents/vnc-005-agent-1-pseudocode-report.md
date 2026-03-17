# Agent Report: vnc-005-agent-1-pseudocode

## Task

Produce per-component pseudocode for vnc-005 (Daemon Mode — Persistent Background Server
via UDS MCP Transport).

## Files Produced

| File | Component |
|---|---|
| `pseudocode/OVERVIEW.md` | Component interaction, data flow, shared types, wave order |
| `pseudocode/daemonizer.md` | Component 1: `infra/daemon.rs` |
| `pseudocode/mcp_listener.md` | Component 2: `uds/mcp_listener.rs` |
| `pseudocode/server_refactor.md` | Components 3 + 5 combined: `server.rs` |
| `pseudocode/shutdown.md` | Component 4: `infra/shutdown.rs` |
| `pseudocode/bridge.md` | Component 6: `bridge.rs` |
| `pseudocode/stop_cmd.md` | Component 7 + main.rs additions |

## Components Covered

1. Daemonizer (`infra/daemon.rs`) — Wave 1
2. MCP Session Acceptor (`uds/mcp_listener.rs`) — Wave 2
3 + 5. UnimatrixServer Clone + PendingEntriesAnalysis refactor (`server.rs`) — Wave 1, single file
4. Shutdown Signal Router (`infra/shutdown.rs`) — Wave 1
6. Bridge Client (`bridge.rs`) — Wave 2
7. Stop Subcommand + main.rs routing (`main.rs`) — Wave 2

Also covers: `ProjectPaths::mcp_socket_path` addition to `unimatrix-engine/src/project.rs` (Wave 1,
referenced in OVERVIEW.md and bridge/stop_cmd pseudocode).

## Wave Dependency Order

Wave 1 (build first, no inter-wave deps):
1. `project.rs` — `mcp_socket_path` field
2. `server.rs` — `PendingEntriesAnalysis` refactor + clone verification
3. `infra/shutdown.rs` — `LifecycleHandles` additions + decoupled `graceful_shutdown`
4. `infra/daemon.rs` — daemonizer (pure sync, no tokio dep)

Wave 2 (depends on Wave 1):
5. `uds/mcp_listener.rs` — MCP acceptor
6. `bridge.rs` — bridge client
7. `main.rs` — integrates all above

C-04 gate: items 2 and 3 are reviewed as a single unit.

## Open Questions / Gaps

1. **`tokio_main` split structure** — The current `tokio_main` function must be split into
   `tokio_main_daemon` and `tokio_main_stdio`. The `stop_cmd.md` pseudocode describes both.
   The implementation agent should confirm the clap `daemon_child` flag placement: it is a
   top-level `Cli` field (not inside `Serve`), which means `main()` must check it before
   the subcommand match. This ordering is specified in `stop_cmd.md` but must be confirmed
   against clap's parsing model (flag visibility across subcommands).

2. **`CallerId::UdsSession` exact location** — The pseudocode specifies the required code
   comment (C-07 / W2-2) but the implementation agent must grep for the current rate-limit
   match arm location (it may be in `server.rs`, `services/`, or the tool pipeline). This
   is not ambiguous in design but requires a code search at implementation time.

3. **Bridge `copy_bidirectional` API** — `tokio::io::copy_bidirectional` takes `(A, B)` where
   both implement `AsyncRead + AsyncWrite`. `tokio::io::stdin()` and `tokio::io::stdout()`
   are not combined into a single type. The `bridge.md` pseudocode provides two approaches:
   `copy_bidirectional` with a combined handle, or a two-task `select!`. The implementation
   agent should use the `select!` approach (explicitly described in `bridge.md`) as it maps
   cleanly to separate stdin/stdout types.

4. **`--project-dir` forwarding in `spawn_daemon`** — The bridge and launcher both pass
   `--project-dir` to the child process. This is correct but creates a dependency on the
   flag being parseable as a `serve --daemon` argument. Implementation agent must verify
   clap accepts `--project-dir` at the top level when `serve --daemon` is the subcommand.

5. **Existing tests for `LifecycleHandles`** — `shutdown.rs` contains integration tests
   (`test_shutdown_drops_release_all_store_refs` etc.) that construct `LifecycleHandles`
   directly. These will fail to compile after the new fields are added unless updated.
   The `shutdown.md` pseudocode calls this out in test scenario 9.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server daemon UDS MCP session lifecycle CancellationToken` — found #1897 (thin stdio bridge pattern), #1898 (separate sockets for hook IPC and MCP), #300 (UDS fixed capability set authorization boundary). All three patterns are directly applicable and consistent with the architecture. No stale entries found.
- Queried: `/uni-query-patterns` for `graceful shutdown Arc try_unwrap store session join` — found #1367 (spawn_blocking timeout pattern), #1560 (Arc<RwLock<T>> background-tick cache), #312 outcome (bugfix-264 Arc/store regression). All referenced in Risk Register evidence (R-01, R-05).
- Deviations from established patterns: none. The spawn-new-process daemonization (ADR-001) is novel for this codebase but does not contradict any stored pattern. The two-socket separation (#1898) and UDS capability boundary (#300) are reinforced by this design.
