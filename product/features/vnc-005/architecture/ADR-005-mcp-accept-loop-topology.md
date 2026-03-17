## ADR-005: MCP Accept Loop — Single Acceptor Task + Per-Connection Spawned Tasks

### Context

The daemon must accept multiple concurrent MCP sessions on `unimatrix-mcp.sock`.
Each Claude Code window is a separate MCP session. Typical usage is 1-2 concurrent
sessions; the architecture should handle up to ~16 without degradation.

The scope document (Component 1) proposes: "Accept loop: for each incoming connection,
wrap the `UnixStream` as an rmcp transport using `transport-async-rw`, spawn a tokio
task that calls `server.serve(stream)` for that session."

SR-09 from the risk assessment notes that unbounded task spawning could be a concern
if a misbehaving client reconnects in a tight loop. The prior failure pattern #1688
(spawn_blocking pool saturation) is a relevant reference.

Three topologies considered:

**Option A: Single acceptor task + unlimited per-connection tasks.** One long-running
acceptor task loops on `listener.accept()`. For each connection, spawns a new task
with `tokio::spawn(run_session(...))`. No connection cap.

**Option B: Single acceptor task + semaphore-bounded per-connection tasks.** Same as
A, but a `tokio::sync::Semaphore` with N permits gates task spawning. New connections
are accepted from the OS but held (the `accept()` call is not issued) until a permit is
available. On semaphore exhaustion, the oldest task's hold is not interrupted — the
semaphore just prevents new spawns.

**Option C: Multi-threaded acceptor using `tokio::task::JoinSet`.** `JoinSet` manages
the session handles, allowing completed sessions to be reaped in the background without
the acceptor manually sweeping a `Vec`.

### Decision

Use **Option A** for the initial implementation, with a `MAX_CONCURRENT_SESSIONS`
constant set to 32 and a connection-count guard (not a semaphore — just a counter).

Rationale for Option A over Option B:
- The `spawn_blocking` pool saturation (pattern #1688) applies to `spawn_blocking`
  threads, not async tasks. Async tasks are cheap (a few KB each). 32 concurrent MCP
  sessions would represent an extraordinary load on a local dev workspace. The risk
  is not spawn_blocking exhaustion but rather the bounded number of `Mutex<Connection>`
  write slots in SQLite — which is already serialized regardless of session count.
- A semaphore adds complexity with no practical benefit for the intended usage profile.

The connection-count guard:
- The acceptor tracks `active_sessions: usize` (atomically via `Arc<AtomicUsize>` or
  via a counter in the accept loop's local state).
- When `active_sessions >= MAX_CONCURRENT_SESSIONS`, the accept loop still calls
  `listener.accept()` (to drain the OS queue), but immediately closes the stream
  without spawning a task.
- This prevents file descriptor exhaustion without adding semaphore complexity.

Rationale for Option A over Option C:
- `JoinSet` is cleaner for reaping completed tasks, but it is in `tokio::task` and
  requires `tokio >= 1.21`. The current codebase uses tokio directly and the
  `JoinSet` API is available. However, since the accept loop already needs to sweep
  finished handles (per ADR-002 consequences), using a `Vec` with periodic
  `is_finished()` sweeps is equally correct. `JoinSet` is a valid future refactor.

The accept loop structure:

```rust
async fn run_mcp_acceptor(
    listener: tokio::net::UnixListener,
    server: UnimatrixServer,
    daemon_token: CancellationToken,
) {
    let mut session_handles: Vec<tokio::task::JoinHandle<()>> = Vec::new();
    let active_count = Arc::new(AtomicUsize::new(0));

    loop {
        // Sweep finished handles before accepting new connections
        session_handles.retain(|h| !h.is_finished());

        tokio::select! {
            _ = daemon_token.cancelled() => break,
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        if active_count.load(Ordering::Relaxed) >= MAX_CONCURRENT_SESSIONS {
                            // Drop stream — OS accept queue is drained but session refused
                            tracing::warn!("max concurrent sessions reached, dropping connection");
                            continue;
                        }
                        let child_token = daemon_token.child_token();
                        let server_clone = server.clone();
                        let count = Arc::clone(&active_count);
                        let handle = tokio::spawn(async move {
                            count.fetch_add(1, Ordering::Relaxed);
                            run_session(stream, server_clone, child_token).await;
                            count.fetch_sub(1, Ordering::Relaxed);
                        });
                        session_handles.push(handle);
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "accept error on MCP socket");
                    }
                }
            }
        }
    }

    // Daemon token fired — wait for all sessions to finish
    daemon_token.cancel(); // child tokens already fired
    for handle in session_handles {
        let _ = tokio::time::timeout(Duration::from_secs(30), handle).await;
    }
}
```

rmcp transport wrapping uses `transport-async-rw`. The `UnixStream` is split into
`OwnedReadHalf` and `OwnedWriteHalf` via `into_split()`, then passed to
`rmcp::transport::io::duplex(read_half, write_half)`.

### Consequences

Easier:
- Simple, readable accept loop with no additional synchronization primitives beyond
  what ADR-002 requires.
- `MAX_CONCURRENT_SESSIONS = 32` is a safe upper bound that can be tuned without
  architecture changes.
- The periodic `retain(|h| !h.is_finished())` sweep keeps the `Vec` from growing
  unboundedly in long-running daemons.

Harder:
- The `AtomicUsize` for `active_count` is slightly redundant with `session_handles.len()`.
  Using the Vec length is simpler but requires the retain-sweep to run atomically with
  the new-spawn check. The AtomicUsize is slightly more defensive. Either approach is
  acceptable; implementors may use whichever they find cleaner.
- Dropped connections (when at the cap) are silent from the client's perspective —
  the rmcp client will see a connection-refused or immediate close. No error message
  is sent to the client over the MCP protocol because the connection is dropped before
  an MCP session is established.
