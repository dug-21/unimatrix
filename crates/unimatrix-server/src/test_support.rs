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
            // dsn-001: default; test harness preserves pre-dsn-001 behavior.
            std::collections::HashSet::from(["lesson-learned".to_string()]),
            test_pool,
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
