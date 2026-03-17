# Pseudocode: Shutdown Signal Router (`infra/shutdown.rs`)

## Purpose

Extend `LifecycleHandles` with the two new fields required by the MCP session acceptor:
`mcp_socket_guard` and `mcp_acceptor_handle`. Update `graceful_shutdown` to:

1. Abort the MCP acceptor (which internally joins all session tasks before returning).
2. Drop `mcp_socket_guard` before `socket_guard` (correct drop ordering for both sockets).
3. Ensure exactly one `graceful_shutdown` call site remains in the codebase (C-05).

Decouple the daemon shutdown path from session EOF: `graceful_shutdown` is called only
after the daemon token fires, NOT on `QuitReason::Closed` from a session disconnect.
The stdio path (`serve --stdio`) retains its current behavior: process exits on stdin close.

---

## Files Affected

- **Modified**: `crates/unimatrix-server/src/infra/shutdown.rs`

---

## Dependencies

- Existing: `LifecycleHandles`, `graceful_shutdown`, `shutdown_signal` — all in this file
- `crate::uds::listener::SocketGuard` — already imported
- `tokio::task::JoinHandle` — already in scope
- `tokio_util::sync::CancellationToken` — NOT imported here yet; `shutdown_signal` is the
  signal function. The `CancellationToken` for the daemon token lives in `main.rs` and is
  passed into the signal handler task there.

---

## `LifecycleHandles` — Extended Struct

### New Fields to Add

```
/// RAII guard for MCP UDS socket cleanup (vnc-005).
/// Dropped during graceful_shutdown BEFORE socket_guard (drop ordering).
pub mcp_socket_guard: Option<SocketGuard>,

/// Accept loop task handle for MCP sessions (vnc-005).
/// Aborted during graceful_shutdown; internally joins all session task handles.
pub mcp_acceptor_handle: Option<tokio::task::JoinHandle<()>>,
```

### Updated Struct (full field list, for implementation reference)

```
pub struct LifecycleHandles {
    pub store: Arc<Store>,
    pub vector_index: Arc<VectorIndex>,
    pub vector_dir: PathBuf,
    pub registry: Arc<AgentRegistry>,
    pub audit: Arc<AuditLog>,
    pub adapt_service: Arc<AdaptationService>,
    pub data_dir: PathBuf,
    pub mcp_socket_guard: Option<SocketGuard>,    // NEW (vnc-005)
    pub mcp_acceptor_handle: Option<JoinHandle<()>>,  // NEW (vnc-005)
    pub socket_guard: Option<SocketGuard>,        // existing (hook IPC)
    pub uds_handle: Option<JoinHandle<()>>,       // existing (hook IPC accept loop)
    pub tick_handle: Option<JoinHandle<()>>,      // existing
    pub services: Option<ServiceLayer>,           // existing (#92 fix)
}
```

The field order in the struct definition determines the default drop order in Rust. However,
`graceful_shutdown` explicitly controls drop order via `drop(handles.X.take())`, so struct
field order is not the primary enforcement mechanism. The explicit drop sequence in the
function body is authoritative.

---

## `graceful_shutdown` — Updated Sequence

### Current sequence (from source)

```
Step 0:   abort uds_handle + wait 1s
Step 0b:  drop socket_guard
Step 0c:  abort tick_handle + wait 1s
Step 1:   dump vector_index
Step 1b:  save adaptation state
Step 2:   drop ServiceLayer, adapt_service, registry, audit, vector_index
Step 3:   Arc::try_unwrap(store) -> compact
```

### Updated sequence (with MCP acceptor, maintaining correct drop order)

```
async fn graceful_shutdown(mut handles: LifecycleHandles) -> Result<(), ServerError>:

    // Brief pause for final responses to flush (unchanged)
    sleep(100ms)

    // Step 0: Stop MCP acceptor task (NEW — vnc-005)
    // The acceptor task's run_mcp_acceptor() joins all session tasks internally
    // before it returns (30s timeout per session). We abort the handle here
    // which signals the task to stop; the task has already broken its accept loop
    // because the daemon_token was cancelled (which happens before graceful_shutdown
    // is called in the daemon path).
    //
    // R-01: All session Arc<UnimatrixServer> clones must be dropped before
    // Arc::try_unwrap(store) below. The acceptor task's join loop ensures this.
    if let Some(handle) = handles.mcp_acceptor_handle.take():
        handle.abort()
        match tokio::time::timeout(Duration::from_secs(35), handle).await:
            Ok(_) => tracing::info!("MCP acceptor task finished")
            Err(_) => tracing::warn!("MCP acceptor task did not finish within timeout")

    // Step 0a: Drop MCP socket guard (NEW — vnc-005)
    // mcp_socket_guard drops BEFORE socket_guard (hook IPC) per drop ordering.
    // Removing unimatrix-mcp.sock first prevents a bridge's stale-check from
    // seeing the socket as present while the old daemon is still shutting down.
    drop(handles.mcp_socket_guard.take())

    // Step 0b: Stop hook IPC UDS listener (unchanged)
    if let Some(handle) = handles.uds_handle.take():
        handle.abort()
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await

    // Step 0c: Remove hook IPC socket guard (unchanged, now explicitly after mcp guard)
    drop(handles.socket_guard.take())

    // Step 0d: Abort background tick loop (unchanged)
    if let Some(handle) = handles.tick_handle.take():
        handle.abort()
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await
        tracing::info!("background tick loop stopped")

    // Step 1: Dump vector index (unchanged)
    tracing::info!("dumping vector index")
    match handles.vector_index.dump(&handles.vector_dir):
        Ok(()) => tracing::info!("vector index dumped successfully")
        Err(e) => tracing::warn!(error = %e, "vector dump failed, continuing shutdown")

    // Step 1b: Save adaptation state (unchanged)
    tracing::info!("saving adaptation state")
    match handles.adapt_service.save_state(&handles.data_dir):
        Ok(()) => tracing::info!("adaptation state saved successfully")
        Err(e) => tracing::warn!(error = %e, "adaptation state save failed, continuing shutdown")

    // Step 2: Drop all Arc<Store> holders (unchanged drop list, new ordering note)
    // ServiceLayer is dropped first (holds 5+ Arc<Store> clones via internal services).
    // By this point, all session task clones have been dropped (Step 0 joined them),
    // the tick is stopped (Step 0d), and the UDS listeners are gone.
    drop(handles.services.take())
    drop(handles.adapt_service)
    drop(handles.registry)
    drop(handles.audit)
    drop(handles.vector_index)

    // Step 3: Arc::try_unwrap(store) for compaction (unchanged)
    match Arc::try_unwrap(handles.store):
        Ok(mut store):
            tracing::info!("compacting database")
            match store.compact():
                Ok(()) => tracing::info!("database compacted successfully")
                Err(e) => tracing::warn!(error = %e, "compact failed, continuing exit")
        Err(_arc):
            tracing::warn!("skipping compact: outstanding Store references")

    // PidGuard drops in main.rs caller after this function returns.
    Ok(())
```

---

## Daemon Token Signal Handler (in `main.rs`)

The daemon startup path must use a `CancellationToken` (the "daemon token") instead of the
current direct `cancel_token` approach. The signal handler pattern changes:

### Current stdio pattern (main.rs lines 389-394, preserved for `serve --stdio`)

```
// For serve --stdio: keep existing behavior
let cancel_token = running.cancellation_token()
tokio::spawn(async move {
    shutdown::shutdown_signal().await
    cancel_token.cancel()
})
match running.waiting().await { ... }
graceful_shutdown(lifecycle_handles).await?   // ONLY call site
```

### New daemon pattern (in `tokio_main_daemon` or the daemon branch of `tokio_main`)

```
// Create the daemon-level CancellationToken
let daemon_token = CancellationToken::new()

// Signal handler cancels daemon token (not a per-session token)
let signal_token = daemon_token.clone()
tokio::spawn(async move {
    shutdown::shutdown_signal().await
    tracing::info!("received shutdown signal; cancelling daemon token")
    signal_token.cancel()
})

// Start MCP acceptor (accepts daemon_token clone)
let (mcp_acceptor_handle, mcp_socket_guard) =
    uds::mcp_listener::start_mcp_uds_listener(
        &paths.mcp_socket_path,
        server.clone(),
        daemon_token.clone(),
    ).await?

// Wait for daemon token to be cancelled (signal triggers this)
daemon_token.cancelled().await
tracing::info!("daemon token cancelled; beginning graceful shutdown")

// At this point: acceptor loop has broken, session join has begun inside acceptor task
// graceful_shutdown will abort the acceptor handle and wait for it to fully drain

graceful_shutdown(lifecycle_handles).await?   // ONLY call site
tracing::info!("unimatrix daemon exited cleanly")
```

The `QuitReason::Closed` path (session EOF in daemon mode) happens inside `run_session`
in `mcp_listener.rs` and does NOT call `graceful_shutdown`. This is the critical C-04
decoupling.

---

## Stdio Mode Preservation (R-12)

For `serve --stdio`, the `main.rs` path must retain the existing behavior:

```
// serve --stdio path (unchanged from pre-vnc-005):
let running = server.serve(rmcp::transport::io::stdio()).await?
let cancel_token = running.cancellation_token()
tokio::spawn(async move {
    shutdown::shutdown_signal().await
    cancel_token.cancel()
})
match running.waiting().await:
    Ok(reason) => tracing::info!(?reason, "stdio transport closed")
    Err(e) => tracing::error!(error = %e, "stdio transport error")

// QuitReason::Closed (stdin EOF) OR QuitReason::Cancelled (SIGTERM) both reach here
// and trigger graceful_shutdown for the stdio path.
graceful_shutdown(lifecycle_handles).await?   // still the ONLY call site (stdio branch)
```

Both the daemon branch and the stdio branch call `graceful_shutdown` once. They are in
separate code paths — only one path is entered per process lifetime. The gate requirement
(C-05) is that grep for `graceful_shutdown(` finds exactly two call sites after refactor:
one in the stdio branch, one in the daemon branch.

---

## Key Test Scenarios

1. **Drop ordering: mcp_socket_guard before socket_guard** — in a test harness, build
   `LifecycleHandles` with both guards; run `graceful_shutdown`; assert `mcp_socket_path`
   is deleted before `socket_path` (via `Drop` impl side effects).

2. **Arc::try_unwrap succeeds after session drain** (R-01) — open 4 MCP sessions; cancel
   daemon token; run `graceful_shutdown`; assert `Arc::strong_count(&store) == 1` at
   Step 3 entry; assert compaction runs.

3. **Graceful shutdown completes when N sessions active** — with N=1 and N=4 active
   sessions at SIGTERM time; assert shutdown completes without panic.

4. **Stdio mode: QuitReason::Closed still triggers shutdown** (R-12 / AC-12) — invoke
   `serve --stdio`; close stdin; assert daemon exits (graceful_shutdown called).

5. **Daemon mode: QuitReason::Closed does NOT trigger shutdown** (R-03 / AC-04) — invoke
   daemon; connect bridge A; close bridge A stdin; assert daemon continues running.

6. **Exactly one graceful_shutdown call site** (C-05) — static assertion: after
   implementation, grep `crates/unimatrix-server/src/ -r "graceful_shutdown("` must return
   exactly two lines: one in the stdio branch, one in the daemon branch of main.rs.

7. **mcp_socket_guard is gone after graceful_shutdown** (R-16) — start daemon; SIGTERM;
   wait for exit; assert `unimatrix-mcp.sock` does not exist on disk.

8. **socket_guard is also gone after graceful_shutdown** — same test; assert
   `unimatrix.sock` does not exist on disk.

9. **Existing shutdown integration test still passes** — `test_shutdown_drops_release_all_store_refs`
   in the existing `shutdown.rs` test module must continue to pass after the struct gains
   two new fields. Update it to also populate `mcp_socket_guard: None` and
   `mcp_acceptor_handle: None`.
