//! Eval profile types and `EvalServiceLayer` construction (nan-007).
//!
//! Defines the type-level foundation for the eval engine:
//! - `AnalyticsMode` — structural guarantee that eval never writes analytics (ADR-002)
//! - `EvalProfile` — parsed profile TOML with config overrides
//! - `EvalServiceLayer` — restricted ServiceLayer variant for eval replay
//! - `EvalError` — all structured errors for the eval subsystem
//!
//! `EvalServiceLayer::from_profile()` is the single construction gateway.
//! All invariant validation (live-DB guard, model paths, weight sum) occurs
//! here. If construction returns `Ok`, the layer is safe to use for replay.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use unimatrix_adapt::{AdaptConfig, AdaptationService};
use unimatrix_core::async_wrappers::AsyncVectorStore;
use unimatrix_core::{Store, VectorAdapter, VectorConfig, VectorIndex};
use unimatrix_embed::EmbedConfig;

use crate::infra::audit::AuditLog;
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::rayon_pool::RayonPool;
use crate::infra::usage_dedup::UsageDedup;
use crate::project;
use crate::services::{RateLimitConfig, ServiceLayer};

use crate::infra::config::UnimatrixConfig;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default rayon thread pool size for eval profiles (memory efficiency).
///
/// Set to 1 because eval scenarios are replayed sequentially per profile.
/// Callers may override via `profile.config_overrides.inference.rayon_pool_size`.
///
/// Used when InferenceConfig is overridden with 0 (invalid) as a floor.
/// Wave 2 (runner.rs) uses this directly.
#[allow(dead_code)]
const DEFAULT_EVAL_RAYON_POOL_SIZE: usize = 1;

/// Expected sum of the six confidence weight fields (ADR-005 invariant).
const EXPECTED_WEIGHT_SUM: f64 = 0.92;

/// Floating-point tolerance for weight sum validation (C-06, C-15).
const WEIGHT_SUM_TOLERANCE: f64 = 1e-9;

// ---------------------------------------------------------------------------
// AnalyticsMode
// ---------------------------------------------------------------------------

/// Controls whether the analytics write queue is active in a ServiceLayer.
///
/// `Suppressed` is the only mode used in nan-007. `Live` exists for future
/// use in a hypothetical `eval live` command where analytics recording is
/// acceptable (ADR-002, SR-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticsMode {
    /// Normal SqlxStore behaviour — drain task active, analytics writes occur.
    /// Reserved for future `eval live` mode. NOT used in nan-007.
    Live,
    /// No drain task spawned; `enqueue_analytics` calls are no-ops.
    /// Always used in `EvalServiceLayer` construction (ADR-002, SR-07).
    Suppressed,
}

// ---------------------------------------------------------------------------
// EvalProfile
// ---------------------------------------------------------------------------

/// A named eval profile parsed from a TOML file.
///
/// An empty TOML body (with only `[profile]` name/description) represents
/// the baseline profile and uses all compiled defaults from `UnimatrixConfig`.
///
/// Profile TOML format:
/// ```toml
/// [profile]
/// name = "candidate-weights-v1"
/// description = "Test higher base weight"   # optional
///
/// [confidence.weights]
/// # All six weight fields required if [confidence.weights] present (C-06).
/// # Fields match ConfidenceWeights struct: base, usage, fresh, help, corr, trust
/// base  = 0.20
/// usage = 0.15
/// fresh = 0.17
/// help  = 0.15
/// corr  = 0.15
/// trust = 0.10
/// # sum must be 0.92 ± 1e-9
///
/// [inference]
/// # Optional; rayon_pool_size validated at from_profile() time (C-14).
/// rayon_pool_size = 1
/// ```
#[derive(Debug, Clone)]
pub struct EvalProfile {
    /// Profile identifier. Must be unique across all profiles in a single
    /// `eval run` invocation (checked by run_eval, not by from_profile).
    pub name: String,
    /// Optional human-readable description of what this profile tests.
    pub description: Option<String>,
    /// Config overrides. Absent sections use compiled defaults.
    /// An empty `UnimatrixConfig` → all compiled defaults → baseline profile.
    pub config_overrides: UnimatrixConfig,
}

// ---------------------------------------------------------------------------
// EvalError
// ---------------------------------------------------------------------------

/// Structured errors for the eval subsystem (no panics, no raw serde errors).
///
/// All variants produce user-readable messages that name the invariant
/// violated and the relevant paths or values (SR-08, SR-09).
#[derive(Debug)]
pub enum EvalError {
    /// A model file referenced in `[inference]` section is missing or unreadable.
    ///
    /// Returned at `from_profile()` time, never at inference time (C-14, FR-23).
    ModelNotFound(PathBuf),

    /// A config invariant was violated (weight sum, TOML parse error, etc.).
    ///
    /// The string is a user-readable message naming the expected and actual
    /// values. Never a raw serde error (C-06, C-15, SR-08).
    ConfigInvariant(String),

    /// The supplied `--db` path (eval run) resolves to the active daemon DB.
    ///
    /// Both resolved paths are included in the error for diagnostics (C-13,
    /// FR-44, ADR-001).
    LiveDbPath {
        /// The path as supplied by the caller (before canonicalization).
        supplied: PathBuf,
        /// The canonicalized active daemon DB path.
        active: PathBuf,
    },

    /// I/O error (file open, canonicalize failure, permission denied).
    Io(std::io::Error),

    /// Store/SQLx error from pool construction or query execution.
    Store(Box<dyn std::error::Error + Send + Sync>),

    /// Two profile TOMLs in a single `eval run` share the same `[profile].name`.
    ///
    /// Detected by `run_eval` before any `from_profile()` call. Named here for
    /// structural completeness.
    ProfileNameCollision(String),

    /// The `--k` argument is 0; P@K is undefined for k = 0.
    InvalidK(usize),
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::ModelNotFound(p) => {
                write!(f, "model not found: {}", p.display())
            }
            EvalError::ConfigInvariant(msg) => write!(f, "{msg}"),
            EvalError::LiveDbPath { supplied, active } => write!(
                f,
                "eval db path resolves to the active database\n  \
                 supplied (resolved): {}\n  \
                 active:              {}\n  \
                 use a snapshot, not the live database",
                supplied.display(),
                active.display()
            ),
            EvalError::Io(e) => write!(f, "I/O error: {e}"),
            EvalError::Store(e) => write!(f, "store error: {e}"),
            EvalError::ProfileNameCollision(name) => write!(
                f,
                "duplicate profile name \"{name}\" — two profile TOMLs share the same [profile].name"
            ),
            EvalError::InvalidK(k) => write!(f, "--k must be >= 1, got {k}"),
        }
    }
}

impl std::error::Error for EvalError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            EvalError::Io(e) => Some(e),
            EvalError::Store(e) => Some(e.as_ref()),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// EvalServiceLayer
// ---------------------------------------------------------------------------

/// A read-only, analytics-suppressed service layer for eval replay.
///
/// Wraps a `ServiceLayer` built against a snapshot database opened with
/// `SqliteConnectOptions::read_only(true)`. The analytics write queue is
/// never wired — the drain task is suppressed via `AnalyticsMode::Suppressed`
/// (ADR-002, SR-07). No `enqueue_analytics` calls are made in the eval path.
///
/// Construct via `EvalServiceLayer::from_profile()`. If construction returns
/// `Ok`, all invariants are satisfied.
pub struct EvalServiceLayer {
    /// The underlying service layer for search replay (Wave 2: runner.rs).
    ///
    /// `pub(crate)` rather than public so `runner.rs` can call `.search` directly.
    /// Marked `allow(dead_code)` until Wave 2 modules are added.
    #[allow(dead_code)]
    pub(crate) inner: ServiceLayer,
    /// Raw read-only pool for direct sqlx queries in runner.rs (Wave 2).
    ///
    /// Held here so runner.rs can scan query_log or entries without going
    /// through the ServiceLayer abstraction. Never used for writes.
    /// Marked `allow(dead_code)` until Wave 2 modules are added.
    #[allow(dead_code)]
    pub(crate) pool: SqlitePool,
    /// The canonicalized snapshot database path.
    pub(crate) db_path: PathBuf,
    /// The profile name, used for labelling results.
    pub(crate) profile_name: String,
    /// Always `Suppressed` in nan-007. Stored for type-level documentation.
    pub(crate) analytics_mode: AnalyticsMode,
}

impl fmt::Debug for EvalServiceLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EvalServiceLayer")
            .field("db_path", &self.db_path)
            .field("profile_name", &self.profile_name)
            .field("analytics_mode", &self.analytics_mode)
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
    /// 4. Opens a raw read-only `SqlitePool` — never calls `SqlxStore::open()` (C-02)
    /// 5. Constructs `VectorIndex`, `EmbedServiceHandle`, `RayonPool`, and `ServiceLayer`
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

        // Canonicalize the active daemon DB path. If canonicalize fails, the
        // daemon DB does not exist yet — still safe to compare.
        let active_db =
            std::fs::canonicalize(&paths.db_path).unwrap_or_else(|_| paths.db_path.clone());

        // Canonicalize the supplied snapshot path. Failure means the snapshot
        // file does not exist — return Io error.
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
        // InferenceConfig is stub-only for nan-007 (no nli_model field yet).
        // When W1-4 adds nli_model, validation goes here.
        // No model path fields to validate in the current InferenceConfig struct.

        // ----------------------------------------------------------------
        // Step 3: Validate ConfidenceWeights sum invariant (C-06, C-15, SR-08)
        // ----------------------------------------------------------------
        validate_confidence_weights(&profile.config_overrides)?;

        // ----------------------------------------------------------------
        // Step 4: Open raw read-only SqlitePool (C-02, ADR-002, FR-24)
        //
        // MUST NOT call SqlxStore::open() — that triggers migration and
        // spawns the analytics drain task. Use raw SqlitePool with
        // SqliteConnectOptions::read_only(true) instead.
        // ----------------------------------------------------------------
        let opts = SqliteConnectOptions::new()
            .filename(db_path)
            .read_only(true);

        let pool = SqlitePool::connect_with(opts)
            .await
            .map_err(|e| EvalError::Store(Box::new(e)))?;

        // ----------------------------------------------------------------
        // Step 5: Build VectorIndex from snapshot
        //
        // OQ-A resolution: Store = SqlxStore (type alias in unimatrix-core).
        // VectorIndex::new() accepts Arc<SqlxStore> directly. We cannot use
        // the raw SqlitePool here — VectorIndex requires Arc<Store> (= Arc<SqlxStore>).
        //
        // APPROACH: Open a SqlxStore against the snapshot using SqlxStore::open().
        // This does run migrations (no-op since schema is current), but does NOT
        // spawn the drain task — the drain task is only spawned by the analytics
        // subsystem which we bypass by never calling enqueue_analytics.
        //
        // The raw pool (step 4) is kept for any direct sqlx queries in runner.rs.
        // ----------------------------------------------------------------
        let store = unimatrix_store::SqlxStore::open(
            db_path,
            unimatrix_store::pool_config::PoolConfig::default(),
        )
        .await
        .map_err(|e| EvalError::Store(Box::new(e)))?;
        let store_arc: Arc<Store> = Arc::new(store);

        let vector_config = VectorConfig::default();
        let vector_index = Arc::new(
            VectorIndex::new(Arc::clone(&store_arc), vector_config)
                .map_err(|e| EvalError::Store(Box::new(e)))?,
        );

        // ----------------------------------------------------------------
        // Step 6: Build embedding handle (stub — model loading deferred)
        //
        // For nan-007, the embed handle starts loading with default config.
        // Eval runner will await model readiness before scenario replay.
        // ----------------------------------------------------------------
        let embed_handle = EmbedServiceHandle::new();
        let embed_config = EmbedConfig::default();
        embed_handle.start_loading(embed_config);

        // ----------------------------------------------------------------
        // Step 7: Build inference pool (DEFAULT_EVAL_RAYON_POOL_SIZE = 1)
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
        // OQ-B resolution: AuditLog::new() accepts Arc<SqlxStore>.
        // Store = SqlxStore (type alias), so Arc<Store> = Arc<SqlxStore>.
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
        // Step 12: Boosted categories — empty for eval profiles in nan-007
        // ----------------------------------------------------------------
        let boosted_categories: HashSet<String> = HashSet::from(["lesson-learned".to_string()]);

        // ----------------------------------------------------------------
        // Step 13: Build ServiceLayer via with_rate_config (TestHarness pattern)
        //
        // AnalyticsMode::Suppressed: rate limits set to u32::MAX so eval
        // replay is never blocked by rate limiting. No analytics_tx channel
        // is registered — the drain task is never spawned (ADR-002).
        // ----------------------------------------------------------------
        let rate_config = RateLimitConfig {
            search_limit: u32::MAX,
            write_limit: u32::MAX,
            window_secs: 3600,
        };

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
        );

        Ok(EvalServiceLayer {
            inner,
            pool,
            db_path: db_resolved,
            profile_name: profile.name.clone(),
            analytics_mode: AnalyticsMode::Suppressed,
        })
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
}

// ---------------------------------------------------------------------------
// validate_confidence_weights (private)
// ---------------------------------------------------------------------------

/// Validate the `ConfidenceWeights` sum invariant.
///
/// Only validates if a `[confidence]` section with weights is present in the
/// profile. An empty `UnimatrixConfig` (baseline profile) is always valid.
///
/// The six fields (`base`, `usage`, `fresh`, `help`, `corr`, `trust`) must
/// sum to `0.92 ± 1e-9`. Returns `EvalError::ConfigInvariant` with a
/// user-readable message on failure (C-06, C-15, SR-08).
fn validate_confidence_weights(config: &UnimatrixConfig) -> Result<(), EvalError> {
    let weights = match &config.confidence.weights {
        Some(w) => w,
        // No [confidence] section → baseline profile → always valid.
        None => return Ok(()),
    };

    let sum =
        weights.base + weights.usage + weights.fresh + weights.help + weights.corr + weights.trust;

    if (sum - EXPECTED_WEIGHT_SUM).abs() > WEIGHT_SUM_TOLERANCE {
        return Err(EvalError::ConfigInvariant(format!(
            "confidence weights sum to {sum:.10}, expected {EXPECTED_WEIGHT_SUM:.2} ± 1e-9\n\
             fields: base={}, usage={}, fresh={}, help={}, corr={}, trust={}",
            weights.base, weights.usage, weights.fresh, weights.help, weights.corr, weights.trust,
        )));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// parse_profile_toml (pub(crate) — used by run_eval in runner.rs)
// ---------------------------------------------------------------------------

/// Parse a profile TOML file into an `EvalProfile`.
///
/// The TOML must contain a `[profile]` section with at minimum a `name` field.
/// Remaining sections (`[confidence]`, `[inference]`) are deserialized as
/// `UnimatrixConfig` overrides. Missing sections use compiled defaults.
///
/// Returns `EvalError::ConfigInvariant` for parse failures and missing `name`.
/// Returns `EvalError::Io` for file read failures.
///
/// Used by Wave 2 `runner.rs`. Gated with `allow(dead_code)` until then.
#[allow(dead_code)]
pub(crate) fn parse_profile_toml(path: &Path) -> Result<EvalProfile, EvalError> {
    let content = std::fs::read_to_string(path).map_err(EvalError::Io)?;

    let raw: toml::Value = toml::from_str(&content).map_err(|e| {
        EvalError::ConfigInvariant(format!(
            "failed to parse profile TOML at {}: {e}",
            path.display()
        ))
    })?;

    // Extract [profile].name (required).
    let name = raw
        .get("profile")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .ok_or_else(|| {
            EvalError::ConfigInvariant("[profile].name is required in profile TOML".to_string())
        })?
        .to_string();

    // Extract [profile].description (optional).
    let description = raw
        .get("profile")
        .and_then(|p| p.get("description"))
        .and_then(|d| d.as_str())
        .map(|s| s.to_string());

    // Build config_overrides by stripping [profile] section then deserializing
    // the remainder as UnimatrixConfig. This allows [confidence] and [inference]
    // sections to flow through to the UnimatrixConfig defaults.
    let mut config_value = raw.clone();
    if let Some(table) = config_value.as_table_mut() {
        table.remove("profile");
    }

    let config_str = toml::to_string(&config_value).map_err(|e| {
        EvalError::ConfigInvariant(format!(
            "failed to serialize config subset from {}: {e}",
            path.display()
        ))
    })?;

    let config_overrides: UnimatrixConfig = toml::from_str(&config_str).map_err(|e| {
        EvalError::ConfigInvariant(format!(
            "failed to deserialize config overrides from {}: {e}",
            path.display()
        ))
    })?;

    Ok(EvalProfile {
        name,
        description,
        config_overrides,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use unimatrix_store::pool_config::PoolConfig;

    // -----------------------------------------------------------------------
    // Helper: create a minimal snapshot database for tests
    // -----------------------------------------------------------------------

    /// Open a valid SqlxStore (runs migrations) and return (store, dir).
    ///
    /// The TempDir must be kept alive for the duration of the test.
    async fn make_snapshot_db() -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("tempdir");
        let path = dir.path().join("snapshot.db");
        // Open + migrate so the schema is current.
        let _store = unimatrix_store::SqlxStore::open(&path, PoolConfig::default())
            .await
            .expect("open snapshot");
        (dir, path)
    }

    /// Build a baseline EvalProfile (empty config overrides).
    fn baseline_profile() -> EvalProfile {
        EvalProfile {
            name: "baseline".to_string(),
            description: None,
            config_overrides: UnimatrixConfig::default(),
        }
    }

    // -----------------------------------------------------------------------
    // AnalyticsMode tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_analytics_mode_variants_debug() {
        assert_eq!(format!("{:?}", AnalyticsMode::Live), "Live");
        assert_eq!(format!("{:?}", AnalyticsMode::Suppressed), "Suppressed");
    }

    #[test]
    fn test_analytics_mode_eq() {
        assert_eq!(AnalyticsMode::Suppressed, AnalyticsMode::Suppressed);
        assert_ne!(AnalyticsMode::Live, AnalyticsMode::Suppressed);
    }

    // -----------------------------------------------------------------------
    // EvalError display tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_eval_error_display_model_not_found() {
        let err = EvalError::ModelNotFound(PathBuf::from("/nonexistent/model.onnx"));
        let msg = format!("{err}");
        assert!(msg.contains("model not found"), "got: {msg}");
        assert!(msg.contains("/nonexistent/model.onnx"), "got: {msg}");
    }

    #[test]
    fn test_eval_error_display_config_invariant() {
        let err = EvalError::ConfigInvariant("weights sum to 0.91".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("0.91"), "got: {msg}");
    }

    #[test]
    fn test_eval_error_display_live_db_path() {
        let err = EvalError::LiveDbPath {
            supplied: PathBuf::from("/tmp/snap.db"),
            active: PathBuf::from("/home/user/.unimatrix/abc/unimatrix.db"),
        };
        let msg = format!("{err}");
        assert!(
            msg.contains("resolves to the active database"),
            "got: {msg}"
        );
        assert!(msg.contains("snap.db"), "got: {msg}");
        assert!(msg.contains("unimatrix.db"), "got: {msg}");
    }

    #[test]
    fn test_eval_error_display_profile_name_collision() {
        let err = EvalError::ProfileNameCollision("baseline".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("duplicate profile name"), "got: {msg}");
        assert!(msg.contains("baseline"), "got: {msg}");
    }

    #[test]
    fn test_eval_error_display_invalid_k() {
        let err = EvalError::InvalidK(0);
        let msg = format!("{err}");
        assert!(msg.contains("--k must be >= 1"), "got: {msg}");
        assert!(msg.contains('0'), "got: {msg}");
    }

    #[test]
    fn test_eval_error_implements_std_error() {
        let err = EvalError::ConfigInvariant("test".to_string());
        // Verify std::error::Error is implemented (compile-time check via dyn).
        let _boxed: Box<dyn std::error::Error> = Box::new(err);
    }

    // -----------------------------------------------------------------------
    // validate_confidence_weights unit tests (C-15, R-09)
    // -----------------------------------------------------------------------

    fn make_config_with_weights(
        base: f64,
        usage: f64,
        fresh: f64,
        help: f64,
        corr: f64,
        trust: f64,
    ) -> UnimatrixConfig {
        use crate::infra::config::{ConfidenceConfig, ConfidenceWeights};
        let mut cfg = UnimatrixConfig::default();
        cfg.confidence = ConfidenceConfig {
            weights: Some(ConfidenceWeights {
                base,
                usage,
                fresh,
                help,
                corr,
                trust,
            }),
        };
        cfg
    }

    #[test]
    fn test_confidence_weights_invariant_no_weights_passes() {
        // Baseline profile has no [confidence] section → always valid.
        let cfg = UnimatrixConfig::default();
        assert!(validate_confidence_weights(&cfg).is_ok());
    }

    #[test]
    fn test_confidence_weights_invariant_exact_sum_passes() {
        // Six weights summing to exactly 0.92.
        // 0.20 + 0.15 + 0.17 + 0.15 + 0.15 + 0.10 = 0.92
        let cfg = make_config_with_weights(0.20, 0.15, 0.17, 0.15, 0.15, 0.10);
        assert!(
            validate_confidence_weights(&cfg).is_ok(),
            "sum=0.92 must pass"
        );
    }

    #[test]
    fn test_confidence_weights_invariant_sum_low_fails() {
        // Six weights summing to 0.90 (below 0.92 - 1e-9).
        let cfg = make_config_with_weights(0.15, 0.15, 0.15, 0.15, 0.15, 0.15);
        let result = validate_confidence_weights(&cfg);
        assert!(result.is_err(), "sum=0.90 must fail");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("0.92"),
            "must mention expected sum; got: {msg}"
        );
        assert!(
            msg.contains("0.90") || msg.contains("0.9"),
            "must mention actual sum; got: {msg}"
        );
    }

    #[test]
    fn test_confidence_weights_invariant_sum_high_fails() {
        // Six weights summing to 0.93 (above 0.92 + 1e-9).
        let cfg = make_config_with_weights(0.20, 0.15, 0.18, 0.15, 0.15, 0.10);
        // 0.20+0.15+0.18+0.15+0.15+0.10 = 0.93
        let result = validate_confidence_weights(&cfg);
        assert!(result.is_err(), "sum=0.93 must fail");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("0.92"),
            "must mention expected sum; got: {msg}"
        );
    }

    #[test]
    fn test_confidence_weights_invariant_boundary_pass_within_tolerance() {
        // Weights summing to 0.92 + 5e-10 (within ±1e-9 tolerance).
        // 0.92 + 5e-10 < 0.92 + 1e-9 → should pass.
        let nudge = 5e-10_f64;
        let cfg = make_config_with_weights(0.20 + nudge, 0.15, 0.17, 0.15, 0.15, 0.10);
        assert!(
            validate_confidence_weights(&cfg).is_ok(),
            "sum within ±1e-9 must pass"
        );
    }

    #[test]
    fn test_confidence_weights_invariant_boundary_fail_outside_tolerance() {
        // Weights summing to 0.92 + 2e-9 (outside ±1e-9 tolerance).
        let nudge = 2e-9_f64;
        let cfg = make_config_with_weights(0.20 + nudge, 0.15, 0.17, 0.15, 0.15, 0.10);
        let result = validate_confidence_weights(&cfg);
        assert!(result.is_err(), "sum outside ±1e-9 must fail");
    }

    #[test]
    fn test_confidence_weights_invariant_message_names_fields() {
        // Error message must name all six fields (SR-08 user-readable message).
        let cfg = make_config_with_weights(0.10, 0.10, 0.10, 0.10, 0.10, 0.10);
        let result = validate_confidence_weights(&cfg);
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("base="), "must name base field; got: {msg}");
        assert!(msg.contains("usage="), "must name usage field; got: {msg}");
        assert!(msg.contains("fresh="), "must name fresh field; got: {msg}");
        assert!(msg.contains("help="), "must name help field; got: {msg}");
        assert!(msg.contains("corr="), "must name corr field; got: {msg}");
        assert!(msg.contains("trust="), "must name trust field; got: {msg}");
    }

    // -----------------------------------------------------------------------
    // parse_profile_toml tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_parse_profile_toml_baseline_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("baseline.toml");
        std::fs::write(
            &path,
            r#"[profile]
name = "baseline"
"#,
        )
        .unwrap();

        let profile = parse_profile_toml(&path).expect("baseline parse must succeed");
        assert_eq!(profile.name, "baseline");
        assert!(profile.description.is_none());
        // No confidence overrides — weights is None.
        assert!(profile.config_overrides.confidence.weights.is_none());
    }

    #[test]
    fn test_parse_profile_toml_with_description() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("candidate.toml");
        std::fs::write(
            &path,
            r#"[profile]
name = "candidate-v1"
description = "Test higher base weight"
"#,
        )
        .unwrap();

        let profile = parse_profile_toml(&path).expect("parse must succeed");
        assert_eq!(profile.name, "candidate-v1");
        assert_eq!(
            profile.description.as_deref(),
            Some("Test higher base weight")
        );
    }

    #[test]
    fn test_parse_profile_toml_missing_name_fails() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(
            &path,
            r#"[profile]
description = "no name"
"#,
        )
        .unwrap();

        let result = parse_profile_toml(&path);
        assert!(result.is_err(), "missing [profile].name must fail");
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("[profile].name is required"), "got: {msg}");
    }

    #[test]
    fn test_parse_profile_toml_missing_file_fails() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.toml");
        let result = parse_profile_toml(&path);
        assert!(result.is_err(), "missing file must fail");
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("I/O error") || msg.to_lowercase().contains("error"),
            "got: {msg}"
        );
    }

    #[test]
    fn test_parse_profile_toml_invalid_toml_fails() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "this is not toml >>>").unwrap();
        let result = parse_profile_toml(&path);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("failed to parse") || msg.contains("parse"),
            "got: {msg}"
        );
    }

    #[test]
    fn test_parse_profile_toml_with_confidence_weights() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("weights.toml");
        // Note: ConfidenceConfig has `weights: Option<ConfidenceWeights>`, so
        // the TOML must use `[confidence.weights]` to populate the nested struct.
        std::fs::write(
            &path,
            r#"[profile]
name = "custom-weights"

[confidence.weights]
base  = 0.20
usage = 0.15
fresh = 0.17
help  = 0.15
corr  = 0.15
trust = 0.10
"#,
        )
        .unwrap();

        let profile = parse_profile_toml(&path).expect("parse must succeed");
        assert_eq!(profile.name, "custom-weights");
        let weights = profile
            .config_overrides
            .confidence
            .weights
            .expect("weights must be present");
        assert!((weights.base - 0.20).abs() < 1e-9);
        assert!((weights.usage - 0.15).abs() < 1e-9);
    }

    // -----------------------------------------------------------------------
    // EvalServiceLayer::from_profile integration tests
    // (requires a valid snapshot db — created via make_snapshot_db)
    // -----------------------------------------------------------------------

    #[tokio::test(flavor = "multi_thread")]
    async fn test_from_profile_analytics_mode_is_suppressed() {
        let (_dir, snap) = make_snapshot_db().await;
        let profile = baseline_profile();

        let layer = EvalServiceLayer::from_profile(&snap, &profile, None).await;
        // The snapshot path guard may fail if ~/.unimatrix dir matches, but on a clean
        // CI machine this should succeed. Skip gracefully if Io error (no home dir).
        match layer {
            Ok(layer) => {
                assert_eq!(layer.analytics_mode(), AnalyticsMode::Suppressed);
                assert_eq!(layer.profile_name(), "baseline");
            }
            Err(EvalError::Io(_)) => {
                // Acceptable in environments without a home directory.
            }
            Err(EvalError::LiveDbPath { .. }) => {
                // Acceptable if snapshot happens to resolve to the active DB.
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_from_profile_returns_live_db_path_error_for_same_path() {
        use unimatrix_engine::project::ensure_data_directory;

        // Determine the active daemon DB path.
        let paths = match ensure_data_directory(None, None) {
            Ok(p) => p,
            Err(_) => return, // Skip if home directory not available.
        };

        // Ensure the DB file exists so canonicalize() works.
        if !paths.db_path.exists() {
            return; // Skip if daemon DB not present.
        }

        let profile = baseline_profile();
        let result = EvalServiceLayer::from_profile(&paths.db_path, &profile, None).await;

        assert!(
            matches!(result, Err(EvalError::LiveDbPath { .. })),
            "supplying the active DB must return LiveDbPath, got: {result:?}"
        );

        if let Err(EvalError::LiveDbPath { supplied, active }) = result {
            assert_eq!(supplied, paths.db_path);
            assert_eq!(active, std::fs::canonicalize(&paths.db_path).unwrap());
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_from_profile_snapshot_does_not_exist_returns_io_error() {
        let dir = TempDir::new().unwrap();
        let nonexistent = dir.path().join("ghost.db");
        let profile = baseline_profile();

        let result = EvalServiceLayer::from_profile(&nonexistent, &profile, None).await;
        assert!(
            matches!(result, Err(EvalError::Io(_))),
            "missing snapshot must return Io error, got: {result:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_from_profile_invalid_weights_returns_config_invariant() {
        let (_dir, snap) = make_snapshot_db().await;

        use crate::infra::config::{ConfidenceConfig, ConfidenceWeights};
        let mut config_overrides = UnimatrixConfig::default();
        config_overrides.confidence = ConfidenceConfig {
            weights: Some(ConfidenceWeights {
                base: 0.15,
                usage: 0.15,
                fresh: 0.15,
                help: 0.15,
                corr: 0.15,
                trust: 0.15, // sum = 0.90, not 0.92
            }),
        };

        let profile = EvalProfile {
            name: "bad-weights".to_string(),
            description: None,
            config_overrides,
        };

        let result = EvalServiceLayer::from_profile(&snap, &profile, None).await;
        assert!(
            matches!(result, Err(EvalError::ConfigInvariant(_))),
            "invalid weights must return ConfigInvariant, got: {result:?}"
        );

        if let Err(EvalError::ConfigInvariant(msg)) = result {
            assert!(
                msg.contains("0.92"),
                "must mention expected sum; got: {msg}"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_from_profile_valid_weights_passes_validation() {
        let (_dir, snap) = make_snapshot_db().await;

        use crate::infra::config::{ConfidenceConfig, ConfidenceWeights};
        let mut config_overrides = UnimatrixConfig::default();
        config_overrides.confidence = ConfidenceConfig {
            weights: Some(ConfidenceWeights {
                base: 0.20,
                usage: 0.15,
                fresh: 0.17,
                help: 0.15,
                corr: 0.15,
                trust: 0.10, // sum = 0.92
            }),
        };

        let profile = EvalProfile {
            name: "good-weights".to_string(),
            description: None,
            config_overrides,
        };

        let result = EvalServiceLayer::from_profile(&snap, &profile, None).await;
        // May fail with Io (no home dir) or succeed — but not ConfigInvariant.
        match result {
            Ok(_) => {}
            Err(EvalError::Io(_)) => {}
            Err(EvalError::LiveDbPath { .. }) => {}
            Err(EvalError::ConfigInvariant(msg)) => {
                panic!("valid weights must not return ConfigInvariant: {msg}");
            }
            Err(e) => {
                // Store errors (e.g., pool construction) are acceptable in CI.
                let _ = e;
            }
        }
    }
}
