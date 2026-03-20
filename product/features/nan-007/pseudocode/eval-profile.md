# Pseudocode: eval/profile.rs

**Location**: `crates/unimatrix-server/src/eval/profile.rs`

## Purpose

Defines the type-level foundation for the eval engine:
- `AnalyticsMode` — structural guarantee that eval never writes analytics (ADR-002)
- `EvalProfile` — parsed profile TOML with override config
- `EvalServiceLayer` — restricted ServiceLayer variant for eval replay
- `EvalError` — all structured errors for the eval subsystem

This module is the construction gateway. All other eval modules depend on it.
`EvalServiceLayer::from_profile()` is the one place where DB pool opens and invariant
validation occur. If construction returns `Ok`, the layer is safe to use.

## Dependencies

| Dependency | Location | Role |
|------------|----------|------|
| `ServiceLayer::with_rate_config` | `crates/unimatrix-server/src/services/mod.rs` | Construction path (TestHarness pattern) |
| `sqlx::SqlitePool`, `SqliteConnectOptions` | sqlx | Raw read-only pool |
| `VectorIndex::new` / `VectorIndex::load` | `unimatrix-core` | Index construction from snapshot |
| `EmbedServiceHandle` | `crates/unimatrix-server/src/infra/embed_handle.rs` | Inference handle |
| `RayonPool` | `crates/unimatrix-server/src/infra/rayon_pool.rs` | ML inference pool |
| `AdaptationService` | `unimatrix-adapt` | Adaptation (read path only) |
| `UnimatrixConfig`, `ConfidenceWeights` | `crates/unimatrix-server/src/infra/config.rs` | Profile TOML parsing |
| `ProjectPaths.db_path` | `unimatrix_engine::project` | Live-DB path guard |
| `std::fs::canonicalize` | stdlib | Live-DB path guard |
| `AuditLog` | `crates/unimatrix-server/src/infra/audit.rs` | Required by ServiceLayer ctor |
| `UsageDedup` | `crates/unimatrix-server/src/infra/usage_dedup.rs` | Required by ServiceLayer ctor |
| `VectorAdapter`, `AsyncVectorStore` | `unimatrix-core` | Required by ServiceLayer ctor |

## Types

### `AnalyticsMode`

```
pub enum AnalyticsMode {
    /// Normal SqlxStore behaviour — drain task active, analytics writes occur.
    /// Present for future use (eval live mode). NOT used in nan-007.
    Live,
    /// No drain task spawned, enqueue_analytics calls are no-ops.
    /// Always used in EvalServiceLayer construction (ADR-002, SR-07).
    Suppressed,
}
```

### `EvalProfile`

```
pub struct EvalProfile {
    pub name: String,
    pub description: Option<String>,
    /// Subset of UnimatrixConfig fields that override compiled defaults.
    /// An empty TOML → all compiled defaults → baseline profile.
    pub config_overrides: UnimatrixConfig,
}
```

Profile TOML format:
```toml
[profile]
name = "candidate-weights-v1"
description = "Test higher helpfulness weight"   # optional

[confidence]
# All six weight fields required if [confidence] section present (C-06):
base_recency     = 0.20
base_helpfulness = 0.20
base_correction  = 0.15
base_usage       = 0.15
base_coherence   = 0.12
base_embedding   = 0.10
# sum = 0.92 exactly

[inference]
# Optional section; model path validated at from_profile() time (C-14):
# nli_model = "/path/to/model.onnx"  -- not in scope for nan-007
```

### `EvalServiceLayer`

```
pub struct EvalServiceLayer {
    pub(crate) inner: ServiceLayer,
    pub(crate) db_path: PathBuf,
    pub(crate) profile_name: String,
    // analytics_mode is always Suppressed for eval:
    // the drain task is never spawned; this is enforced by construction (ADR-002).
    // The ServiceLayer.inner does not have an analytics channel registered.
}
```

### `EvalError`

```
pub enum EvalError {
    /// A model file referenced in [inference] section is missing or unreadable.
    /// Returned at from_profile() time, never at inference time (C-14, FR-23).
    ModelNotFound(PathBuf),

    /// ConfidenceWeights invariant violated: sum != 0.92 ± 1e-9 or missing fields.
    /// String is a user-readable message naming expected/actual sums (C-06, SR-08).
    ConfigInvariant(String),

    /// --db path (eval run) or --out path (snapshot) resolves to the active daemon DB.
    /// Both resolved paths named in the error (C-13, FR-44, ADR-001).
    LiveDbPath { supplied: PathBuf, active: PathBuf },

    /// I/O errors (file open, read, write).
    Io(std::io::Error),

    /// Store/SQLx errors from pool construction or query execution.
    Store(Box<dyn std::error::Error + Send + Sync>),

    /// Two profile TOMLs share the same [profile].name field.
    ProfileNameCollision(String),

    /// --k argument is 0; P@K undefined (RISK-TEST-STRATEGY edge case).
    InvalidK(usize),
}

impl std::fmt::Display for EvalError:
  -- ModelNotFound(p): "model not found: {}", p.display()
  -- ConfigInvariant(msg): "{}", msg
  -- LiveDbPath { supplied, active }: "eval db path resolves to the active database\n  supplied (resolved): {}\n  active:              {}\n  use a snapshot, not the live database", supplied.display(), active.display()
  -- Io(e): "I/O error: {e}"
  -- Store(e): "store error: {e}"
  -- ProfileNameCollision(name): "duplicate profile name \"{name}\" — two profile TOMLs share the same [profile].name"
  -- InvalidK(k): "--k must be >= 1, got {k}"

impl std::error::Error for EvalError -- standard derivation
```

## Function: `EvalServiceLayer::from_profile`

```
pub async fn from_profile(
    db_path: &Path,
    profile: &EvalProfile,
    project_dir: Option<&Path>,
) -> Result<EvalServiceLayer, EvalError>

BODY:
  1. Live-DB path guard (C-13, FR-44, ADR-001):
       paths = project::ensure_data_directory(project_dir, None)
                 .map_err(|e| EvalError::Io(e.into()))?
       active_db = canonicalize(paths.db_path)
                     .map_err(|e| EvalError::Io(e))?
       db_resolved = canonicalize(db_path)
                       .map_err(|e| EvalError::Io(e))?
       if db_resolved == active_db:
         return Err(EvalError::LiveDbPath {
           supplied: db_path.to_path_buf(),
           active: active_db,
         })

  2. Validate [inference] model paths (C-14, FR-23, SR-09):
       if profile.config_overrides.inference.nli_model is Some(path):
         if !path.exists() || !path.is_file():
           return Err(EvalError::ModelNotFound(path.clone()))
         -- Check readability: attempt std::fs::File::open(path)
         --   if Err: return Err(EvalError::ModelNotFound(path.clone()))

  3. Validate ConfidenceWeights if [confidence] overrides present (C-06, FR-18, SR-08):
       validate_confidence_weights(&profile.config_overrides)?
       -- see validate_confidence_weights function below

  4. Open raw read-only SqlitePool (C-02, ADR-002, FR-24):
       opts = SqliteConnectOptions::new()
                .filename(db_path)
                .read_only(true)
       pool = SqlitePool::connect_with(opts).await
                .map_err(|e| EvalError::Store(Box::new(e)))?

  5. Build vector index from snapshot (one per profile, FR architecture decision):
       vector_config = VectorConfig::default()  -- or from profile.config_overrides if applicable
       store_arc = build_read_only_store_wrapper(pool.clone())
         -- NOTE: The eval engine cannot use SqlxStore::open() (C-02).
         --   VectorIndex::new() requires Arc<Store>. Two paths:
         --   Path A: If VectorIndex accepts raw SqlitePool — use that.
         --   Path B: If VectorIndex requires Arc<Store> — wrap pool in a minimal
         --           Store-compatible adapter that provides read-only access to
         --           VECTOR_MAP and ENTRIES tables.
         -- IMPLEMENTATION GAP: The exact Store wrapper approach must be confirmed
         --   by the implementer inspecting VectorIndex::new() and VectorIndex::load()
         --   signatures. See Open Questions at end of this file.
       meta_path = db_path.parent()?.join("unimatrix-vector.meta")
         -- For a snapshot, the vector meta is embedded in the snapshot SQLite file,
         --   not in a separate directory. VectorIndex must be reconstructed from scratch
         --   using VECTOR_MAP table rows in the snapshot.
       vector_index = Arc::new(
         VectorIndex::new(store_arc, vector_config)
           .map_err(|e| EvalError::Store(Box::new(e)))?
       )

  6. Build embedding handle:
       embed_handle = EmbedServiceHandle::new()
       embed_config = derive_embed_config_from_profile(profile)
         -- uses profile.config_overrides.inference fields if present,
         --   otherwise EmbedConfig::default()
       embed_handle.start_loading(embed_config)
       -- NOTE: Eval runner will call embed_handle.get_adapter().await before
       --   any scenario replay to ensure model is loaded.
       --   Failure here produces EvalError::ModelNotFound via get_adapter() error.

  7. Build inference pool:
       rayon_pool_size = profile.config_overrides.inference.rayon_pool_size
                         .unwrap_or(DEFAULT_EVAL_RAYON_POOL_SIZE)
         -- DEFAULT_EVAL_RAYON_POOL_SIZE = 1 for eval (memory efficiency)
       rayon_pool = Arc::new(
         RayonPool::new(rayon_pool_size, &format!("eval-{}", profile.name))
           .map_err(|e| EvalError::Store(Box::new(e)))?
       )

  8. Build adaptation service:
       adapt_svc = Arc::new(AdaptationService::new(AdaptConfig::default()))

  9. Build AuditLog (required by ServiceLayer ctor, no-op for eval):
       -- AuditLog writes to the store; since store is read-only, writes will fail
       --   but AuditLog::new() itself does not write.
       -- Construction succeeds; audit writes are silently dropped.
       audit = Arc::new(AuditLog::new_from_pool(pool.clone()))
         -- IMPLEMENTATION GAP: AuditLog::new() currently accepts Arc<Store>.
         --   If it cannot accept a raw pool, implementer must use the same
         --   store_arc wrapper from step 5. See Open Questions.

  10. Build UsageDedup:
        usage_dedup = Arc::new(UsageDedup::new())

  11. Build VectorAdapter and AsyncVectorStore:
        vector_adapter = VectorAdapter::new(Arc::clone(&vector_index))
        async_vector_store = Arc::new(AsyncVectorStore::new(Arc::new(vector_adapter)))

  12. Derive boosted_categories from profile (or empty set for baseline):
        boosted_categories: HashSet<String> = HashSet::new()
        -- Eval profiles do not override category boosting in nan-007.
        --   Future profiles may add [knowledge] section for this.

  13. Build ServiceLayer via with_rate_config (TestHarness pattern, ADR-002):
        -- AnalyticsMode::Suppressed: pass rate_config with u32::MAX limits
        --   and empty boosted_categories. Do NOT wire analytics_tx channel.
        --   The ServiceLayer constructed here does not call SqlxStore::open(),
        --   so no drain task is ever spawned.
        rate_config = RateLimitConfig {
          search_limit: u32::MAX,
          write_limit: u32::MAX,
          window_secs: 3600,
        }
        inner = ServiceLayer::with_rate_config(
          Arc::clone(&store_arc),        -- store (read-only wrapper)
          Arc::clone(&vector_index),
          Arc::clone(&async_vector_store),
          Arc::clone(&store_arc),        -- entry_store (same read-only wrapper)
          Arc::clone(&embed_handle),
          Arc::clone(&adapt_svc),
          Arc::clone(&audit),
          Arc::clone(&usage_dedup),
          rate_config,
          HashSet::from(["lesson-learned".to_string()]),  -- default, mirrors TestHarness
          Arc::clone(&rayon_pool),
        )

  14. Return Ok:
        Ok(EvalServiceLayer {
          inner,
          db_path: db_path.to_path_buf(),
          profile_name: profile.name.clone(),
        })
```

## Function: `fn validate_confidence_weights` (private)

```
fn validate_confidence_weights(
    config: &UnimatrixConfig,
) -> Result<(), EvalError>

BODY:
  -- Only validate if [confidence] section is explicitly provided.
  -- An empty UnimatrixConfig (baseline profile) is always valid.
  if config has no confidence overrides (all fields are at compiled defaults):
    return Ok(())

  weights = config.confidence  -- ConfidenceWeights struct

  -- Check all six fields are present (non-None / non-default-sentinel):
  -- NOTE: If ConfidenceWeights uses Option<f64> for each field to detect
  --   "not provided" vs "explicitly set", check for None here.
  --   If it uses f64 with a default of 0.0, check for zero sum separately.
  -- Implementation must match the actual ConfidenceWeights struct layout.

  sum = weights.base_recency
      + weights.base_helpfulness
      + weights.base_correction
      + weights.base_usage
      + weights.base_coherence
      + weights.base_embedding

  const EXPECTED_SUM: f64 = 0.92;
  const TOLERANCE: f64 = 1e-9;

  if (sum - EXPECTED_SUM).abs() > TOLERANCE:
    return Err(EvalError::ConfigInvariant(format!(
      "confidence weights sum to {:.10}, expected {:.2} ± 1e-9\n\
       fields: base_recency={}, base_helpfulness={}, base_correction={}, \
       base_usage={}, base_coherence={}, base_embedding={}",
      sum, EXPECTED_SUM,
      weights.base_recency, weights.base_helpfulness, weights.base_correction,
      weights.base_usage, weights.base_coherence, weights.base_embedding,
    )))

  return Ok(())
```

## Function: `pub fn parse_profile_toml` (private, used by run_eval)

```
pub(crate) fn parse_profile_toml(
    path: &Path,
) -> Result<EvalProfile, EvalError>

BODY:
  content = std::fs::read_to_string(path)
              .map_err(|e| EvalError::Io(e))?
  raw: toml::Value = toml::from_str(&content)
                       .map_err(|e| EvalError::ConfigInvariant(format!(
                         "failed to parse profile TOML at {}: {e}", path.display()
                       )))?

  -- Extract [profile] section:
  name = raw["profile"]["name"].as_str()
           .ok_or(EvalError::ConfigInvariant(
             "[profile].name is required in profile TOML".to_string()
           ))?.to_string()

  description = raw["profile"]["description"].as_str().map(|s| s.to_string())

  -- Deserialize config_overrides from remaining TOML sections:
  --   Remove the [profile] key, then deserialize the rest as UnimatrixConfig subset.
  --   Empty remaining TOML → UnimatrixConfig::default() (baseline).
  config_overrides: UnimatrixConfig = deserialize_config_subset(raw)?

  return Ok(EvalProfile { name, description, config_overrides })
```

## Error Handling

| Call Site | Error Path |
|-----------|-----------|
| `canonicalize` on active DB path | `EvalError::Io` — usually means daemon not initialized |
| `canonicalize` on supplied `db_path` | `EvalError::Io` — snapshot file does not exist |
| `db_path == active_db` | `EvalError::LiveDbPath` — never proceeds to pool open |
| Inference model path missing | `EvalError::ModelNotFound` — caught before any inference |
| ConfidenceWeights sum invalid | `EvalError::ConfigInvariant` — user-readable message |
| `SqlitePool::connect_with` fails | `EvalError::Store` |
| `VectorIndex::new` fails | `EvalError::Store` |
| `RayonPool::new` fails | `EvalError::Store` |

No panics permitted in any code path through this module (FR-23, SR-09).

## Key Test Scenarios

1. **Baseline profile (empty TOML)**: parse_profile_toml succeeds; from_profile returns
   Ok; ServiceLayer constructed with AnalyticsMode::Suppressed; drain task never spawned.

2. **ConfidenceWeights sum = 0.91**: validate_confidence_weights returns
   `EvalError::ConfigInvariant`; message names 0.92 as expected and 0.91 as actual (R-09).

3. **ConfidenceWeights sum = 0.92 exactly**: validate_confidence_weights returns Ok.

4. **Boundary: sum = 0.92 + 1e-9**: passes (within tolerance).

5. **Boundary: sum = 0.92 + 2e-9**: fails with ConfigInvariant (R-09).

6. **Missing model path**: from_profile returns `EvalError::ModelNotFound(path)`;
   construction does not proceed to pool open (C-14, R-10).

7. **db_path == active_db**: from_profile returns `EvalError::LiveDbPath`; both paths
   in error message (AC-16, R-06).

8. **No drain task spawned**: after from_profile(), assert no tokio task with name
   matching "drain" or "analytics" is spawned (R-01).

9. **Profile name collision**: caller (run_eval) detects collision; EvalError::ProfileNameCollision
   returned before any from_profile() call.

## Open Questions for Implementer

**OQ-A**: `VectorIndex::new()` requires `Arc<Store>`. Can a read-only eval use the
raw `sqlx::SqlitePool` directly, or is a Store-compatible wrapper needed? Inspect
`unimatrix_core::VectorIndex::new()` signature before implementing step 5. If a wrapper
is needed, it should be a minimal struct in `eval/profile.rs` that implements only the
trait methods VectorIndex requires for construction (VECTOR_MAP reads).

**OQ-B**: `AuditLog::new()` currently accepts `Arc<Store>`. If it cannot accept a raw
pool, use the same Store wrapper from OQ-A. Writes will fail silently since the pool is
read-only, which is acceptable for eval (the AuditLog call sites should not be called
in the eval search path, but if they are, silent failure is preferred over panic).

**OQ-C**: `ServiceLayer::with_rate_config()` signature — verify parameter order matches
TestHarness::new() in test_support.rs. The TestHarness is the canonical model.

These gaps require inspection of existing crate internals, not new design decisions.

## Knowledge Stewardship

Queried: /uni-query-patterns for "evaluation harness patterns conventions" (category: pattern) — 5 results; #1042 (Pure Computation Engine Module Pattern) is relevant: EvalServiceLayer is a pure-construction module with no IO side-effects after from_profile() returns Ok. Pattern followed.
Queried: /uni-query-patterns for "nan-007 architectural decisions" (category: decision, topic: nan-007) — ADR-002 (#2585, AnalyticsMode::Suppressed) and ADR-001 (#2602, live-DB path guard) directly govern this module. Both followed exactly in from_profile() steps 1 and 13.
Queried: /uni-query-patterns for "block_export_sync async bridge pattern" — #61 and #13 (Synchronous API with spawn_blocking Delegation) confirm the from_profile() async wrapper approach is consistent with established codebase conventions.
Stored: nothing novel to store — pseudocode agents are read-only; patterns are consumed not created
