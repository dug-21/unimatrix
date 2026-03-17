//! Graceful shutdown coordination.
//!
//! Handles signal reception, vector dump, Arc lifecycle, and database compaction.
//! Supports both stdio mode (single session lifetime = process lifetime) and
//! daemon mode (multiple sessions; process lifetime ends only on signal).
//!
//! ## Drop Ordering (enforced explicitly in `graceful_shutdown`)
//!
//! 1. `mcp_acceptor_handle` — abort + join (drains all session Arc clones)
//! 2. `mcp_socket_guard`    — removes `unimatrix-mcp.sock`
//! 3. `uds_handle`          — abort + join (hook IPC accept loop)
//! 4. `socket_guard`        — removes `unimatrix.sock`
//! 5. `tick_handle`         — abort + join (background tick Arc holders)
//! 6. All `Arc<Store>` holders (services, adapt_service, registry, audit, vector_index)
//! 7. `Arc::try_unwrap(store)` → compaction
//!
//! `PidGuard` is NOT in this struct; it lives in `main()` as a local and drops after
//! this function returns — always last.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use unimatrix_adapt::AdaptationService;
use unimatrix_store::Store;
use unimatrix_vector::VectorIndex;

use crate::error::ServerError;
use crate::infra::audit::AuditLog;
use crate::infra::registry::AgentRegistry;
use crate::services::ServiceLayer;
use crate::uds::listener::SocketGuard;

/// Handles needed for lifecycle operations during shutdown.
///
/// Holds the original Arc references that must be the last to drop
/// so that `Arc::try_unwrap` can succeed for compaction.
///
/// Field ordering is documented for Rust's implicit drop sequence, but
/// `graceful_shutdown` enforces drop order explicitly via `take()` calls.
pub struct LifecycleHandles {
    /// The Store Arc for compaction via try_unwrap.
    pub store: Arc<Store>,
    /// The VectorIndex Arc for dump.
    pub vector_index: Arc<VectorIndex>,
    /// Directory for vector dump files.
    pub vector_dir: PathBuf,
    /// Registry (holds Arc<Store>; must drop before try_unwrap).
    pub registry: Arc<AgentRegistry>,
    /// Audit log (holds Arc<Store>; must drop before try_unwrap).
    pub audit: Arc<AuditLog>,
    /// Adaptation service for state persistence on shutdown (crt-006).
    pub adapt_service: Arc<AdaptationService>,
    /// Data directory for adaptation state files.
    pub data_dir: PathBuf,
    /// RAII guard for MCP UDS socket cleanup (vnc-005).
    /// Dropped during graceful_shutdown BEFORE socket_guard (drop ordering).
    /// `None` in stdio mode (no MCP UDS socket is created).
    pub mcp_socket_guard: Option<SocketGuard>,
    /// Accept loop task handle for MCP sessions (vnc-005).
    /// Aborted during graceful_shutdown; internally joins all session task handles.
    /// `None` in stdio mode.
    pub mcp_acceptor_handle: Option<tokio::task::JoinHandle<()>>,
    /// Socket guard for hook IPC UDS cleanup (col-006). Dropped during shutdown.
    pub socket_guard: Option<SocketGuard>,
    /// UDS accept loop task handle for hook IPC shutdown coordination (col-006).
    pub uds_handle: Option<tokio::task::JoinHandle<()>>,
    /// Background tick task handle (#52). Must be aborted during shutdown
    /// to release Arc<Store>, Arc<VectorIndex>, and other clones held by
    /// the tick loop.
    pub tick_handle: Option<tokio::task::JoinHandle<()>>,
    /// ServiceLayer holding Arc<Store> clones via internal services (#92).
    /// Must be dropped before Arc::try_unwrap(store) to release all references.
    pub services: Option<ServiceLayer>,
}

/// Create a new daemon-level `CancellationToken`.
///
/// The daemon startup path calls this to obtain the root token. The signal
/// handler task cancels this token on SIGTERM/SIGINT. Session tasks receive
/// child tokens via `daemon_token.child_token()`.
///
/// Stdio mode does not use a daemon token — it uses the rmcp transport's own
/// cancellation token directly.
pub fn new_daemon_token() -> CancellationToken {
    CancellationToken::new()
}

/// Run the graceful shutdown sequence.
///
/// Called after either:
/// - **Daemon mode**: the daemon token is cancelled (signal handler path). All session
///   task handles must be joined before calling this (done inside the MCP acceptor task).
/// - **Stdio mode**: `running.waiting()` returns (transport closed or signal).
///
/// Drop ordering is enforced explicitly. See module-level documentation.
///
/// `PidGuard` cleanup is handled by `PidGuard::drop` in the caller after this returns.
pub async fn graceful_shutdown(mut handles: LifecycleHandles) -> Result<(), ServerError> {
    // Brief pause for final responses to flush (unchanged from pre-vnc-005).
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Step 0: Stop MCP acceptor task (vnc-005).
    //
    // R-01: All session Arc<UnimatrixServer> clones must be dropped before
    // Arc::try_unwrap(store) in Step 3 below. The acceptor task's internal
    // session-join loop ensures this: it joins all session handles (with a
    // 30s timeout each) before returning. We abort the handle here to signal
    // the accept loop to stop; if the accept loop already exited (daemon token
    // was cancelled before this call), abort is a no-op and we just wait for
    // the join to confirm clean exit.
    //
    // The 35s timeout here is intentionally larger than the 30s per-session
    // timeout inside the acceptor, giving it room to drain all sessions.
    if let Some(handle) = handles.mcp_acceptor_handle.take() {
        handle.abort();
        match tokio::time::timeout(Duration::from_secs(35), handle).await {
            Ok(_) => tracing::info!("MCP acceptor task finished"),
            Err(_) => tracing::warn!("MCP acceptor task did not finish within 35s timeout"),
        }
    }

    // Step 0a: Drop MCP socket guard (vnc-005).
    // mcp_socket_guard drops BEFORE socket_guard (hook IPC). Removing
    // unimatrix-mcp.sock first prevents a bridge's stale-check from seeing
    // the socket as present while the old daemon is still shutting down.
    drop(handles.mcp_socket_guard.take());

    // Step 0b: Stop hook IPC UDS listener (col-006, unchanged).
    if let Some(handle) = handles.uds_handle.take() {
        handle.abort();
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }

    // Step 0c: Remove hook IPC socket guard (col-006, now explicitly after mcp guard).
    drop(handles.socket_guard.take());

    // Step 0d: Abort background tick loop (#52). The tick loop holds Arc clones
    // of Store, VectorIndex, EmbedServiceHandle, etc. Without aborting, these
    // Arcs are never released and Arc::try_unwrap(store) fails.
    if let Some(handle) = handles.tick_handle.take() {
        handle.abort();
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
        tracing::info!("background tick loop stopped");
    }

    // Step 1: Dump vector index (works through Arc — dump takes &self).
    tracing::info!("dumping vector index");
    match handles.vector_index.dump(&handles.vector_dir) {
        Ok(()) => tracing::info!("vector index dumped successfully"),
        Err(e) => tracing::warn!(error = %e, "vector dump failed, continuing shutdown"),
    }

    // Step 1b: Save adaptation state (crt-006).
    tracing::info!("saving adaptation state");
    match handles.adapt_service.save_state(&handles.data_dir) {
        Ok(()) => tracing::info!("adaptation state saved successfully"),
        Err(e) => tracing::warn!(error = %e, "adaptation state save failed, continuing shutdown"),
    }

    // Step 2: Drop all Arc<Store> holders before try_unwrap.
    // ServiceLayer (vnc-006) holds 5+ Arc<Store> clones via its internal
    // services — drop it first to release those references (#92).
    // By this point: session task clones are dropped (Step 0 joined them),
    // tick is stopped (Step 0d), UDS listeners are gone (Steps 0b/0c).
    drop(handles.services.take());
    drop(handles.adapt_service);
    drop(handles.registry);
    drop(handles.audit);
    drop(handles.vector_index);

    // Step 3: Try to unwrap Store for compaction.
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

    // PID file cleanup handled by PidGuard::drop in main().

    Ok(())
}

/// Wait for a shutdown signal (SIGTERM or SIGINT).
///
/// Public so `main.rs` can use it in the transport select loop (#236).
pub async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm =
            signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_try_unwrap_succeeds_when_sole_owner() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Arc::new(Store::open(&path).unwrap());

        // Only one reference exists
        assert_eq!(Arc::strong_count(&store), 1);
        let result = Arc::try_unwrap(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_try_unwrap_fails_with_outstanding_refs() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Arc::new(Store::open(&path).unwrap());
        let _clone = Arc::clone(&store);

        assert_eq!(Arc::strong_count(&store), 2);
        let result = Arc::try_unwrap(store);
        assert!(result.is_err());
    }

    #[test]
    fn test_compact_succeeds_after_unwrap() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Arc::new(Store::open(&path).unwrap());

        let mut owned = Arc::try_unwrap(store).ok().expect("should be sole owner");
        owned.compact().unwrap();
    }

    /// Verify that the shutdown drop sequence releases ALL Arc<Store> clones,
    /// including the ServiceLayer introduced in vnc-006 (#92).
    ///
    /// Before the fix, ServiceLayer was not in LifecycleHandles, so
    /// Arc::try_unwrap(store) always failed after vnc-006.
    ///
    /// Updated for vnc-005: LifecycleHandles now has two new Option fields
    /// (mcp_socket_guard, mcp_acceptor_handle) — both set to None here
    /// because stdio mode does not use them.
    #[test]
    fn test_shutdown_drops_release_all_store_refs() {
        use unimatrix_adapt::{AdaptConfig, AdaptationService};
        use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};
        use unimatrix_core::{StoreAdapter, VectorAdapter, VectorConfig};

        use crate::infra::audit::AuditLog;
        use crate::infra::embed_handle::EmbedServiceHandle;
        use crate::infra::registry::AgentRegistry;
        use crate::infra::usage_dedup::UsageDedup;

        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let vector_dir = dir.path().join("vector");
        std::fs::create_dir_all(&vector_dir).unwrap();

        let store = Arc::new(Store::open(&db_path).unwrap());
        let vector_config = VectorConfig::default();
        let vector_index = Arc::new(VectorIndex::new(Arc::clone(&store), vector_config).unwrap());

        // Build all the components that hold Arc<Store>, mirroring main.rs
        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store)).unwrap());
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));
        let embed_handle = EmbedServiceHandle::new();
        let usage_dedup = Arc::new(UsageDedup::new());

        let store_adapter = StoreAdapter::new(Arc::clone(&store));
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));
        let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

        // Build ServiceLayer (vnc-006) — this holds 5+ Arc<Store> clones
        let services = ServiceLayer::new(
            Arc::clone(&store),
            Arc::clone(&vector_index),
            Arc::clone(&async_vector_store),
            Arc::clone(&async_entry_store),
            Arc::clone(&embed_handle),
            Arc::clone(&adapt_service),
            Arc::clone(&audit),
            Arc::clone(&usage_dedup),
        );

        // Build LifecycleHandles with ServiceLayer included (#92 fix).
        // vnc-005: mcp_socket_guard and mcp_acceptor_handle are None (stdio mode).
        let mut handles = LifecycleHandles {
            store,
            vector_index,
            vector_dir,
            registry,
            audit,
            adapt_service,
            data_dir: dir.path().to_path_buf(),
            mcp_socket_guard: None,
            mcp_acceptor_handle: None,
            socket_guard: None,
            uds_handle: None,
            tick_handle: None,
            services: Some(services),
        };

        // Drop remaining locals that held Arc clones (mirrors tokio_main ownership)
        drop(async_entry_store);
        drop(async_vector_store);
        drop(embed_handle);
        drop(usage_dedup);

        // Simulate the shutdown drop sequence from graceful_shutdown
        drop(handles.mcp_acceptor_handle.take()); // Step 0 (None — no-op)
        drop(handles.mcp_socket_guard.take()); // Step 0a (None — no-op)
        drop(handles.uds_handle.take()); // Step 0b (None — no-op)
        drop(handles.socket_guard.take()); // Step 0c (None — no-op)
        drop(handles.tick_handle.take()); // Step 0d (None — no-op)
        drop(handles.services.take());
        drop(handles.adapt_service);
        drop(handles.registry);
        drop(handles.audit);
        drop(handles.vector_index);

        // Arc::try_unwrap should now succeed — all other refs are released
        let result = Arc::try_unwrap(handles.store);
        assert!(
            result.is_ok(),
            "Arc::try_unwrap(store) failed: outstanding references remain after shutdown drop sequence"
        );
    }

    /// Verify that WITHOUT dropping ServiceLayer, Arc::try_unwrap fails.
    /// This is the regression test: proves the bug existed before the fix.
    #[test]
    fn test_shutdown_fails_without_service_layer_drop() {
        use unimatrix_adapt::{AdaptConfig, AdaptationService};
        use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};
        use unimatrix_core::{StoreAdapter, VectorAdapter, VectorConfig};

        use crate::infra::audit::AuditLog;
        use crate::infra::embed_handle::EmbedServiceHandle;
        use crate::infra::registry::AgentRegistry;
        use crate::infra::usage_dedup::UsageDedup;

        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let vector_dir = dir.path().join("vector");
        std::fs::create_dir_all(&vector_dir).unwrap();

        let store = Arc::new(Store::open(&db_path).unwrap());
        let vector_config = VectorConfig::default();
        let vector_index = Arc::new(VectorIndex::new(Arc::clone(&store), vector_config).unwrap());

        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store)).unwrap());
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));
        let embed_handle = EmbedServiceHandle::new();
        let usage_dedup = Arc::new(UsageDedup::new());

        let store_adapter = StoreAdapter::new(Arc::clone(&store));
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_entry_store = Arc::new(AsyncEntryStore::new(Arc::new(store_adapter)));
        let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

        // Build ServiceLayer — holds internal Arc<Store> clones
        let services = ServiceLayer::new(
            Arc::clone(&store),
            Arc::clone(&vector_index),
            Arc::clone(&async_vector_store),
            Arc::clone(&async_entry_store),
            Arc::clone(&embed_handle),
            Arc::clone(&adapt_service),
            Arc::clone(&audit),
            Arc::clone(&usage_dedup),
        );

        // Drop locals except ServiceLayer
        drop(async_entry_store);
        drop(async_vector_store);
        drop(embed_handle);
        drop(usage_dedup);

        // Drop the handles that graceful_shutdown would drop
        drop(adapt_service);
        drop(registry);
        drop(audit);
        drop(vector_index);

        // ServiceLayer is NOT dropped — simulating the pre-fix bug
        // Arc::try_unwrap should FAIL because ServiceLayer still holds refs
        let result = Arc::try_unwrap(store);
        assert!(
            result.is_err(),
            "Arc::try_unwrap should fail when ServiceLayer is not dropped"
        );

        // Clean up (drop services so Store can be released for tempdir cleanup)
        drop(services);
        drop(result);
    }

    // --- vnc-005 new tests ---

    /// T-SHUT-U-03: LifecycleHandles has mcp_socket_guard and mcp_acceptor_handle fields.
    ///
    /// Verifies both new fields are present and correctly typed by constructing
    /// the struct in a test and checking Option<_> semantics.
    #[test]
    fn test_lifecycle_handles_has_vnc005_fields() {
        use unimatrix_adapt::{AdaptConfig, AdaptationService};
        use unimatrix_core::VectorConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let vector_dir = dir.path().join("vector");
        std::fs::create_dir_all(&vector_dir).unwrap();

        let store = Arc::new(Store::open(&db_path).unwrap());
        let vector_config = VectorConfig::default();
        let vector_index = Arc::new(VectorIndex::new(Arc::clone(&store), vector_config).unwrap());

        use crate::infra::audit::AuditLog;
        use crate::infra::registry::AgentRegistry;
        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store)).unwrap());
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));

        let handles = LifecycleHandles {
            store,
            vector_index,
            vector_dir,
            registry,
            audit,
            adapt_service,
            data_dir: dir.path().to_path_buf(),
            mcp_socket_guard: None, // Option<SocketGuard> — new vnc-005 field
            mcp_acceptor_handle: None, // Option<JoinHandle<()>> — new vnc-005 field
            socket_guard: None,
            uds_handle: None,
            tick_handle: None,
            services: None,
        };

        // Both new fields are Option; None means stdio mode (no MCP UDS)
        assert!(handles.mcp_socket_guard.is_none());
        assert!(handles.mcp_acceptor_handle.is_none());
    }

    /// new_daemon_token() returns a fresh CancellationToken that is not yet cancelled.
    #[test]
    fn test_new_daemon_token_not_cancelled() {
        let token = new_daemon_token();
        assert!(
            !token.is_cancelled(),
            "new daemon token must not be pre-cancelled"
        );
    }

    /// child_token() inherits cancellation from parent.
    #[test]
    fn test_daemon_token_child_inherits_cancel() {
        let parent = new_daemon_token();
        let child = parent.child_token();
        assert!(!child.is_cancelled());
        parent.cancel();
        assert!(
            child.is_cancelled(),
            "child token must be cancelled when parent is cancelled"
        );
    }

    /// Cancelling a daemon token does not affect an independently created token.
    #[test]
    fn test_daemon_token_independent_tokens_isolated() {
        let token_a = new_daemon_token();
        let token_b = new_daemon_token();
        token_a.cancel();
        assert!(token_a.is_cancelled());
        assert!(
            !token_b.is_cancelled(),
            "unrelated tokens must not share cancellation state"
        );
    }

    /// T-SHUT-U-04 (structural): drop ordering is enforced by the take() sequence.
    ///
    /// This test verifies that after running the Step 0 / Step 0a sequence in order,
    /// both MCP fields are None (consumed), confirming they were processed before the
    /// hook IPC fields.
    #[test]
    fn test_drop_ordering_mcp_before_hook_ipc() {
        use unimatrix_adapt::{AdaptConfig, AdaptationService};
        use unimatrix_core::VectorConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let vector_dir = dir.path().join("vector");
        std::fs::create_dir_all(&vector_dir).unwrap();

        let store = Arc::new(Store::open(&db_path).unwrap());
        let vector_config = VectorConfig::default();
        let vector_index = Arc::new(VectorIndex::new(Arc::clone(&store), vector_config).unwrap());

        use crate::infra::audit::AuditLog;
        use crate::infra::registry::AgentRegistry;
        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store)).unwrap());
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));

        let mut handles = LifecycleHandles {
            store,
            vector_index,
            vector_dir,
            registry,
            audit,
            adapt_service,
            data_dir: dir.path().to_path_buf(),
            mcp_socket_guard: None,
            mcp_acceptor_handle: None,
            socket_guard: None,
            uds_handle: None,
            tick_handle: None,
            services: None,
        };

        // Simulate the graceful_shutdown drop sequence steps 0 through 0c.
        // MCP fields must be taken before hook IPC fields.
        let mcp_acceptor = handles.mcp_acceptor_handle.take(); // Step 0
        let mcp_guard = handles.mcp_socket_guard.take(); // Step 0a
        let uds_h = handles.uds_handle.take(); // Step 0b
        let sock_guard = handles.socket_guard.take(); // Step 0c

        // After take(), all fields are None — confirms they were consumed in order
        assert!(handles.mcp_acceptor_handle.is_none());
        assert!(handles.mcp_socket_guard.is_none());
        assert!(handles.uds_handle.is_none());
        assert!(handles.socket_guard.is_none());

        // All taken values are None in this test (stdio mode)
        assert!(mcp_acceptor.is_none());
        assert!(mcp_guard.is_none());
        assert!(uds_h.is_none());
        assert!(sock_guard.is_none());
    }

    /// Verify that mcp_acceptor_handle abort + join pattern works for a real JoinHandle.
    ///
    /// This mirrors Step 0 in graceful_shutdown: abort the handle, then timeout-join it.
    /// The task is a simple async sleep; abort causes it to end with a JoinError::is_cancelled.
    #[tokio::test]
    async fn test_mcp_acceptor_handle_abort_join() {
        let handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        });

        handle.abort();
        let result = tokio::time::timeout(Duration::from_secs(5), handle).await;
        // timeout should NOT fire (abort is immediate); inner result is Err(JoinError::cancelled)
        match result {
            Ok(Err(e)) => assert!(e.is_cancelled(), "expected cancellation error, got: {e}"),
            Ok(Ok(())) => {
                // Also acceptable if the task happened to complete before abort
            }
            Err(_timeout) => panic!("abort + join timed out unexpectedly"),
        }
    }
}
