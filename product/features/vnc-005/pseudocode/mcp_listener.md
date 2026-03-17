# Pseudocode: MCP Session Acceptor (`uds/mcp_listener.rs`)

## Purpose

Bind `unimatrix-mcp.sock` (permissions 0600), run the accept loop as a Tokio task, spawn a
per-session task for each incoming `UnixStream`, and enforce the `MAX_CONCURRENT_SESSIONS = 32`
cap. Returns the acceptor task `JoinHandle` and a `SocketGuard` to the daemon startup so both
can be stored in `LifecycleHandles` for orderly shutdown.

See ADR-005 for the topology decision (single acceptor + per-connection tasks, counter-based
cap, periodic `retain` sweep).

---

## Files Affected

- **New**: `crates/unimatrix-server/src/uds/mcp_listener.rs`
- **Modified**: `crates/unimatrix-server/src/uds/mod.rs` — add `pub mod mcp_listener;`

---

## Dependencies

- `tokio::net::{UnixListener, UnixStream}` — socket I/O
- `tokio::task::JoinHandle` — session handles
- `tokio_util::sync::CancellationToken` — daemon lifetime signal
- `std::sync::atomic::{AtomicUsize, Ordering}` — active session counter
- `std::sync::Arc` — shared counter
- `rmcp::ServiceExt` — `.serve(transport)` method
- `rmcp::transport::io::duplex` — wraps `(AsyncRead, AsyncWrite)` as rmcp transport
- `crate::uds::listener::SocketGuard` — RAII socket cleanup (reused from hook IPC)
- `crate::uds::listener::handle_stale_socket` — reused stale file removal
- `crate::server::UnimatrixServer` — cloned into each session task
- `crate::error::ServerError` — error type

---

## Constants

```
MAX_CONCURRENT_SESSIONS: usize = 32
SESSION_JOIN_TIMEOUT: Duration = 30s       // per-session join at daemon shutdown
```

---

## Function: `start_mcp_uds_listener`

### Signature

```
pub async fn start_mcp_uds_listener(
    path: &Path,
    server: UnimatrixServer,
    shutdown_token: CancellationToken,
) -> Result<(JoinHandle<()>, SocketGuard), ServerError>
```

### Pseudocode

```
async fn start_mcp_uds_listener(path, server, shutdown_token):

    // Step 1: Validate socket path length (C-08 / FR-20)
    validate_socket_path_length(path)?

    // Step 2: Remove stale socket if present (FR-14, reuse existing function)
    handle_stale_socket(path)
        .map_err(|e| ServerError::ProjectInit(format!("mcp socket cleanup: {e}")))?

    // Step 3: Bind the UnixListener
    listener = tokio::net::UnixListener::bind(path)
        .map_err(|e| ServerError::ProjectInit(format!("bind mcp socket: {e}")))?

    // Step 4: Set permissions 0600 (FR-13)
    // MUST happen before accept loop starts — no window where wrong perms apply.
    #[cfg(unix)]
    std::fs::set_permissions(path, Permissions::from_mode(0o600))
        .map_err(|e| ServerError::ProjectInit(format!("set mcp socket permissions: {e}")))?

    tracing::info!(path = %path.display(), "MCP UDS listener bound (0600)")

    // Step 5: Create SocketGuard for RAII cleanup
    socket_guard = SocketGuard::new(path.to_path_buf())

    // Step 6: Spawn the accept loop task
    handle = tokio::spawn(run_mcp_acceptor(listener, server, shutdown_token))

    return Ok((handle, socket_guard))
```

---

## Function: `validate_socket_path_length`

### Signature

```
fn validate_socket_path_length(path: &Path) -> Result<(), ServerError>
```

### Pseudocode

```
fn validate_socket_path_length(path):
    // C-08 / FR-20: macOS sun_path limit is 104 bytes; use 103 as safe margin.
    // IMPLEMENTATION-BRIEF states 107 as the minimum across platforms, but
    // ARCHITECTURE.md and C-08 state 103 (one byte below macOS 104-byte limit).
    // Use 103 — the more conservative value from C-08.
    MAX_SOCKET_PATH_BYTES: usize = 103

    path_bytes = path.as_os_str().len()

    // Also check for null bytes (edge case: security risk for C-string based syscalls)
    path_str = path.as_os_str().as_bytes()
    if path_str.contains(&0u8):
        return Err(ServerError::ProjectInit(
            "socket path contains null byte".to_string()
        ))

    if path_bytes > MAX_SOCKET_PATH_BYTES:
        return Err(ServerError::ProjectInit(format!(
            "socket path too long: {} bytes (max {}); home directory path is too long for UDS. Path: {}",
            path_bytes, MAX_SOCKET_PATH_BYTES, path.display()
        )))

    Ok(())
```

---

## Function: `run_mcp_acceptor` (private, spawned as task)

### Signature

```
async fn run_mcp_acceptor(
    listener: UnixListener,
    server: UnimatrixServer,
    daemon_token: CancellationToken,
)
```

### Pseudocode

```
async fn run_mcp_acceptor(listener, server, daemon_token):

    session_handles: Vec<JoinHandle<()>> = Vec::new()
    active_count: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0))

    loop:
        // R-10: Sweep finished handles BEFORE accepting new connections (every iteration)
        session_handles.retain(|h| !h.is_finished())

        tokio::select!:
            // Branch 1: daemon shutdown signal
            _ = daemon_token.cancelled() =>
                break

            // Branch 2: incoming connection
            result = listener.accept() =>
                match result:
                    Err(e):
                        tracing::error!(error = %e, "MCP accept error")
                        // Do NOT break — transient accept errors should not kill the daemon
                        continue

                    Ok((stream, _addr)):
                        // Check session cap AFTER accepting from OS queue (C-15 from ADR-005)
                        current = active_count.load(Ordering::Relaxed)
                        if current >= MAX_CONCURRENT_SESSIONS:
                            // R-11: Log warn and drop stream — client sees connection close
                            tracing::warn!(
                                current_sessions = current,
                                "max concurrent sessions ({}) reached; dropping connection",
                                MAX_CONCURRENT_SESSIONS
                            )
                            // Stream is dropped here; OS notifies client of close
                            drop(stream)
                            continue

                        // Spawn per-session task
                        child_token = daemon_token.child_token()
                        server_clone = server.clone()
                        count_clone = Arc::clone(&active_count)

                        handle = tokio::spawn(async move {
                            count_clone.fetch_add(1, Ordering::Relaxed)
                            run_session(stream, server_clone, child_token).await
                            count_clone.fetch_sub(1, Ordering::Relaxed)
                        })

                        session_handles.push(handle)

    // Daemon token cancelled — drain active sessions
    // child_token cancellation was triggered by daemon_token.cancel() automatically;
    // all child tokens are already cancelled when the parent fires.

    tracing::info!(
        active = active_count.load(Ordering::Relaxed),
        "MCP acceptor shutting down; joining active sessions"
    )

    // Join all session handles with timeout (ADR-002: 30s total window)
    for handle in session_handles.drain(..):
        match tokio::time::timeout(SESSION_JOIN_TIMEOUT, handle).await:
            Ok(Ok(())) => {}   // clean exit
            Ok(Err(e)) =>
                // R-??: Session task panic — log but continue shutdown
                tracing::error!(error = %e, "session task panicked during shutdown")
            Err(_elapsed) =>
                // Timeout — session task is stuck; continue anyway
                tracing::warn!("session task did not exit within timeout; continuing shutdown")
```

---

## Function: `run_session` (private)

### Signature

```
async fn run_session(
    stream: UnixStream,
    server: UnimatrixServer,
    child_token: CancellationToken,
)
```

### Pseudocode

```
async fn run_session(stream, server, child_token):

    // SR-01 prototype pattern: split stream and wrap as rmcp transport
    // UnixStream::into_split() gives (OwnedReadHalf, OwnedWriteHalf)
    (read_half, write_half) = stream.into_split()

    // rmcp transport-async-rw wraps (AsyncRead, AsyncWrite) pair
    // Requires `transport-async-rw` feature active on rmcp — already enabled
    // via the `server` feature in Cargo.toml.
    transport = rmcp::transport::io::duplex(read_half, write_half)

    // Serve the MCP session
    // server.clone() is a cheap Arc refcount increment across all Arc fields
    // (ADR-003: all UnimatrixServer fields are Arc-wrapped)
    running = match server.serve(transport).await:
        Ok(r) => r
        Err(e):
            tracing::error!(error = %e, "MCP session setup failed")
            return

    // Bridge the daemon CancellationToken to rmcp's internal cancellation token.
    // This mirrors the existing pattern in main.rs (lines 389-394 pre-vnc-005).
    // Without this, SIGTERM does not propagate into the rmcp session loop.
    rmcp_cancel = running.cancellation_token()
    tokio::spawn(async move {
        child_token.cancelled().await
        tracing::debug!("daemon token cancelled; cancelling rmcp session transport")
        rmcp_cancel.cancel()
    })

    // Wait for this session to end:
    // - QuitReason::Closed   -> bridge client disconnected (stdin EOF)
    // - QuitReason::Cancelled -> daemon shutdown token propagated
    //
    // ADR-002 / C-04: Do NOT call graceful_shutdown here.
    // When the session ends, only this task exits. The daemon continues.
    match running.waiting().await:
        Ok(reason) =>
            tracing::debug!(?reason, "MCP session ended")
        Err(e) =>
            tracing::error!(error = %e, "MCP session task failed")

    // Arc<UnimatrixServer> clone drops here when this async block ends.
    // All Arc fields decrement their refcounts.
    // graceful_shutdown can call Arc::try_unwrap(store) only after ALL session
    // tasks have exited (guaranteed by the join loop in run_mcp_acceptor).
```

---

## Integration Notes

### Caller in `main.rs` (daemon path)

```
// After server is constructed and before lifecycle_handles is built:
let (mcp_acceptor_handle, mcp_socket_guard) = uds::mcp_listener::start_mcp_uds_listener(
    &paths.mcp_socket_path,
    server.clone(),           // Clone once for the listener; original stays with main
    daemon_token.clone(),
).await?;

// Add to LifecycleHandles:
LifecycleHandles {
    ...
    mcp_socket_guard: Some(mcp_socket_guard),
    mcp_acceptor_handle: Some(mcp_acceptor_handle),
}
```

### C-07 CallerId::UdsSession exemption

The `CallerId::UdsSession` variant is set when a session arrives via `run_session`. The
exemption from rate limiting must carry the code comment at the match arm site:

```rust
// C-07: UDS is filesystem-gated (0600 socket) — rate-limit exemption is
// local-only. When HTTP transport is introduced (W2-2), the HTTP CallerId
// variant MUST NOT inherit this exemption. See W2-2 in PRODUCT-VISION.md.
CallerId::UdsSession => { /* no rate limit */ }
```

The implementation agent must locate the existing rate-limit check in `server.rs` or the
tool pipeline and add this comment. The comment is a gate requirement (C-07, R-07).

---

## Key Test Scenarios

1. **Socket permissions are 0600 before first accept** — bind socket, stat it immediately;
   assert mode bits are 0600 before any `accept()` call returns.

2. **Stale MCP socket is unlinked at startup** (AC-16, R-09) — pre-create a plain file at
   `mcp_socket_path`; call `start_mcp_uds_listener`; assert it succeeds and socket is bound.

3. **Session cap enforcement** (AC-20, R-11) — open 32 concurrent streams; attempt 33rd;
   assert 33rd is closed without a panic; assert daemon `warn!` log was emitted.

4. **Session handle Vec does not grow unboundedly** (R-10) — simulate 20 connect/disconnect
   cycles; assert `session_handles.len()` after all disconnects is 0 or near-zero (retain
   sweep runs every accept iteration).

5. **Daemon token cancellation joins all sessions** (R-01, R-02) — open 3 sessions; cancel
   daemon token; assert all session tasks exit within `SESSION_JOIN_TIMEOUT`; assert
   `Arc::strong_count` on server is 1 (only acceptor's copy) before function returns.

6. **Session task panic does not crash acceptor** — inject a panic in a session task; assert
   acceptor loop logs the error and continues accepting new connections.

7. **Socket path length validation rejects oversized path** (C-08, R-14) — construct a path
   of 104 bytes; assert `start_mcp_uds_listener` returns `Err` before binding.

8. **ECONNREFUSED during poll is retried, not fatal** — bridge connect attempt during the
   window between "socket file exists" and "listen() called"; this is handled at the bridge
   level (bridge pseudocode), but the acceptor must not reject connections during startup.

9. **SR-01 prototype validation** — call `server.clone().serve(duplex(read, write))` with
   a real in-memory `UnixStream` pair (`tokio::net::UnixListener::bind` to a temp path);
   assert the serve call returns a `RunningService` without error. This is the transport-
   async-rw wrapping smoke test called for in IMPLEMENTATION-BRIEF SR-01.
