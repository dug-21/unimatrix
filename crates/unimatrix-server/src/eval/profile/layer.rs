//! `EvalServiceLayer` — read-only, analytics-suppressed ServiceLayer for eval replay (nan-007).

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use unimatrix_adapt::{AdaptConfig, AdaptationService};
use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{Store, VectorAdapter, VectorConfig, VectorIndex};
use unimatrix_embed::{EmbedConfig, NliModel};

use crate::infra::audit::AuditLog;
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::nli_handle::{NliConfig, NliServiceHandle};
use crate::infra::rayon_pool::RayonPool;
use crate::infra::usage_dedup::UsageDedup;
use crate::project;
use crate::services::{RateLimitConfig, ServiceLayer};

use super::error::EvalError;
use super::types::{AnalyticsMode, EvalProfile};
use super::validation::validate_confidence_weights;

// ---------------------------------------------------------------------------
// EvalServiceLayer
// ---------------------------------------------------------------------------

/// A read-only, analytics-suppressed service layer for eval replay.
///
/// Wraps a `ServiceLayer` built against a snapshot database opened with
/// `SqlxStore::open_readonly()` (no migration, no drain task). The analytics
/// write queue is never wired — the drain task is suppressed via
/// `AnalyticsMode::Suppressed` (ADR-002, SR-07). No `enqueue_analytics` calls
/// are made in the eval path.
///
/// Construct via `EvalServiceLayer::from_profile()`. If construction returns
/// `Ok`, all invariants are satisfied.
pub struct EvalServiceLayer {
    /// The underlying service layer for search replay (Wave 2: runner.rs).
    ///
    /// `pub(crate)` rather than public so `runner.rs` can call `.search` directly.
    #[allow(dead_code)]
    pub(crate) inner: ServiceLayer,
    /// Raw read-only pool for direct sqlx queries in runner.rs (Wave 2).
    ///
    /// Held here so runner.rs can scan query_log or entries without going
    /// through the ServiceLayer abstraction. Never used for writes.
    #[allow(dead_code)]
    pub(crate) pool: SqlitePool,
    /// Embedding handle for model-readiness polling in runner.rs.
    ///
    /// `runner.rs` calls `embed_handle().get_adapter()` in a 30 × 100 ms poll
    /// loop before scenario replay begins (eval-runner.md lines 148–158).
    pub(crate) embed_handle: Arc<EmbedServiceHandle>,
    /// The canonicalized snapshot database path.
    pub(crate) db_path: PathBuf,
    /// The profile name, used for labelling results.
    pub(crate) profile_name: String,
    /// Always `Suppressed` in nan-007. Stored for type-level documentation.
    pub(crate) analytics_mode: AnalyticsMode,
    /// crt-023: NLI handle for NLI-enabled eval profiles (ADR-006).
    ///
    /// `None` when `nli_enabled = false` (baseline profiles).
    /// `Some(...)` when `nli_enabled = true`; may be Loading or Failed.
    /// Used by runner.rs to poll for NLI readiness before scenario replay.
    pub(crate) nli_handle: Option<Arc<NliServiceHandle>>,
}

impl fmt::Debug for EvalServiceLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvalServiceLayer")
            .field("db_path", &self.db_path)
            .field("profile_name", &self.profile_name)
            .field("analytics_mode", &self.analytics_mode)
            .field("nli_handle", &self.nli_handle.is_some())
            .finish_non_exhaustive()
    }
}

impl EvalServiceLayer {
    /// Construct an `EvalServiceLayer` from a snapshot database and a profile.
    ///
    /// Performs all validation before opening any database connection:
    /// 1. Live-DB path guard — rejects if `db_path` resolves to the active DB (C-13, FR-44)
    /// 2. Inference model path validation — rejects missing/unreadable model files (C-14)
    /// 3. `ConfidenceWeights` sum invariant — rejects if weights do not sum to 0.92 (C-15)
    /// 4. Opens a raw read-only `SqlitePool` for direct queries in runner.rs (C-02)
    /// 5. Opens a `SqlxStore` via `open_readonly()` — no migration, no drain task (FR-24, C-02)
    /// 6. Constructs `VectorIndex`, `EmbedServiceHandle`, `RayonPool`, and `ServiceLayer`
    ///    with `AnalyticsMode::Suppressed` (ADR-002)
    ///
    /// Returns `EvalError` for all failure modes. Never panics (FR-23, SR-09).
    pub async fn from_profile(
        db_path: &Path,
        profile: &EvalProfile,
        project_dir: Option<&Path>,
    ) -> Result<Self, EvalError> {
        // ----------------------------------------------------------------
        // Step 1: Live-DB path guard (C-13, FR-44, ADR-001)
        // ----------------------------------------------------------------
        let paths = project::ensure_data_directory(project_dir, None).map_err(EvalError::Io)?;

        let active_db =
            std::fs::canonicalize(&paths.db_path).unwrap_or_else(|_| paths.db_path.clone());

        let db_resolved = std::fs::canonicalize(db_path).map_err(EvalError::Io)?;

        if db_resolved == active_db {
            return Err(EvalError::LiveDbPath {
                supplied: db_path.to_path_buf(),
                active: active_db,
            });
        }

        // ----------------------------------------------------------------
        // Step 2: Validate [inference] model paths (C-14, FR-23, SR-09)
        // ----------------------------------------------------------------
        // crt-023 (W1-4 stub fill): validate NLI model fields when nli_enabled.
        let nli_cfg = &profile.config_overrides.inference;
        if nli_cfg.nli_enabled {
            // Validate that nli_model_name is a recognized variant if set.
            if let Some(ref name) = nli_cfg.nli_model_name {
                if NliModel::from_config_name(name).is_none() {
                    return Err(EvalError::ConfigInvariant(format!(
                        "nli_model_name '{}' is not a recognized model variant; valid: minilm2, minilm2-q8, deberta, deberta-q8",
                        name
                    )));
                }
            }
            // Warn if nli_model_path is set but the file does not exist.
            // ADR-006: SKIP behavior on load failure, not immediate error here.
            if let Some(ref path) = nli_cfg.nli_model_path {
                if !path.exists() {
                    tracing::warn!(
                        profile = %profile.name,
                        path = %path.display(),
                        "eval: nli_model_path not found; profile may be SKIPPED if model unavailable"
                    );
                }
            }
        }

        // ----------------------------------------------------------------
        // Step 3: Validate ConfidenceWeights sum invariant (C-06, C-15, SR-08)
        // ----------------------------------------------------------------
        validate_confidence_weights(&profile.config_overrides)?;

        // ----------------------------------------------------------------
        // Step 4: Open raw read-only SqlitePool (C-02, ADR-002, FR-24)
        //
        // This pool is kept in EvalServiceLayer.pool for direct sqlx queries
        // in runner.rs. It must not be used for writes.
        // ----------------------------------------------------------------
        let opts = SqliteConnectOptions::new()
            .filename(db_path)
            .read_only(true);

        let pool = SqlitePool::connect_with(opts)
            .await
            .map_err(|e| EvalError::Store(Box::new(e)))?;

        // ----------------------------------------------------------------
        // Step 5: Build VectorIndex from snapshot (C-02, FR-24)
        //
        // VectorIndex::new() requires Arc<SqlxStore> — a concrete type, not a
        // trait object. We cannot pass the raw SqlitePool directly.
        //
        // SqlxStore::open_readonly() opens a read-only pool with no migration
        // step and no drain task. The analytics channel receiver is immediately
        // dropped so all enqueue_analytics calls are silent no-ops.
        // ----------------------------------------------------------------
        let store = unimatrix_store::SqlxStore::open_readonly(db_path)
            .await
            .map_err(|e| EvalError::Store(Box::new(e)))?;
        let store_arc: Arc<Store> = Arc::new(store);

        // Determine the sibling vector directory for this snapshot.
        // Convention: {db_parent}/vector/ holds the HNSW files copied by `snapshot`.
        let vector_config = VectorConfig::default();
        let db_parent = db_resolved.parent().ok_or_else(|| {
            EvalError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "snapshot db path has no parent directory",
            ))
        })?;
        let vector_dir = db_parent.join("vector");
        let vector_meta = vector_dir.join("unimatrix-vector.meta");

        let vector_index = if vector_meta.exists() {
            // Load the persisted HNSW index from the snapshot's vector dir (GH-323).
            Arc::new(
                VectorIndex::load(Arc::clone(&store_arc), vector_config, &vector_dir)
                    .await
                    .map_err(|e| EvalError::Store(Box::new(e)))?,
            )
        } else {
            // No vector files present (pre-fix snapshot or empty index) — fall back
            // to a fresh empty index for backward compatibility (GH-323).
            Arc::new(
                VectorIndex::new(Arc::clone(&store_arc), vector_config)
                    .map_err(|e| EvalError::Store(Box::new(e)))?,
            )
        };

        // ----------------------------------------------------------------
        // Step 6: Build embedding handle (model loading deferred to background)
        //
        // runner.rs polls embed_handle().get_adapter() up to 30 × 100 ms
        // before scenario replay (eval-runner.md lines 148–158).
        // ----------------------------------------------------------------
        let embed_handle = EmbedServiceHandle::new();
        let embed_config = EmbedConfig::default();
        embed_handle.start_loading(embed_config);

        // ----------------------------------------------------------------
        // Step 6b: Build NLI handle for NLI-enabled profiles (crt-023, FR-26, ADR-006).
        // Baseline profiles (nli_enabled = false) set nli_handle = None.
        // The NliServiceHandle may be in Loading state; runner.rs polls for readiness.
        // ----------------------------------------------------------------
        let nli_handle: Option<Arc<NliServiceHandle>> =
            if profile.config_overrides.inference.nli_enabled {
                let handle = NliServiceHandle::new();
                let cache_dir = EmbedConfig::default().resolve_cache_dir();
                let inf = &profile.config_overrides.inference;
                let nli_config = NliConfig {
                    nli_enabled: true,
                    nli_model_name: inf.nli_model_name.clone(),
                    nli_model_path: inf.nli_model_path.clone(),
                    nli_model_sha256: inf.nli_model_sha256.clone(),
                    cache_dir,
                };
                handle.start_loading(nli_config);
                Some(handle)
            } else {
                None // baseline profile — no NLI handle needed
            };

        // ----------------------------------------------------------------
        // Step 7: Build inference pool
        // ----------------------------------------------------------------
        let rayon_pool_size = profile.config_overrides.inference.rayon_pool_size;
        let rayon_pool = Arc::new(
            RayonPool::new(rayon_pool_size, &format!("eval-{}", profile.name))
                .map_err(|e| EvalError::Store(Box::new(e)))?,
        );

        // ----------------------------------------------------------------
        // Step 8: Build adaptation service
        // ----------------------------------------------------------------
        let adapt_svc = Arc::new(AdaptationService::new(AdaptConfig::default()));

        // ----------------------------------------------------------------
        // Step 9: Build AuditLog
        //
        // Writes will fail (read-only pool) — acceptable; audit writes are
        // not called in the eval search path.
        // ----------------------------------------------------------------
        let audit = Arc::new(AuditLog::new(Arc::clone(&store_arc)));

        // ----------------------------------------------------------------
        // Step 10: Build UsageDedup
        // ----------------------------------------------------------------
        let usage_dedup = Arc::new(UsageDedup::new());

        // ----------------------------------------------------------------
        // Step 11: Build VectorAdapter and AsyncVectorStore
        // ----------------------------------------------------------------
        let vector_adapter = VectorAdapter::new(Arc::clone(&vector_index));
        let async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)));

        // ----------------------------------------------------------------
        // Step 12: Boosted categories
        // ----------------------------------------------------------------
        let boosted_categories: HashSet<String> = HashSet::from(["lesson-learned".to_string()]);

        // ----------------------------------------------------------------
        // Step 13: Build ServiceLayer via with_rate_config (TestHarness pattern)
        //
        // AnalyticsMode::Suppressed: rate limits set to u32::MAX so eval
        // replay is never blocked. No analytics_tx channel registered — the
        // drain task is never spawned (ADR-002).
        // ----------------------------------------------------------------
        let rate_config = RateLimitConfig {
            search_limit: u32::MAX,
            write_limit: u32::MAX,
            window_secs: 3600,
        };

        // crt-023: Baseline profiles use an unstarted NliServiceHandle (get_provider()
        // returns NliNotReady immediately) so SearchService always has a handle to call.
        // NLI-enabled profiles use the handle started in Step 6b.
        let nli_handle_arc: Arc<NliServiceHandle> = match nli_handle.clone() {
            Some(h) => h,
            None => NliServiceHandle::new(), // unstarted → NliNotReady → cosine fallback
        };
        let nli_top_k = profile.config_overrides.inference.nli_top_k;
        let nli_enabled = profile.config_overrides.inference.nli_enabled;

        let inner = ServiceLayer::with_rate_config(
            Arc::clone(&store_arc),
            Arc::clone(&vector_index),
            Arc::clone(&async_vector_store),
            Arc::clone(&store_arc),
            Arc::clone(&embed_handle),
            Arc::clone(&adapt_svc),
            Arc::clone(&audit),
            Arc::clone(&usage_dedup),
            rate_config,
            boosted_categories,
            Arc::clone(&rayon_pool),
            nli_handle_arc,
            nli_top_k,
            nli_enabled,
            Arc::new(profile.config_overrides.inference.clone()),
            // col-023: built-in default registry for eval profiles
            Arc::new(unimatrix_observe::domain::DomainPackRegistry::with_builtin_claude_code()),
        );

        Ok(EvalServiceLayer {
            inner,
            pool,
            embed_handle,
            db_path: db_resolved,
            profile_name: profile.name.clone(),
            analytics_mode: AnalyticsMode::Suppressed,
            nli_handle,
        })
    }

    /// Return the embed handle for model-readiness polling.
    ///
    /// Used by `runner.rs` in the 30 × 100 ms poll loop before scenario replay.
    pub(crate) fn embed_handle(&self) -> Arc<EmbedServiceHandle> {
        Arc::clone(&self.embed_handle)
    }

    /// Return the profile name for this layer.
    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }

    /// Return the snapshot database path used by this layer.
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Return the analytics mode (always `Suppressed` for eval layers).
    pub fn analytics_mode(&self) -> AnalyticsMode {
        self.analytics_mode
    }

    /// Return the NLI handle if present (NLI-enabled profiles only).
    ///
    /// `None` for baseline profiles. Used by `runner.rs` to poll readiness.
    pub(crate) fn nli_handle(&self) -> Option<Arc<NliServiceHandle>> {
        self.nli_handle.clone()
    }

    /// Return `true` if this layer has an NLI handle (NLI-enabled profile).
    ///
    /// Used in tests to verify NLI wiring.
    #[allow(dead_code)]
    pub(crate) fn has_nli_handle(&self) -> bool {
        self.nli_handle.is_some()
    }
}
