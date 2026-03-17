# Agent Report: vnc-005-agent-1-architect

## Status: COMPLETE

## Output Files

- `/workspaces/unimatrix/product/features/vnc-005/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/vnc-005/architecture/ADR-001-daemonization-pattern.md`
- `/workspaces/unimatrix/product/features/vnc-005/architecture/ADR-002-session-daemon-lifetime-separation.md`
- `/workspaces/unimatrix/product/features/vnc-005/architecture/ADR-003-unimatrixserver-sharing-model.md`
- `/workspaces/unimatrix/product/features/vnc-005/architecture/ADR-004-feature-cycle-accumulator.md`
- `/workspaces/unimatrix/product/features/vnc-005/architecture/ADR-005-mcp-accept-loop-topology.md`
- `/workspaces/unimatrix/product/features/vnc-005/architecture/ADR-006-unimatrix-stop-subcommand.md`

## Unimatrix ADR Entries Stored

- #1911 — ADR-001: Daemonization via spawn-new-process
- #1912 — ADR-002: Session/daemon lifetime separation via CancellationToken
- #1913 — ADR-003: UnimatrixServer sharing via Clone
- #1914 — ADR-004: Feature-cycle-keyed accumulator with Mutex + TTL eviction
- #1915 — ADR-005: MCP accept loop topology
- #1916 — ADR-006: unimatrix stop subcommand

## Key Decisions

| Decision | Choice |
|---|---|
| Daemonization | spawn-new-process (std::process::Command + --daemon-child flag); no fork in async context |
| Session/daemon lifetime | Per-session CancellationToken child; daemon token cancels acceptor; sessions drained before graceful_shutdown |
| UnimatrixServer sharing | Clone directly (already derives Clone, all fields Arc); no Arc<UnimatrixServer> wrapper needed |
| Accumulator | HashMap<feature_cycle, FeatureBucket> + Mutex; eviction on drain/cycle/72h TTL |
| Accept loop | Single acceptor task + per-connection spawned tasks; MAX_CONCURRENT_SESSIONS=32 cap |
| unimatrix stop | Synchronous, no Tokio; reuses terminate_and_wait; exit codes 0/1/2 |

## New Integration Surface

| Item | Type | Location |
|---|---|---|
| `ProjectPaths::mcp_socket_path` | `PathBuf` (new field) | `unimatrix-engine/src/project.rs` |
| `LifecycleHandles::mcp_socket_guard` | `Option<SocketGuard>` | `infra/shutdown.rs` |
| `LifecycleHandles::mcp_acceptor_handle` | `Option<JoinHandle<()>>` | `infra/shutdown.rs` |
| `PendingEntriesAnalysis::buckets` | `HashMap<String, FeatureBucket>` | `server.rs` |
| `PendingEntriesAnalysis::upsert(feature_cycle, analysis)` | method | `server.rs` |
| `PendingEntriesAnalysis::drain_for(feature_cycle)` | method | `server.rs` |
| `start_mcp_uds_listener(path, server, token)` | `async fn → Result<(JoinHandle<()>, SocketGuard), ServerError>` | `uds/mcp_listener.rs` (new) |
| `Cli::Command::Serve { daemon: bool }` | new clap variant | `main.rs` |
| `Cli::Command::Stop` | new clap variant | `main.rs` |
| `--daemon-child` | hidden bool arg | `main.rs` |

## Open Questions for Implementor

1. **Connection cap behavior (SR-09)**: Dropped connections at MAX_CONCURRENT_SESSIONS cap are silent (closed before MCP handshake). Document this in the impl.
2. **Bridge timeout error message (SR-03)**: When 5-second auto-start poll times out, exact stderr message and exit code to be decided at implementation time.
3. **Session drain timeout**: 30-second join timeout on session handles on daemon shutdown — confirm acceptable for longest tool call (context_store with embedding).
4. **Log rotation**: Not in scope for vnc-005; log file grows indefinitely.
