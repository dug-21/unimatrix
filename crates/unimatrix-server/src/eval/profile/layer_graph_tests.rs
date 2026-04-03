//! Integration tests for TypedGraphState rebuild wiring in EvalServiceLayer (crt-045).
//!
//! Covers: three-layer graph rebuild assertion (AC-06, ADR-003) and degraded-mode
//! cycle-abort-safety (AC-05, ADR-002).

#[cfg(test)]
mod layer_graph_tests {
    use std::path::PathBuf;

    use tempfile::TempDir;
    use unimatrix_store::pool_config::PoolConfig;

    use crate::infra::config::UnimatrixConfig;

    use super::super::error::EvalError;
    use super::super::layer::EvalServiceLayer;
    use super::super::types::EvalProfile;

    fn baseline_profile() -> EvalProfile {
        EvalProfile {
            name: "baseline".to_string(),
            description: None,
            config_overrides: UnimatrixConfig::default(),
            distribution_change: false,
            distribution_targets: None,
        }
    }

    type GraphSnapshot = (
        TempDir,
        PathBuf,
        std::sync::Arc<unimatrix_store::SqlxStore>,
        u64,
        u64,
    );

    /// Open a migrated store, insert two Active entries + one CoAccess edge,
    /// dump an empty VectorIndex. Returns (dir, snap_path, store, id_a, id_b).
    /// Caller keeps `dir` alive; `store` is returned for additional writes.
    async fn seed_graph_snapshot() -> GraphSnapshot {
        use std::sync::Arc;

        use unimatrix_core::{VectorConfig, VectorIndex};
        use unimatrix_store::{NewEntry, SqlxStore, Status};

        let dir = TempDir::new().expect("tempdir");
        let snap_path = dir.path().join("snapshot.db");
        let store = Arc::new(
            SqlxStore::open(&snap_path, PoolConfig::default())
                .await
                .expect("open store"),
        );
        let mk_entry = |title: &str, content: &str| NewEntry {
            title: title.to_string(),
            content: content.to_string(),
            topic: "test".to_string(),
            category: "decision".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: "test".to_string(),
            feature_cycle: "crt-045".to_string(),
            trust_source: "agent".to_string(),
        };
        let id_a = store
            .insert(mk_entry("entry-a-crt045", "graph-test-content-a"))
            .await
            .expect("insert A");
        let id_b = store
            .insert(mk_entry("entry-b-crt045", "graph-test-content-b"))
            .await
            .expect("insert B");
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (?1, ?2, 'CoAccess', 1.0, strftime('%s','now'), 'tick', 'co_access', 0)",
        )
        .bind(id_a as i64)
        .bind(id_b as i64)
        .execute(store.write_pool_server())
        .await
        .expect("insert CoAccess edge A\u{2192}B");
        let vi = VectorIndex::new(Arc::clone(&store), VectorConfig::default()).expect("vi");
        vi.dump(&dir.path().join("vector")).expect("dump");
        (dir, snap_path, store, id_a, id_b)
    }

    /// Three-layer assertion: rebuilt TypedGraphState is visible at handle state,
    /// graph connectivity, and live search call time. AC-06, R-01–R-03, SR-05, SR-06.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_from_profile_typed_graph_rebuilt_after_construction() {
        use crate::services::{
            AuditContext, AuditSource, CallerId, RetrievalMode, ServiceSearchParams,
        };
        use unimatrix_engine::graph::find_terminal_active;

        let (dir, snap_path, _store, id_a, _id_b) = seed_graph_snapshot().await;
        let layer =
            match EvalServiceLayer::from_profile(&snap_path, &baseline_profile(), Some(dir.path()))
                .await
            {
                Ok(l) => l,
                Err(EvalError::LiveDbPath { .. }) => return,
                Err(e) => panic!("from_profile must succeed on seeded snapshot; got: {e}"),
            };

        // Layer 1: Handle state — use_fallback=false, entries populated (AC-01, AC-06).
        let handle = layer.typed_graph_handle();
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(
            !guard.use_fallback,
            "use_fallback must be false after rebuild with a seeded snapshot"
        );
        assert!(
            guard.all_entries.len() >= 2,
            "all_entries must contain at least the two seeded Active entries; got {}",
            guard.all_entries.len()
        );
        // Layer 2: Graph connectivity — entry A is its own terminal (R-02, ADR-003).
        // Entry A is Active and not superseded — find_terminal_active must return Some(id_a).
        let terminal = find_terminal_active(id_a, &guard.typed_graph, &guard.all_entries);
        assert_eq!(
            terminal,
            Some(id_a),
            "Active entry A must be reachable as its own terminal via find_terminal_active"
        );
        drop(guard); // release read lock before live search (deadlock prevention)

        // Layer 3: Live search returns Ok (R-01, R-02, SR-05, ADR-003).
        // CI has no embedding model; accept Ok or EmbeddingFailed — any other Err is a regression.
        let params = ServiceSearchParams {
            query: "test query for graph rebuild verification".to_string(),
            k: 3,
            filters: None,
            similarity_floor: None,
            confidence_floor: None,
            feature_tag: None,
            co_access_anchors: None,
            caller_agent_id: Some("test-agent".to_string()),
            retrieval_mode: RetrievalMode::Flexible,
            session_id: None,
            category_histogram: None,
            current_phase: None,
        };
        let audit_ctx = AuditContext {
            source: AuditSource::Internal {
                service: "layer_test".to_string(),
            },
            caller_id: "test-agent".to_string(),
            session_id: None,
            feature_cycle: Some("crt-045".to_string()),
        };
        let caller_id = CallerId::Agent("test-agent".to_string());
        match layer
            .inner
            .search
            .search(params, &audit_ctx, &caller_id)
            .await
        {
            Ok(_) | Err(crate::services::ServiceError::EmbeddingFailed(_)) => {}
            Err(e) => panic!("search must return Ok or EmbeddingFailed in CI; got: {e:?}"),
        }
    }

    /// Degraded mode: rebuild fails on Supersedes cycle → Ok(layer) with use_fallback=true.
    /// AC-05, R-04, FR-03, ADR-002.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_from_profile_returns_ok_on_cycle_error() {
        // Seed two Active entries; CoAccess edge is harmless for Supersedes cycle detection.
        let (dir, snap_path, store, id_a, id_b) = seed_graph_snapshot().await;
        // Build A→B→A Supersedes cycle via entries.supersedes (authoritative source for
        // cycle detection in Pass 2a; GRAPH_EDGES Supersedes rows are skipped in Pass 2b).
        for (sup, entry) in [(id_b, id_a), (id_a, id_b)] {
            sqlx::query("UPDATE entries SET supersedes = ?1 WHERE id = ?2")
                .bind(sup as i64)
                .bind(entry as i64)
                .execute(store.write_pool_server())
                .await
                .expect("set supersedes");
        }
        // from_profile() must return Ok(layer) in degraded mode — never abort (AC-05).
        let layer =
            match EvalServiceLayer::from_profile(&snap_path, &baseline_profile(), Some(dir.path()))
                .await
            {
                Ok(l) => l,
                Err(EvalError::LiveDbPath { .. }) => return,
                Err(e) => {
                    panic!("from_profile must return Ok even on rebuild cycle error; got: {e}")
                }
            };
        // use_fallback must remain true — rebuild was skipped due to cycle (AC-05, ADR-002).
        let handle = layer.typed_graph_handle();
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(
            guard.use_fallback,
            "use_fallback must remain true when rebuild fails due to Supersedes cycle"
        );
    }
}
