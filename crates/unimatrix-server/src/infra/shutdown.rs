//! Graceful shutdown coordination.
//!
//! Handles signal reception, vector dump, Arc lifecycle, and database compaction.
//! Supports both stdio mode (single session lifetime = process lifetime) and
//! daemon mode (multiple sessions; process lifetime ends only on signal).
//!
//! ## Drop Ordering (enforced explicitly in `graceful_shutdown`)
//!
//! 1. `mcp_acceptor_handle`  — abort + join (drains all session Arc clones)
//!    1b. `http_acceptor_handle` — abort + join (HTTP accept loop, vnc-021)
//! 2. `mcp_socket_guard`     — removes `unimatrix-mcp.sock`
//! 3. `uds_handle`           — abort + join (hook IPC accept loop)
//! 4. `socket_guard`         — removes `unimatrix.sock`
//! 5. `tick_handle`          — abort + join (background tick Arc holders)
//!    5a. daemon + per-slug vector dumps (#823) — dump before the Arc drops below
//! 6. All `Arc<Store>` holders (services, adapt_service, registry, audit, vector_index, per_slug_vectors)
//! 7. `Arc::try_unwrap(store)` → compaction
//!
//! `PidGuard` is NOT in this struct; it lives in `main()` as a local and drops after
//! this function returns — always last.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use unimatrix_adapt::AdaptationService;
use unimatrix_store::SqlxStore;
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
    pub store: Arc<SqlxStore>,
    /// The VectorIndex Arc for dump.
    pub vector_index: Arc<VectorIndex>,
    /// Directory for vector dump files.
    pub vector_dir: PathBuf,
    /// Per-slug `(VectorIndex, dir)` pairs to dump on shutdown (#823).
    ///
    /// In multi-project HTTP mode each registered slug has its OWN in-memory
    /// `VectorIndex` over `{base}/{slug}/vector`. These were never registered
    /// for the shutdown dump, so the per-slug HNSW index was lost on restart
    /// (semantic search silently degraded). Each pair is dumped in Step 1
    /// (warn-and-continue per entry) BEFORE the Step 2 Arc drops. Empty in
    /// single-project / stdio mode (the daemon pair above covers those).
    pub per_slug_vectors: Vec<(Arc<VectorIndex>, PathBuf)>,
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
    /// HTTP accept loop task handle (vnc-021).
    /// Aborted during graceful_shutdown between MCP acceptor (Step 0) and hook IPC (Step 0b).
    /// `None` when HTTP is disabled or in stdio mode.
    pub http_acceptor_handle: Option<tokio::task::JoinHandle<()>>,
    /// HTTP listener bound address (vnc-021).
    /// Stored for logging/debugging. `None` when HTTP is disabled.
    pub http_listener_addr: Option<SocketAddr>,
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

    // Step 0-http: Stop HTTP acceptor task (vnc-021).
    // Placed after MCP acceptor to match architecture's specified ordering.
    // The HTTP accept loop stops accepting new connections when its
    // CancellationToken is cancelled (which happens when the daemon token
    // is cancelled, before graceful_shutdown is called). This abort + join
    // ensures the accept loop task itself is cleaned up.
    if let Some(handle) = handles.http_acceptor_handle.take() {
        handle.abort();
        match tokio::time::timeout(Duration::from_secs(35), handle).await {
            Ok(_) => tracing::info!("HTTP acceptor task finished"),
            Err(_) => tracing::warn!("HTTP acceptor task did not finish within 35s timeout"),
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

    // Step 1-slug: Dump each per-slug vector index to its OWN dir (#823).
    // Multi-project HTTP mode only — empty in single-project / stdio.
    // Warn-and-continue PER ENTRY (N1): one bad slug dir cannot abort the
    // others, nor block the daemon `try_unwrap` in Step 3. Same step band as the
    // daemon dump above (BEFORE the Step 2 Arc drops) and the same dump idiom.
    // These Arcs hold per-slug stores (NOT the daemon `store` Step 3 unwraps), so
    // they add no try_unwrap hazard — they drop with `handles` at function end.
    for (index, dir) in &handles.per_slug_vectors {
        match index.dump(dir) {
            Ok(()) => {
                tracing::info!(dir = %dir.display(), "per-slug vector index dumped successfully")
            }
            Err(e) => tracing::warn!(
                error = %e,
                dir = %dir.display(),
                "per-slug vector dump failed, continuing shutdown"
            ),
        }
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
    // #823: release the per-slug index Arcs (they hold per-slug stores, not the
    // daemon store, so this is independent of the Step 3 try_unwrap below).
    handles.per_slug_vectors.clear();

    // Step 3: Try to unwrap Store for compaction.
    match Arc::try_unwrap(handles.store) {
        Ok(store) => {
            tracing::info!("compacting database");
            match store.compact().await {
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
    use unimatrix_store::pool_config::PoolConfig;

    async fn open_store(path: &std::path::Path) -> Arc<SqlxStore> {
        Arc::new(
            SqlxStore::open(path, PoolConfig::default())
                .await
                .expect("open store"),
        )
    }

    #[tokio::test]
    async fn test_try_unwrap_succeeds_when_sole_owner() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = open_store(&path).await;

        // Only one reference exists
        assert_eq!(Arc::strong_count(&store), 1);
        let result = Arc::try_unwrap(store);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_try_unwrap_fails_with_outstanding_refs() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = open_store(&path).await;
        let _clone = Arc::clone(&store);

        assert_eq!(Arc::strong_count(&store), 2);
        let result = Arc::try_unwrap(store);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_compact_succeeds_after_unwrap() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = open_store(&path).await;

        let owned = Arc::try_unwrap(store).unwrap_or_else(|_| panic!("should be sole owner"));
        owned.compact().await.unwrap();
    }

    /// Verify that the shutdown drop sequence releases ALL Arc<Store> clones.
    #[tokio::test]
    async fn test_shutdown_drops_release_all_store_refs() {
        use unimatrix_adapt::{AdaptConfig, AdaptationService};
        use unimatrix_core::async_wrappers::AsyncVectorStore;
        use unimatrix_core::{VectorAdapter, VectorConfig};

        use crate::infra::audit::AuditLog;
        use crate::infra::embed_handle::EmbedServiceHandle;
        use crate::infra::registry::AgentRegistry;
        use crate::infra::usage_dedup::UsageDedup;

        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let vector_dir = dir.path().join("vector");
        std::fs::create_dir_all(&vector_dir).unwrap();

        let store = open_store(&db_path).await;
        let vector_config = VectorConfig::default();
        let vector_index = Arc::new(VectorIndex::new(Arc::clone(&store), vector_config).unwrap());

        // Build all the components that hold Arc<Store>, mirroring main.rs
        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store), true, vec![]).unwrap());
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));
        let embed_handle = EmbedServiceHandle::new();
        let usage_dedup = Arc::new(UsageDedup::new());

        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

        // Build ServiceLayer (vnc-006) — this holds 5+ Arc<Store> clones
        let test_pool = Arc::new(
            crate::infra::rayon_pool::RayonPool::new(1, "test-pool")
                .expect("test RayonPool construction must succeed"),
        );
        let services = ServiceLayer::new(
            Arc::clone(&store),
            Arc::clone(&vector_index),
            Arc::clone(&async_vector_store),
            Arc::clone(&store),
            Arc::clone(&embed_handle),
            Arc::clone(&adapt_service),
            Arc::clone(&audit),
            Arc::clone(&usage_dedup),
            crate::infra::config::default_boosted_categories_set(),
            test_pool,
            // crt-023: disabled NLI for test (no model in test env)
            crate::infra::nli_handle::NliServiceHandle::new(),
            20,    // nli_top_k default
            false, // nli_enabled: disabled for tests
            Arc::new(crate::infra::config::InferenceConfig::default()),
            // col-023: built-in default registry for test
            Arc::new(unimatrix_observe::domain::DomainPackRegistry::with_builtin_claude_code()),
            // GH #311: default params for tests.
            Arc::new(unimatrix_engine::confidence::ConfidenceParams::default()),
            // crt-031: default lifecycle policy for tests.
            Arc::new(crate::infra::categories::CategoryAllowlist::new()),
        );

        // Build LifecycleHandles with ServiceLayer included (#92 fix).
        // vnc-005: mcp_socket_guard and mcp_acceptor_handle are None (stdio mode).
        let mut handles = LifecycleHandles {
            store,
            vector_index,
            vector_dir,
            per_slug_vectors: Vec::new(),
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
            http_acceptor_handle: None,
            http_listener_addr: None,
        };

        // Drop remaining locals that held Arc clones (mirrors tokio_main ownership)
        drop(async_vector_store);
        drop(embed_handle);
        drop(usage_dedup);

        // Simulate the shutdown drop sequence from graceful_shutdown
        drop(handles.mcp_acceptor_handle.take()); // Step 0 (None — no-op)
        drop(handles.http_acceptor_handle.take()); // Step 0-http (None — no-op)
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
    #[tokio::test]
    async fn test_shutdown_fails_without_service_layer_drop() {
        use unimatrix_adapt::{AdaptConfig, AdaptationService};
        use unimatrix_core::async_wrappers::AsyncVectorStore;
        use unimatrix_core::{VectorAdapter, VectorConfig};

        use crate::infra::audit::AuditLog;
        use crate::infra::embed_handle::EmbedServiceHandle;
        use crate::infra::registry::AgentRegistry;
        use crate::infra::usage_dedup::UsageDedup;

        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let vector_dir = dir.path().join("vector");
        std::fs::create_dir_all(&vector_dir).unwrap();

        let store = open_store(&db_path).await;
        let vector_config = VectorConfig::default();
        let vector_index = Arc::new(VectorIndex::new(Arc::clone(&store), vector_config).unwrap());

        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store), true, vec![]).unwrap());
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));
        let embed_handle = EmbedServiceHandle::new();
        let usage_dedup = Arc::new(UsageDedup::new());

        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

        // Build ServiceLayer — holds internal Arc<Store> clones
        let test_pool2 = Arc::new(
            crate::infra::rayon_pool::RayonPool::new(1, "test-pool")
                .expect("test RayonPool construction must succeed"),
        );
        let services = ServiceLayer::new(
            Arc::clone(&store),
            Arc::clone(&vector_index),
            Arc::clone(&async_vector_store),
            Arc::clone(&store),
            Arc::clone(&embed_handle),
            Arc::clone(&adapt_service),
            Arc::clone(&audit),
            Arc::clone(&usage_dedup),
            crate::infra::config::default_boosted_categories_set(),
            test_pool2,
            // crt-023: disabled NLI for test (no model in test env)
            crate::infra::nli_handle::NliServiceHandle::new(),
            20,    // nli_top_k default
            false, // nli_enabled: disabled for tests
            Arc::new(crate::infra::config::InferenceConfig::default()),
            // col-023: built-in default registry for test
            Arc::new(unimatrix_observe::domain::DomainPackRegistry::with_builtin_claude_code()),
            // GH #311: default params for tests.
            Arc::new(unimatrix_engine::confidence::ConfidenceParams::default()),
            // crt-031: default lifecycle policy for tests.
            Arc::new(crate::infra::categories::CategoryAllowlist::new()),
        );

        // Drop locals except ServiceLayer
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
    #[tokio::test]
    async fn test_lifecycle_handles_has_vnc005_fields() {
        use unimatrix_adapt::{AdaptConfig, AdaptationService};
        use unimatrix_core::VectorConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let vector_dir = dir.path().join("vector");
        std::fs::create_dir_all(&vector_dir).unwrap();

        let store = open_store(&db_path).await;
        let vector_config = VectorConfig::default();
        let vector_index = Arc::new(VectorIndex::new(Arc::clone(&store), vector_config).unwrap());

        use crate::infra::audit::AuditLog;
        use crate::infra::registry::AgentRegistry;
        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store), true, vec![]).unwrap());
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));

        let handles = LifecycleHandles {
            store,
            vector_index,
            vector_dir,
            per_slug_vectors: Vec::new(),
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
            http_acceptor_handle: None, // vnc-021: None when HTTP disabled
            http_listener_addr: None,
        };

        // Both new fields are Option; None means stdio mode (no MCP UDS)
        assert!(handles.mcp_socket_guard.is_none());
        assert!(handles.mcp_acceptor_handle.is_none());
        // vnc-021: HTTP fields are None when HTTP disabled
        assert!(handles.http_acceptor_handle.is_none());
        assert!(handles.http_listener_addr.is_none());
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
    #[tokio::test]
    async fn test_drop_ordering_mcp_before_hook_ipc() {
        use unimatrix_adapt::{AdaptConfig, AdaptationService};
        use unimatrix_core::VectorConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let vector_dir = dir.path().join("vector");
        std::fs::create_dir_all(&vector_dir).unwrap();

        let store = open_store(&db_path).await;
        let vector_config = VectorConfig::default();
        let vector_index = Arc::new(VectorIndex::new(Arc::clone(&store), vector_config).unwrap());

        use crate::infra::audit::AuditLog;
        use crate::infra::registry::AgentRegistry;
        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store), true, vec![]).unwrap());
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));

        let mut handles = LifecycleHandles {
            store,
            vector_index,
            vector_dir,
            per_slug_vectors: Vec::new(),
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
            http_acceptor_handle: None,
            http_listener_addr: None,
        };

        // Simulate the graceful_shutdown drop sequence steps 0 through 0c.
        // MCP fields must be taken before hook IPC fields.
        // HTTP acceptor is taken between MCP acceptor and MCP socket guard (vnc-021).
        let mcp_acceptor = handles.mcp_acceptor_handle.take(); // Step 0
        let http_acceptor = handles.http_acceptor_handle.take(); // Step 0-http
        let mcp_guard = handles.mcp_socket_guard.take(); // Step 0a
        let uds_h = handles.uds_handle.take(); // Step 0b
        let sock_guard = handles.socket_guard.take(); // Step 0c

        // After take(), all fields are None — confirms they were consumed in order
        assert!(handles.mcp_acceptor_handle.is_none());
        assert!(handles.http_acceptor_handle.is_none());
        assert!(handles.mcp_socket_guard.is_none());
        assert!(handles.uds_handle.is_none());
        assert!(handles.socket_guard.is_none());

        // All taken values are None in this test (stdio mode)
        assert!(mcp_acceptor.is_none());
        assert!(http_acceptor.is_none());
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

    // --- vnc-021 tests ---

    /// T-LI-13: LifecycleHandles has http_acceptor_handle and http_listener_addr fields.
    /// Structural compile-time test.
    #[tokio::test]
    async fn test_lifecycle_handles_has_http_fields() {
        use unimatrix_adapt::{AdaptConfig, AdaptationService};
        use unimatrix_core::VectorConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let vector_dir = dir.path().join("vector");
        std::fs::create_dir_all(&vector_dir).unwrap();

        let store = open_store(&db_path).await;
        let vector_config = VectorConfig::default();
        let vector_index = Arc::new(VectorIndex::new(Arc::clone(&store), vector_config).unwrap());

        use crate::infra::audit::AuditLog;
        use crate::infra::registry::AgentRegistry;
        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store), true, vec![]).unwrap());
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));

        let handles = LifecycleHandles {
            store,
            vector_index,
            vector_dir,
            per_slug_vectors: Vec::new(),
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
            http_acceptor_handle: None,
            http_listener_addr: None,
        };

        // Structural: fields exist and are None when HTTP disabled
        assert!(handles.http_acceptor_handle.is_none());
        assert!(handles.http_listener_addr.is_none());
    }

    /// T-LI-14: LifecycleHandles stores a real JoinHandle for HTTP acceptor.
    #[tokio::test]
    async fn test_lifecycle_handles_stores_http_join_handle() {
        use unimatrix_adapt::{AdaptConfig, AdaptationService};
        use unimatrix_core::VectorConfig;

        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let vector_dir = dir.path().join("vector");
        std::fs::create_dir_all(&vector_dir).unwrap();

        let store = open_store(&db_path).await;
        let vector_config = VectorConfig::default();
        let vector_index = Arc::new(VectorIndex::new(Arc::clone(&store), vector_config).unwrap());

        use crate::infra::audit::AuditLog;
        use crate::infra::registry::AgentRegistry;
        let registry = Arc::new(AgentRegistry::new(Arc::clone(&store), true, vec![]).unwrap());
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));

        // Spawn a dummy async task to get a real JoinHandle.
        let dummy_handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        });

        let addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();

        let mut handles = LifecycleHandles {
            store,
            vector_index,
            vector_dir,
            per_slug_vectors: Vec::new(),
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
            http_acceptor_handle: Some(dummy_handle),
            http_listener_addr: Some(addr),
        };

        assert!(handles.http_acceptor_handle.is_some());
        assert_eq!(handles.http_listener_addr, Some(addr));

        // Abort to clean up
        if let Some(h) = handles.http_acceptor_handle.take() {
            h.abort();
            let _ = h.await;
        }
    }

    /// Verify HTTP acceptor abort + join pattern works (mirrors MCP acceptor test).
    #[tokio::test]
    async fn test_http_acceptor_handle_abort_join() {
        let handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        });

        handle.abort();
        let result = tokio::time::timeout(Duration::from_secs(5), handle).await;
        match result {
            Ok(Err(e)) => assert!(e.is_cancelled(), "expected cancellation error, got: {e}"),
            Ok(Ok(())) => {}
            Err(_timeout) => panic!("abort + join timed out unexpectedly"),
        }
    }

    // --- #823: per-slug vector dump regression ---

    /// Read `point_count` out of a dumped `unimatrix-vector.meta` (`-1` if the file
    /// is missing, so callers can distinguish "never dumped" from "dumped empty").
    fn meta_point_count(vector_dir: &std::path::Path) -> i64 {
        let meta_path = vector_dir.join("unimatrix-vector.meta");
        let Ok(text) = std::fs::read_to_string(&meta_path) else {
            return -1;
        };
        for line in text.lines() {
            if let Some(v) = line.strip_prefix("point_count=") {
                return v.trim().parse::<i64>().unwrap_or(-1);
            }
        }
        -1
    }

    /// Insert one store entry + its embedding into a `VectorIndex` (mirrors the
    /// vector crate's `seed_vectors` helper without pulling its test-support
    /// feature). Returns the seeded entry id and the embedding used.
    async fn seed_one_vector(
        store: &Arc<SqlxStore>,
        vi: &VectorIndex,
        dim: usize,
    ) -> (u64, Vec<f32>) {
        use unimatrix_store::{NewEntry, Status};

        let entry = NewEntry {
            title: "slug vector entry".to_string(),
            content: "content for the per-slug persistence round-trip".to_string(),
            topic: "test".to_string(),
            category: "vector".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: String::new(),
            feature_cycle: String::new(),
            trust_source: String::new(),
        };
        let entry_id = store.insert(entry).await.expect("insert store entry");

        // Deterministic unit vector (one hot dimension) — normalized, valid for HNSW.
        let mut embedding = vec![0.0f32; dim];
        embedding[0] = 1.0;
        vi.insert(entry_id, &embedding)
            .await
            .expect("insert vector");

        (entry_id, embedding)
    }

    /// #823 regression: in multi-project HTTP mode the per-slug `VectorIndex` must
    /// be dumped to its OWN `{slug}/vector/` dir on graceful shutdown, and that dump
    /// must NOT land in the daemon's hash-rooted dir.
    ///
    /// Drives the REAL `graceful_shutdown` (not a simulated drop sequence) so the
    /// Step 1 per-slug dump loop actually runs, then asserts:
    /// 1. `{slug}/vector/unimatrix-vector.meta` exists with a non-zero point_count.
    /// 2. The daemon hash dir did NOT gain the slug's vectors (its meta is empty).
    /// 3. Round-trip: `VectorIndex::load` of the slug dir restores the entry and
    ///    `search` returns it (parity with single-project behavior).
    #[tokio::test]
    async fn test_graceful_shutdown_dumps_per_slug_vector_index() {
        use unimatrix_adapt::{AdaptConfig, AdaptationService};
        use unimatrix_core::VectorConfig;

        use crate::infra::audit::AuditLog;
        use crate::infra::registry::AgentRegistry;

        let base = tempfile::TempDir::new().unwrap();
        let base_path = base.path();

        // --- daemon (hash-rooted) subsystems: empty index, distinct dir ---
        let hash_dir = base_path.join("deadbeefhash");
        let daemon_db = hash_dir.join("unimatrix.db");
        let daemon_vector_dir = hash_dir.join("vector");
        std::fs::create_dir_all(&daemon_vector_dir).unwrap();

        let daemon_store = open_store(&daemon_db).await;
        let daemon_index =
            Arc::new(VectorIndex::new(Arc::clone(&daemon_store), VectorConfig::default()).unwrap());
        let dim = daemon_index.config().dimension;

        // --- per-slug subsystems: SEEDED index, its OWN `{slug}/vector/` dir ---
        let slug_dir = base_path.join("alpha");
        let slug_db = slug_dir.join("unimatrix.db");
        let slug_vector_dir = slug_dir.join("vector");
        std::fs::create_dir_all(&slug_vector_dir).unwrap();

        let slug_store = open_store(&slug_db).await;
        let slug_index =
            Arc::new(VectorIndex::new(Arc::clone(&slug_store), VectorConfig::default()).unwrap());
        let (slug_entry_id, query) = seed_one_vector(&slug_store, &slug_index, dim).await;
        assert!(slug_index.point_count() > 0, "slug index must be seeded");

        // Daemon-side holders required by LifecycleHandles (mirror main.rs wiring).
        let registry =
            Arc::new(AgentRegistry::new(Arc::clone(&daemon_store), true, vec![]).unwrap());
        let audit = Arc::new(AuditLog::new(Arc::clone(&daemon_store)));
        let adapt_service = Arc::new(AdaptationService::new(AdaptConfig::default()));

        // Keep a slug-store clone for the post-shutdown round-trip `load`.
        let slug_store_for_load = Arc::clone(&slug_store);

        let handles = LifecycleHandles {
            store: daemon_store,
            vector_index: daemon_index,
            vector_dir: daemon_vector_dir.clone(),
            // #823: the slug index is registered here for its OWN dir.
            per_slug_vectors: vec![(slug_index, slug_vector_dir.clone())],
            registry,
            audit,
            adapt_service,
            data_dir: hash_dir.clone(),
            mcp_socket_guard: None,
            mcp_acceptor_handle: None,
            socket_guard: None,
            uds_handle: None,
            tick_handle: None,
            services: None,
            http_acceptor_handle: None,
            http_listener_addr: None,
        };

        // Drop our local seed handle to the slug store so `slug_store_for_load`
        // is the only outstanding clone aside from the one inside the slug index.
        drop(slug_store);

        graceful_shutdown(handles).await.expect("graceful shutdown");

        // 1. The slug's index dumped to its OWN dir with a non-zero point_count.
        assert!(
            slug_vector_dir.join("unimatrix-vector.meta").exists(),
            "per-slug {{slug}}/vector/unimatrix-vector.meta must exist after shutdown (#823)"
        );
        assert!(
            meta_point_count(&slug_vector_dir) > 0,
            "per-slug vector dump must report a non-zero point_count"
        );

        // 2. The daemon hash dir did NOT gain the slug's vectors (its meta is empty).
        assert_eq!(
            meta_point_count(&daemon_vector_dir),
            0,
            "daemon hash-rooted vector dir must NOT contain the slug's vectors"
        );

        // 3. Round-trip: the slug dir loads and search returns the seeded entry.
        let loaded = VectorIndex::load(
            slug_store_for_load,
            VectorConfig::default(),
            &slug_vector_dir,
        )
        .await
        .expect("load per-slug index from its dumped dir");
        assert!(
            loaded.point_count() > 0,
            "loaded slug index must be non-empty"
        );
        let results = loaded.search(&query, 5, 32).expect("search loaded index");
        assert!(
            results.iter().any(|r| r.entry_id == slug_entry_id),
            "search on the reloaded per-slug index must return the seeded entry"
        );
    }
}
