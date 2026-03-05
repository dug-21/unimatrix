//! Graceful shutdown coordination.
//!
//! Handles signal reception, vector dump, Arc lifecycle, and database compaction.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use unimatrix_adapt::AdaptationService;
use unimatrix_store::Store;
use unimatrix_vector::VectorIndex;

use crate::infra::audit::AuditLog;
use crate::error::ServerError;
use crate::infra::registry::AgentRegistry;
use crate::services::ServiceLayer;
use crate::uds::listener::SocketGuard;

/// Handles needed for lifecycle operations during shutdown.
///
/// Holds the original Arc references that must be the last to drop
/// so that `Arc::try_unwrap` can succeed for compaction.
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
    /// Socket guard for UDS cleanup (col-006). Dropped during shutdown.
    pub socket_guard: Option<SocketGuard>,
    /// UDS accept loop task handle for shutdown coordination (col-006).
    pub uds_handle: Option<tokio::task::JoinHandle<()>>,
    /// ServiceLayer holding Arc<Store> clones via internal services (#92).
    /// Must be dropped before Arc::try_unwrap(store) to release all references.
    pub services: Option<ServiceLayer>,
}

/// Run the graceful shutdown sequence.
///
/// Waits for the MCP session to close or a signal, then:
/// 1. Dumps the vector index
/// 2. Drops all Arc<Store> clones
/// 3. Attempts to compact the database
///
/// PID file cleanup is handled by `PidGuard::drop` in the caller.
pub async fn graceful_shutdown<S>(
    mut handles: LifecycleHandles,
    server: S,
) -> Result<(), ServerError>
where
    S: std::future::Future<Output = ()>,
{
    // Wait for session close or signal
    tokio::select! {
        _ = server => {
            tracing::info!("MCP session closed");
        }
        _ = shutdown_signal() => {
            tracing::info!("received shutdown signal");
        }
    }

    // Brief pause for final responses to flush
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Step 0: Stop UDS listener (col-006)
    if let Some(handle) = handles.uds_handle.take() {
        handle.abort();
        // Wait up to 1s for in-flight handlers to complete
        let _ = tokio::time::timeout(Duration::from_secs(1), handle).await;
    }

    // Step 0b: Remove socket file via SocketGuard drop (col-006)
    drop(handles.socket_guard.take());

    // Step 1: Dump vector index (works through Arc — dump takes &self)
    tracing::info!("dumping vector index");
    match handles.vector_index.dump(&handles.vector_dir) {
        Ok(()) => tracing::info!("vector index dumped successfully"),
        Err(e) => tracing::warn!(error = %e, "vector dump failed, continuing shutdown"),
    }

    // Step 1b: Save adaptation state (crt-006)
    tracing::info!("saving adaptation state");
    match handles.adapt_service.save_state(&handles.data_dir) {
        Ok(()) => tracing::info!("adaptation state saved successfully"),
        Err(e) => tracing::warn!(error = %e, "adaptation state save failed, continuing shutdown"),
    }

    // Step 2: Drop all Arc<Store> holders before try_unwrap.
    // ServiceLayer (vnc-006) holds 5+ Arc<Store> clones via its internal
    // services — drop it first to release those references (#92).
    drop(handles.services.take());
    drop(handles.adapt_service);
    drop(handles.registry);
    drop(handles.audit);
    drop(handles.vector_index);

    // Step 3: Try to unwrap Store for compaction
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
async fn shutdown_signal() {
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
    #[test]
    fn test_shutdown_drops_release_all_store_refs() {
        use unimatrix_adapt::{AdaptConfig, AdaptationService};
        use unimatrix_core::{StoreAdapter, VectorAdapter, VectorConfig};
        use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};

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
        let vector_index = Arc::new(
            VectorIndex::new(Arc::clone(&store), vector_config).unwrap(),
        );

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

        // Build LifecycleHandles with ServiceLayer included (#92 fix)
        let mut handles = LifecycleHandles {
            store,
            vector_index,
            vector_dir,
            registry,
            audit,
            adapt_service,
            data_dir: dir.path().to_path_buf(),
            socket_guard: None,
            uds_handle: None,
            services: Some(services),
        };

        // Drop remaining locals that held Arc clones (mirrors tokio_main ownership)
        drop(async_entry_store);
        drop(async_vector_store);
        drop(embed_handle);
        drop(usage_dedup);

        // Simulate the shutdown drop sequence from graceful_shutdown
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
        use unimatrix_core::{StoreAdapter, VectorAdapter, VectorConfig};
        use unimatrix_core::async_wrappers::{AsyncEntryStore, AsyncVectorStore};

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
        let vector_index = Arc::new(
            VectorIndex::new(Arc::clone(&store), vector_config).unwrap(),
        );

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
}
