//! Graceful shutdown coordination.
//!
//! Handles signal reception, vector dump, Arc lifecycle, and database compaction.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use unimatrix_store::Store;
use unimatrix_vector::VectorIndex;

use crate::audit::AuditLog;
use crate::error::ServerError;
use crate::registry::AgentRegistry;

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
    /// PID file path for cleanup on exit.
    pub pid_path: PathBuf,
}

/// Run the graceful shutdown sequence.
///
/// Waits for the MCP session to close or a signal, then:
/// 1. Dumps the vector index
/// 2. Drops all Arc<Store> clones
/// 3. Attempts to compact the database
/// 4. Removes the PID file
pub async fn graceful_shutdown<S>(
    handles: LifecycleHandles,
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

    // Step 1: Dump vector index (works through Arc — dump takes &self)
    tracing::info!("dumping vector index");
    match handles.vector_index.dump(&handles.vector_dir) {
        Ok(()) => tracing::info!("vector index dumped successfully"),
        Err(e) => tracing::warn!(error = %e, "vector dump failed, continuing shutdown"),
    }

    // Step 2: Drop all Arc<Store> holders before try_unwrap
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

    // Step 4: Remove PID file
    crate::pidfile::remove_pid_file(&handles.pid_path);
    tracing::info!("PID file removed");

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
        let path = dir.path().join("test.redb");
        let store = Arc::new(Store::open(&path).unwrap());

        // Only one reference exists
        assert_eq!(Arc::strong_count(&store), 1);
        let result = Arc::try_unwrap(store);
        assert!(result.is_ok());
    }

    #[test]
    fn test_try_unwrap_fails_with_outstanding_refs() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Arc::new(Store::open(&path).unwrap());
        let _clone = Arc::clone(&store);

        assert_eq!(Arc::strong_count(&store), 2);
        let result = Arc::try_unwrap(store);
        assert!(result.is_err());
    }

    #[test]
    fn test_compact_succeeds_after_unwrap() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Arc::new(Store::open(&path).unwrap());

        let mut owned = Arc::try_unwrap(store).ok().expect("should be sole owner");
        owned.compact().unwrap();
    }
}
