# Pseudocode: shutdown.rs (C9 — Shutdown Coordinator)

## Purpose

Orchestrates graceful shutdown following the resolved sequence from ADR-005. Handles SIGTERM/SIGINT, vector dump, Arc lifecycle, and optional compact.

## Types

```
struct LifecycleHandles {
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    vector_dir: PathBuf,
    registry: Arc<AgentRegistry>,
    audit: Arc<AuditLog>,
}
```

## Functions

### graceful_shutdown(handles: LifecycleHandles, server: RunningService) -> Result<(), ServerError>

```
// Step 1: Wait for either MCP session close or signal
// server.waiting() resolves when the MCP client disconnects
// signal future resolves on SIGTERM or SIGINT

tokio::select! {
    _ = server.waiting() => {
        tracing::info!("MCP session closed");
    }
    _ = shutdown_signal() => {
        tracing::info!("received shutdown signal");
        // Cancel the server to stop accepting new requests
        server.cancel();
    }
}

// Step 2: Wait briefly for in-flight requests (bounded timeout)
tokio::time::sleep(Duration::from_millis(100)).await;
// Note: rmcp handles draining internally on cancel.
// The 5-second timeout from the spec is a safety bound on the entire shutdown,
// not a separate drain step. We add a small sleep to allow final responses to flush.

// Step 3: Dump vector index (works through Arc — dump takes &self)
tracing::info!("dumping vector index");
match handles.vector_index.dump(&handles.vector_dir) {
    Ok(()) => tracing::info!("vector index dumped successfully"),
    Err(e) => tracing::warn!(error = %e, "vector dump failed, continuing shutdown"),
}

// Step 4: Drop all Arc clones that hold Arc<Store> references
// At this point, the RunningService (and its server clone) have been dropped.
// Now explicitly drop registry and audit (they hold Arc<Store> clones).
drop(handles.registry);
drop(handles.audit);
// Drop vector index (it also holds Arc<Store> internally)
drop(handles.vector_index);

// Step 5: Try to unwrap Store for compact
match Arc::try_unwrap(handles.store) {
    Ok(mut store) => {
        tracing::info!("compacting database");
        match store.compact() {
            Ok(()) => tracing::info!("database compacted successfully"),
            Err(e) => tracing::warn!(error = %e, "compact failed, continuing exit"),
        }
    }
    Err(_arc) => {
        tracing::warn!("skipping compact: outstanding Store references");
    }
}

Ok(())
```

### shutdown_signal() -> impl Future<Output = ()>

```
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate())
            .expect("failed to register SIGTERM handler");

        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }
}
```

## Design Considerations

1. **Ordering matters**: dump before drop, drop before try_unwrap, try_unwrap before compact
2. **Vector dump failure is non-fatal**: log warning but continue (FR-12d)
3. **Compact failure is non-fatal**: log warning but exit 0 (FR-12e)
4. **try_unwrap failure is expected**: if any Arc<Store> reference leaked, log and skip compact
5. **The server Drop happens implicitly**: when `graceful_shutdown` is called from main, the RunningService is consumed. The UnimatrixServer clone inside it is dropped, releasing its Arc clones.

## Integration with main.rs

```
// In main.rs, the flow is:
let running = server.serve(stdio()).await?;
graceful_shutdown(lifecycle_handles, running).await?;
// Process exits after this
```

The `LifecycleHandles` are created in main.rs from the original Arc references, BEFORE those references are cloned into the server and adapters.

## Error Handling

- Vector dump errors: logged as warnings, not propagated
- Compact errors: logged as warnings, not propagated
- Signal registration failure: panics (acceptable at startup — no recovery possible)
- The function returns `Result<(), ServerError>` but in practice always returns `Ok(())`

## Key Test Scenarios

1. Server exits cleanly when MCP session closes
2. Vector dump files are created during shutdown
3. If vector dump fails (read-only dir), shutdown continues
4. Arc::try_unwrap succeeds when all references are properly dropped
5. If try_unwrap fails, warning logged and exit continues
6. Compact is called when try_unwrap succeeds
7. Shutdown completes within 10 seconds (NFR-03)
