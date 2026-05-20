//! Test support module for server-level pipeline tests.
//!
//! Provides `TestServiceLayer` for constructing a full `ServiceLayer` in tests,
//! and helper functions that wrap internal `pub(crate)` types for integration tests.
//!
//! Feature-gated: only available with `test-support` or in `#[cfg(test)]`.

use std::path::Path;
use std::sync::Arc;

use unimatrix_adapt::AdaptationService;
use unimatrix_core::Store;
use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{EntryRecord, QueryFilter, VectorAdapter, VectorConfig, VectorIndex};
use unimatrix_embed::EmbedConfig;

use crate::infra::audit::AuditLog;
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::usage_dedup::UsageDedup;
use crate::services::search::{RetrievalMode, ServiceSearchParams};
use crate::services::{AuditContext, AuditSource, CallerId, RateLimitConfig, ServiceLayer};

/// A search result from the test harness.
#[derive(Debug)]
pub struct TestSearchResult {
    pub entry: EntryRecord,
    pub final_score: f64,
    pub similarity: f64,
}

/// Check if the ONNX model is available.
///
/// Returns `true` if the model is NOT found (i.e., tests should skip).
pub fn skip_if_no_model() -> bool {
    let config = EmbedConfig::default();
    let cache_dir = config.resolve_cache_dir();
    let model_dir = cache_dir.join(config.model.model_id().replace('/', "--"));
    let model_path = model_dir.join(config.model.onnx_filename());

    if !model_path.exists() {
        eprintln!(
            "ONNX model not found at {}, skipping pipeline_e2e test",
            model_path.display()
        );
        return true;
    }
    false
}

/// Test harness wrapping ServiceLayer with helper methods.
pub struct TestHarness {
    layer: ServiceLayer,
    store: Arc<Store>,
}

impl TestHarness {
    /// Construct a fully-wired test harness.
    ///
    /// Returns `None` if the ONNX model is not available.
    pub async fn new(store_path: &Path) -> Option<Self> {
        if skip_if_no_model() {
            return None;
        }

        let store =
            unimatrix_store::SqlxStore::open(store_path, unimatrix_store::PoolConfig::default())
                .await
                .expect("failed to open test store");
        let store = Arc::new(store);

        let vector_config = VectorConfig::default();
        let vector_index = Arc::new(
            VectorIndex::new(Arc::clone(&store), vector_config)
                .expect("failed to create vector index"),
        );

        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));

        let entry_store = Arc::clone(&store);
        let vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

        let embed_handle = EmbedServiceHandle::new();
        let config = EmbedConfig::default();
        embed_handle.start_loading(config);

        // Wait for model to load
        let mut attempts = 0;
        loop {
            match embed_handle.get_adapter().await {
                Ok(_) => break,
                Err(_) if attempts < 30 => {
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                    attempts += 1;
                }
                Err(e) => {
                    eprintln!("Failed to load ONNX model after {attempts} attempts: {e}");
                    return None;
                }
            }
        }

        let adapt_service = Arc::new(AdaptationService::new(
            unimatrix_adapt::AdaptConfig::default(),
        ));
        let audit = Arc::new(AuditLog::new(Arc::clone(&store)));
        let usage_dedup = Arc::new(UsageDedup::new());

        let rate_config = RateLimitConfig {
            search_limit: u32::MAX,
            write_limit: u32::MAX,
            window_secs: 3600,
        };

        let test_pool = Arc::new(
            crate::infra::rayon_pool::RayonPool::new(1, "test-pool")
                .expect("test RayonPool construction must succeed"),
        );

        let layer = ServiceLayer::with_rate_config(
            Arc::clone(&store),
            vector_index,
            vector_store,
            entry_store,
            embed_handle,
            adapt_service,
            audit,
            usage_dedup,
            rate_config,
            crate::infra::config::default_boosted_categories_set(),
            test_pool,
            // crt-023: disabled NLI for test harness (no model in test env)
            crate::infra::nli_handle::NliServiceHandle::new(),
            20,    // nli_top_k default
            false, // nli_enabled: disabled for tests
            // crt-023: default InferenceConfig for tests (NLI disabled via nli_enabled=false above)
            Arc::new(crate::infra::config::InferenceConfig::default()),
            // col-023: built-in default registry for test harness
            Arc::new(unimatrix_observe::domain::DomainPackRegistry::with_builtin_claude_code()),
            // GH #311: default params for test harness.
            Arc::new(unimatrix_engine::confidence::ConfidenceParams::default()),
            // crt-031: default lifecycle policy for test harness.
            Arc::new(crate::infra::categories::CategoryAllowlist::new()),
        );

        Some(TestHarness { layer, store })
    }

    /// Get a reference to the underlying store.
    pub fn store(&self) -> &Store {
        &self.store
    }

    /// Execute a search query through the full pipeline.
    pub async fn search(&self, query: &str, k: usize) -> Result<Vec<TestSearchResult>, String> {
        let params = ServiceSearchParams {
            query: query.to_string(),
            k,
            filters: None,
            similarity_floor: None,
            confidence_floor: None,
            feature_tag: None,
            co_access_anchors: None,
            caller_agent_id: None,
            retrieval_mode: RetrievalMode::Flexible,
            session_id: None,         // crt-026: no session context in test harness
            category_histogram: None, // crt-026: no histogram in test harness
            current_phase: None,      // col-031: no phase context in test harness
        };

        let audit_ctx = AuditContext {
            source: AuditSource::Internal {
                service: "test".to_string(),
            },
            caller_id: "test-harness".to_string(),
            session_id: Some("test-session".to_string()),
            feature_cycle: None,
        };

        let caller_id = CallerId::Agent("test-harness".to_string());

        let results = self
            .layer
            .search
            .search(params, &audit_ctx, &caller_id)
            .await
            .map_err(|e| format!("{e}"))?;

        Ok(results
            .entries
            .into_iter()
            .map(|se| TestSearchResult {
                entry: se.entry,
                final_score: se.final_score,
                similarity: se.similarity,
            })
            .collect())
    }

    /// Call `context_graph` through the full `handle_graph` dispatch path.
    ///
    /// Accepts a `serde_json::Value` representing the `GraphParams` wire object.
    /// Deserializes it to `GraphParams`, then calls `handle_graph` directly —
    /// exercising validation → mode dispatch → subgraph BFS → SQL reads.
    ///
    /// The capability check (require_cap) that normally runs in `tools.rs` is
    /// intentionally skipped here; this helper is for topology and BFS path
    /// testing, not auth testing. Returns the JSON response text on success,
    /// or the `ErrorData` message on failure.
    ///
    /// Used by integration tests (FR-23, AC-14): exercises the full
    /// `handle_graph` call path with a real store and graph state.
    pub async fn call_graph(&self, params: serde_json::Value) -> Result<serde_json::Value, String> {
        let graph_params: crate::mcp::graph_read::GraphParams =
            serde_json::from_value(params).map_err(|e| format!("param parse error: {e}"))?;

        let handle = self.layer.typed_graph_handle();

        let result = crate::mcp::graph_read::handle_graph(
            &self.store,
            &handle,
            graph_params,
            &crate::mcp::context::ToolContext {
                agent_id: "test-harness".to_string(),
                trust_level: crate::infra::registry::TrustLevel::Internal,
                format: crate::mcp::response::ResponseFormat::Json,
                audit_ctx: crate::services::AuditContext {
                    source: crate::services::AuditSource::Internal {
                        service: "test-harness".to_string(),
                    },
                    caller_id: "test-harness".to_string(),
                    session_id: None,
                    feature_cycle: None,
                },
                caller_id: crate::services::CallerId::Agent("test-harness".to_string()),
                client_type: None,
            },
        )
        .await
        .map_err(|e| e.message.to_string())?;

        // Extract the text content from CallToolResult.
        let text = result
            .content
            .into_iter()
            .filter_map(|c| {
                if let rmcp::model::RawContent::Text(t) = c.raw {
                    Some(t.text)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");

        serde_json::from_str(&text).map_err(|e| format!("response parse error: {e}"))
    }

    /// Populate the in-memory TypedRelationGraph from the store's GRAPH_EDGES table.
    ///
    /// Calls `TypedGraphState::rebuild()` and writes the result to the shared handle.
    /// Required before any subgraph BFS call that expects to traverse real edges.
    pub async fn rebuild_typed_graph(&self) {
        let new_state = crate::services::TypedGraphState::rebuild(&self.store)
            .await
            .expect("TypedGraphState::rebuild must succeed in test");
        let handle = self.layer.typed_graph_handle();
        let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
        *guard = new_state;
    }

    /// Insert a raw graph edge into GRAPH_EDGES (bypasses business logic, for test setup).
    pub async fn insert_graph_edge(&self, source_id: u64, target_id: u64, relation_type: &str) {
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges \
             (source_id, target_id, relation_type, weight, created_at, \
              created_by, source, bootstrap_only, metadata) \
             VALUES (?1, ?2, ?3, 1.0, strftime('%s','now'), 'test', 'test', 0, '')",
        )
        .bind(source_id as i64)
        .bind(target_id as i64)
        .bind(relation_type)
        .execute(self.store.write_pool_server())
        .await
        .expect("insert_graph_edge must succeed");
    }

    /// Execute a search with explicit filter.
    pub async fn search_with_filter(
        &self,
        query: &str,
        k: usize,
        filter: QueryFilter,
    ) -> Result<Vec<TestSearchResult>, String> {
        let params = ServiceSearchParams {
            query: query.to_string(),
            k,
            filters: Some(filter),
            similarity_floor: None,
            confidence_floor: None,
            feature_tag: None,
            co_access_anchors: None,
            caller_agent_id: None,
            retrieval_mode: RetrievalMode::Flexible,
            session_id: None,         // crt-026: no session context in test harness
            category_histogram: None, // crt-026: no histogram in test harness
            current_phase: None,      // col-031: no phase context in test harness
        };

        let audit_ctx = AuditContext {
            source: AuditSource::Internal {
                service: "test".to_string(),
            },
            caller_id: "test-harness".to_string(),
            session_id: Some("test-session".to_string()),
            feature_cycle: None,
        };

        let caller_id = CallerId::Agent("test-harness".to_string());

        let results = self
            .layer
            .search
            .search(params, &audit_ctx, &caller_id)
            .await
            .map_err(|e| format!("{e}"))?;

        Ok(results
            .entries
            .into_iter()
            .map(|se| TestSearchResult {
                entry: se.entry,
                final_score: se.final_score,
                similarity: se.similarity,
            })
            .collect())
    }
}
