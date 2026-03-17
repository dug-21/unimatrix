# Scope Risk Assessment: vnc-005

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `transport-async-rw` wrapping `UnixStream` as rmcp transport is documented in scope research but untested in this codebase; rmcp =0.16.0 is pinned with no upgrade path | High | Med | Architect must prototype `server.serve(unix_stream)` call in isolation before committing to session-per-task design |
| SR-02 | `nix` crate `fork(2)`+`setsid(2)` daemonization leaves Tokio runtime state (thread pool, signal handlers, open fds) in an undefined state post-fork if called after runtime init | High | High | Architect must resolve fork-before-runtime or double-fork-with-exec ordering; do not assume nix call placement is trivial |
| SR-03 | 5-second auto-start timeout assumes lazy embedding model load; if the model eagerly initializes during daemon startup the bridge silently fails with no user-visible error | Med | Med | Spec must define bridge failure behavior when timeout elapses (error to stdout? exit code?) |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Default binary behavior change (no-subcommand → bridge mode) is a breaking behavioral change for any script or tooling that invokes `unimatrix` expecting a stdio server; `unimatrix serve --stdio` must be a drop-in replacement | High | Med | Spec must enumerate all current invocation paths and confirm `unimatrix serve --stdio` substitutes exactly; regression AC-09 is necessary but not sufficient |
| SR-05 | `pending_entries_analysis` refactor to `HashMap<feature_cycle, Vec<EntryRecord>>` introduces a multi-session accumulator with undefined eviction policy; stale buckets for completed features accumulate indefinitely | Med | High | Spec must define when and how stale feature-cycle buckets are evicted (on `context_cycle`? on drain? TTL?) |
| SR-06 | `UnimatrixServer` Clone/Arc refactor is described as "structural, not semantic" but the existing `graceful_shutdown` coupling to a single-instance lifecycle means the refactor touches the same code path as the shutdown decoupling (SR-07 below) | Med | Med | Architect should treat server cloneability and shutdown decoupling as a single coordinated refactor, not two independent tasks |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | `graceful_shutdown` is tightly coupled to `QuitReason::Closed` (transport close); decoupling it so session close ≠ daemon shutdown is a non-trivial restructure of main.rs; lesson #735 shows fire-and-forget DB write patterns introduce silent failures under contention — the shutdown drain path is the same contention point | High | High | Architect must map every caller of `graceful_shutdown` and every write path that must drain before it; this is the highest-implementation-risk item in the scope |
| SR-08 | Two-socket design (hook IPC on `unimatrix.sock`, MCP on `unimatrix-mcp.sock`) means `SocketGuard` RAII must cover both sockets; partial cleanup on crash (one guard drops, other does not) leaves a stale MCP socket that blocks the next auto-start | Med | Med | Spec must define stale socket detection and unlink logic for `unimatrix-mcp.sock` (mirroring `handle_stale_socket` for hook IPC) |
| SR-09 | Per-session tokio task spawning for MCP connections has no documented backpressure or connection cap; a misbehaving Claude Code instance that reconnects in a tight loop could exhaust the task pool (pattern #1688 identified spawn_blocking saturation as a prior failure mode) | Med | Low | Spec should define maximum concurrent MCP session limit or document that unbounded is acceptable |

## Assumptions

- **SCOPE §Background Research / rmcp transport**: Assumes `transport-async-rw` is activated by the `server` feature and requires no additional Cargo.toml change. If rmcp's feature graph changed in =0.16.0 this assumption fails silently at compile time with a missing trait error.
- **SCOPE §Component 3 / OQ-04**: Assumes 5-second wait is sufficient for daemon socket appearance. If the host is under heavy load or the embedding model is not lazy, this assumption fails with a bridge timeout and no MCP session.
- **SCOPE §Constraints #6**: Assumes `nix::unistd::fork` is safe to call in a tokio binary without triggering Tokio's post-fork undefined behavior. This assumption requires explicit validation — tokio explicitly warns against fork after runtime start.
- **SCOPE §Component 1**: Assumes `UnimatrixServer` internal state is already fully `Arc`-wrapped and Clone derivation is mechanical. Any state field that is not `Arc`-wrapped (e.g., a raw handle or a non-Clone resource) blocks this assumption.

## Design Recommendations

- **SR-02 (Critical)**: Resolve the tokio+fork ordering before any other architecture work. The safest pattern is fork-before-tokio-init (launcher forks, child execs or initializes runtime fresh). Architect should confirm which pattern rmcp and the existing PidGuard support.
- **SR-07 (Critical)**: Treat graceful shutdown decoupling as the highest-risk refactor. Map all `graceful_shutdown` call sites and all write queues before designing the session/daemon lifetime split. Lesson #735 (spawn_blocking pool saturation) and ADR-005 (`Arc::try_unwrap` shutdown) are directly relevant.
- **SR-04 (High)**: The default invocation change is user-facing. Spec must lock down the exact fallback behavior matrix: no daemon + bridge → auto-start; no daemon + bridge + auto-start fails → error with instructions; daemon unhealthy → ???. Ambiguity here will cause field breakage.
