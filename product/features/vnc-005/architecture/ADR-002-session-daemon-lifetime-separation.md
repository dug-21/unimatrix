## ADR-002: Session Lifetime Separate from Daemon Lifetime via CancellationToken

### Context

SR-07 from the risk assessment is the highest-implementation-risk item in the scope.
The current `main.rs` stdio path is structured as:

```
serve(stdio()) → running.waiting() → graceful_shutdown()
```

`waiting()` returns when the transport closes (client disconnect) or when the
cancellation token is triggered by a signal handler. In daemon mode, this structure
must change: a client disconnect must NOT trigger `graceful_shutdown`. The daemon
must survive session end.

Every caller of `graceful_shutdown` must be audited. In the current codebase there is
exactly one call site: `main.rs` line 405, after `running.waiting()` returns. The
function itself aborts the tick handle, drops all Arc holders, and compacts the DB.
It must run exactly once per daemon lifetime, not once per session.

The SCOPE.md (§Key Constraint: Graceful Shutdown Changes) already documents this
requirement. The risk assessment (SR-06) notes that server cloneability and shutdown
decoupling are a coordinated refactor, not independent tasks.

The prior ADR-005 (entry #81, "Shutdown via Arc::try_unwrap") establishes that
`graceful_shutdown` succeeds only if all Arc<Store> references have been dropped. In
daemon mode, session tasks hold cloned `UnimatrixServer` values, which themselves hold
`Arc<Store>` clones. These clones must be dropped before `graceful_shutdown` attempts
`Arc::try_unwrap(store)`.

Three propagation models were considered:

**Model A: Shared shutdown token passed into every session task.** Each session task
selects on both the daemon token and `running.waiting()`. On daemon shutdown, the
token fires, the session task cancels its rmcp transport, `waiting()` returns, and the
task exits. The daemon waits for all session tasks to exit (join with timeout), then
calls `graceful_shutdown`.

**Model B: No notification to session tasks; daemon waits indefinitely for natural
drain.** The daemon stops accepting new connections, then joins all session task handles
without timeout. Sessions in flight finish naturally. Graceful shutdown runs after all
handles are joined.

**Model C: Broadcast channel.** A `tokio::sync::broadcast::Sender<()>` is sent to
each session task. On shutdown, the sender fires, receivers cancel their transports.

Model B is simplest but risks hanging the daemon if a session is stuck (e.g., rmcp
transport frozen on a network partition to a local socket). Model C adds complexity
with no benefit over Model A given that tokio-util `CancellationToken` is already in
the dependency tree.

### Decision

Use **Model A**: one daemon-level `CancellationToken` propagated to session tasks.

The daemon startup creates a `CancellationToken` (the "daemon token"). A signal
handler task watches SIGTERM/SIGINT and cancels it. The MCP acceptor loop holds a
clone of this token and exits when it fires.

Each session task receives a child token (`daemon_token.child_token()`) at spawn time.
The session task spawns its own inner task to monitor the child token and cancel the
rmcp transport if it fires. This mirrors the existing pattern in `main.rs` lines
389-394 (the signal handler that cancels the stdio transport).

Shutdown sequence in the daemon:

```rust
// In accept loop task:
loop {
    tokio::select! {
        _ = daemon_token.cancelled() => break,
        result = listener.accept() => {
            let stream = result?;
            let child_token = daemon_token.child_token();
            let server_clone = Arc::clone(&server);
            let handle = tokio::spawn(run_session(stream, server_clone, child_token));
            session_handles.push(handle);
        }
    }
}
// Token cancelled — notify all sessions
daemon_token.cancel(); // already cancelled, no-op; child tokens are already fired
// Join all session handles with 30-second total timeout
for handle in session_handles {
    let _ = tokio::time::timeout(Duration::from_secs(30), handle).await;
}
```

After all session handles are joined, `graceful_shutdown(lifecycle_handles)` is called.
At this point all session task clones of `UnimatrixServer` have been dropped, so
`Arc::try_unwrap(store)` succeeds (same guarantee as today, now extended to
multi-session).

Session tasks do NOT call `graceful_shutdown`. They only call `running.waiting()` and
exit. The `UnimatrixServer` clone drop releases the session's Arc references.

`LifecycleHandles` gains:
- `mcp_socket_guard: Option<SocketGuard>` for the MCP UDS socket
- `mcp_acceptor_handle: Option<JoinHandle<()>>` for the accept loop task
- Note: session handles are joined inside the accept loop task before it exits, so they
  do not need to be in `LifecycleHandles`. The `mcp_acceptor_handle` is what
  `graceful_shutdown` aborts (or waits on).

### Consequences

Easier:
- `graceful_shutdown` is called exactly once, unchanged in semantics.
- The stdio server path (`unimatrix serve --stdio`) remains structurally identical to
  the current `main.rs` flow — no refactor needed for that path.
- `Arc::try_unwrap(store)` succeeds because session clones are dropped before
  `graceful_shutdown` is reached.
- The 30-second join timeout on session handles prevents a stuck session from blocking
  daemon shutdown indefinitely.

Harder:
- Each session task needs a small wrapper to bridge between the CancellationToken and
  the rmcp `cancellation_token()` call. This is 5-8 lines of boilerplate per session
  task spawn (mirrors the existing pattern already in main.rs).
- The accept loop must track `JoinHandle<()>` values for all live sessions. Handles
  for completed sessions accumulate in the `Vec` until shutdown. For typical usage
  (1-2 sessions) this is negligible; for long-running daemons with hundreds of
  reconnections it becomes a growing Vec of completed handles. Implementation should
  periodically sweep completed handles from the Vec (e.g., on each new accept, filter
  out finished handles via `handle.is_finished()`).
