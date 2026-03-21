# Config Extension — Pseudocode

**File**: `crates/unimatrix-server/src/infra/config.rs` (modified)

**Purpose**: Extend `InferenceConfig` with 10 NLI fields (all `#[serde(default)]`). Extend
`InferenceConfig::validate()` with range checks for all NLI fields plus the cross-field invariant
`nli_auto_quarantine_threshold > nli_contradiction_threshold` (ADR-007). Apply pool floor raise
when `nli_enabled = true` at startup (ADR-001). This file is modified — all existing fields and
behavior are preserved unchanged.

**Prerequisite**: Must be implemented first. All other crt-023 components depend on the extended
`InferenceConfig`.

---

## `InferenceConfig` Struct Extension

Add 10 new fields to the existing `InferenceConfig` struct. Existing `rayon_pool_size` field
is unchanged.

```
// In InferenceConfig (add after existing rayon_pool_size field):

/// NLI cross-encoder is active when true (default true).
/// When false, NliServiceHandle is constructed but never loads a model;
/// get_provider() immediately returns Err(NliNotReady). All search uses cosine fallback.
#[serde(default = "default_nli_enabled")]
pub nli_enabled: bool,               // default: true

/// Model variant identifier. Accepted values: "minilm2", "deberta".
/// None resolves to NliMiniLM2L6H768 at startup.
/// An unrecognized string fails validate() with a structured error (R-15, AC-17).
#[serde(default)]
pub nli_model_name: Option<String>,  // default: None (→ NliMiniLM2L6H768)

/// Explicit path to the ONNX model file. Overrides cache-dir resolution when set.
/// When set alongside nli_model_name, the path is used but the model's tokenizer
/// is still loaded from the same directory (ADR-003).
#[serde(default)]
pub nli_model_path: Option<PathBuf>, // default: None (auto-resolved from cache)

/// SHA-256 hash of the NLI model file as a 64-char lowercase hex string.
/// When set, hash is verified before ONNX session construction (NFR-09, ADR-003).
/// When None, hash verification is skipped with a warn-level log (R-05).
/// Validated at startup: must be exactly 64 hex characters if set (AC-17).
#[serde(default)]
pub nli_model_sha256: Option<String>, // default: None

/// Candidate pool size for NLI search re-ranking.
/// HNSW retrieves nli_top_k candidates; NLI scores all of them before truncating to k.
/// Distinct from nli_post_store_k (D-04, AC-19).
#[serde(default = "default_nli_top_k")]
pub nli_top_k: usize,                // default: 20, range [1, 100]

/// Neighbor count for post-store NLI detection.
/// After context_store, the NLI task queries nli_post_store_k HNSW neighbors.
/// Distinct from nli_top_k (D-04, AC-19).
#[serde(default = "default_nli_post_store_k")]
pub nli_post_store_k: usize,         // default: 10, range [1, 100]

/// Entailment threshold for writing Supports edges.
/// Pairs with nli_scores.entailment > nli_entailment_threshold produce a Supports edge.
/// Strict inequality: score must EXCEED this value (FR-18, R-03 boundary check).
#[serde(default = "default_nli_entailment_threshold")]
pub nli_entailment_threshold: f32,   // default: 0.6, range (0.0, 1.0) exclusive

/// Contradiction threshold for writing Contradicts edges.
/// Pairs with nli_scores.contradiction > nli_contradiction_threshold produce a Contradicts edge.
/// Strict inequality: score must EXCEED this value.
#[serde(default = "default_nli_contradiction_threshold")]
pub nli_contradiction_threshold: f32, // default: 0.6, range (0.0, 1.0) exclusive

/// Per-call cap on total edges written during run_post_store_nli.
/// Counts BOTH Supports AND Contradicts edges combined (FR-22, R-09, AC-13, AC-23).
/// Named "max_contradicts_per_tick" for config compatibility with SCOPE.md.
/// Implementation comments must note: semantic unit is per context_store call, not per tick.
#[serde(default = "default_max_contradicts_per_tick")]
pub max_contradicts_per_tick: usize, // default: 10, range [1, 100]

/// Auto-quarantine threshold for entries penalized ONLY by NLI-origin Contradicts edges.
/// Must be strictly greater than nli_contradiction_threshold (ADR-007).
/// Validated cross-field at startup; violation aborts with structured error naming both fields.
/// Entries penalized by a mix of NLI-origin and manually-curated edges use existing logic.
#[serde(default = "default_nli_auto_quarantine_threshold")]
pub nli_auto_quarantine_threshold: f32, // default: 0.85, range (0.0, 1.0) exclusive
```

---

## Default Value Functions

These are required because `#[serde(default = "fn_name")]` only works with free functions.

```
fn default_nli_enabled()                -> bool  { true }
fn default_nli_top_k()                  -> usize { 20 }
fn default_nli_post_store_k()           -> usize { 10 }
fn default_nli_entailment_threshold()   -> f32   { 0.6 }
fn default_nli_contradiction_threshold()-> f32   { 0.6 }
fn default_max_contradicts_per_tick()   -> usize { 10 }
fn default_nli_auto_quarantine_threshold() -> f32 { 0.85 }
```

---

## `InferenceConfig::Default` Extension

The existing `Default` impl must be updated to include all new fields.

```
impl Default for InferenceConfig {
    fn default() -> Self {
        InferenceConfig {
            rayon_pool_size: (num_cpus::get() / 2).max(4).min(8),  // existing, unchanged
            nli_enabled: true,
            nli_model_name: None,
            nli_model_path: None,
            nli_model_sha256: None,
            nli_top_k: 20,
            nli_post_store_k: 10,
            nli_entailment_threshold: 0.6,
            nli_contradiction_threshold: 0.6,
            max_contradicts_per_tick: 10,
            nli_auto_quarantine_threshold: 0.85,
        }
    }
}
```

---

## `InferenceConfig::validate` Extension

Extend the existing validate method. Existing `rayon_pool_size` range check `[1, 64]` is unchanged.

```
impl InferenceConfig {
    pub fn validate(&self, path: &Path) -> Result<(), ConfigError> {
        // -- Existing check (unchanged) --
        if self.rayon_pool_size < 1 OR self.rayon_pool_size > 64:
            return Err(ConfigError::InferencePoolSizeOutOfRange { path, value: self.rayon_pool_size })

        // -- NLI field range checks --

        // nli_top_k: [1, 100]
        if self.nli_top_k < 1 OR self.nli_top_k > 100:
            return Err(ConfigError::NliFieldOutOfRange {
                path, field: "nli_top_k", value: self.nli_top_k.to_string(),
                reason: "must be in range [1, 100]"
            })

        // nli_post_store_k: [1, 100]
        if self.nli_post_store_k < 1 OR self.nli_post_store_k > 100:
            return Err(ConfigError::NliFieldOutOfRange {
                path, field: "nli_post_store_k", value: self.nli_post_store_k.to_string(),
                reason: "must be in range [1, 100]"
            })

        // nli_entailment_threshold: (0.0, 1.0) exclusive
        if self.nli_entailment_threshold <= 0.0 OR self.nli_entailment_threshold >= 1.0:
            return Err(ConfigError::NliFieldOutOfRange {
                path, field: "nli_entailment_threshold",
                value: self.nli_entailment_threshold.to_string(),
                reason: "must be in range (0.0, 1.0) exclusive"
            })

        // nli_contradiction_threshold: (0.0, 1.0) exclusive
        if self.nli_contradiction_threshold <= 0.0 OR self.nli_contradiction_threshold >= 1.0:
            return Err(ConfigError::NliFieldOutOfRange {
                path, field: "nli_contradiction_threshold",
                value: self.nli_contradiction_threshold.to_string(),
                reason: "must be in range (0.0, 1.0) exclusive"
            })

        // max_contradicts_per_tick: [1, 100]
        if self.max_contradicts_per_tick < 1 OR self.max_contradicts_per_tick > 100:
            return Err(ConfigError::NliFieldOutOfRange {
                path, field: "max_contradicts_per_tick",
                value: self.max_contradicts_per_tick.to_string(),
                reason: "must be in range [1, 100]"
            })

        // nli_auto_quarantine_threshold: (0.0, 1.0) exclusive
        if self.nli_auto_quarantine_threshold <= 0.0 OR self.nli_auto_quarantine_threshold >= 1.0:
            return Err(ConfigError::NliFieldOutOfRange {
                path, field: "nli_auto_quarantine_threshold",
                value: self.nli_auto_quarantine_threshold.to_string(),
                reason: "must be in range (0.0, 1.0) exclusive"
            })

        // nli_model_name: must be a recognized variant if set
        if let Some(ref name) = self.nli_model_name:
            if NliModel::from_config_name(name).is_none():
                return Err(ConfigError::NliFieldOutOfRange {
                    path, field: "nli_model_name",
                    value: name.clone(),
                    reason: "unrecognized model name; valid values: minilm2, deberta"
                })

        // nli_model_sha256: must be exactly 64 hex chars if set
        if let Some(ref sha) = self.nli_model_sha256:
            if sha.len() != 64 OR !sha.chars().all(|c| c.is_ascii_hexdigit()):
                return Err(ConfigError::NliFieldOutOfRange {
                    path, field: "nli_model_sha256",
                    value: format!("{} ({} chars)", sha, sha.len()),
                    reason: "must be a 64-character lowercase hex string"
                })

        // -- Cross-field invariant (ADR-007): nli_auto_quarantine_threshold > nli_contradiction_threshold
        // Violation aborts startup naming BOTH fields (AC-17, last bullet).
        if self.nli_auto_quarantine_threshold <= self.nli_contradiction_threshold:
            return Err(ConfigError::NliThresholdInvariantViolated {
                path,
                auto_quarantine: self.nli_auto_quarantine_threshold,
                contradiction:   self.nli_contradiction_threshold,
            })

        Ok(())
    }
```

---

## Pool Floor Application

Applied in server startup (not in validate()). After validate() passes and before
`NliServiceHandle::start_loading()` is called.

```
// In main.rs / startup wiring, after loading config and calling validate():
if config.inference.nli_enabled {
    // ADR-001: raise pool floor to 6 when NLI is enabled (from default max(4, formula))
    // Cap at 8 (existing ADR-003 upper bound).
    // Operator override: if rayon_pool_size was explicitly set >= 6, use it as-is.
    config.inference.rayon_pool_size = config.inference.rayon_pool_size.max(6).min(8)
}
// Then construct RayonPool with the final rayon_pool_size value.
```

**Note**: The pool floor is applied AFTER validate() (which checks rayon_pool_size in [1,64]).
An operator setting `rayon_pool_size = 4` with `nli_enabled = true` will see the pool raised
to 6 silently. This is intentional (ADR-001). A tracing::debug! should note the raise.

---

## New `ConfigError` Variants

Add to the existing `ConfigError` enum (wherever it is defined in config.rs):

```
// Add to ConfigError:

/// An NLI config field is outside its valid range.
NliFieldOutOfRange {
    path:   PathBuf,
    field:  &'static str,   // field name for operator diagnosis
    value:  String,          // actual value (for display)
    reason: &'static str,   // human-readable valid range description
}

/// nli_auto_quarantine_threshold is not strictly greater than nli_contradiction_threshold.
/// Names both fields in the error message (ADR-007, AC-17).
NliThresholdInvariantViolated {
    path:             PathBuf,
    auto_quarantine:  f32,
    contradiction:    f32,
}
```

Add to `ConfigError` Display impl:
```
NliFieldOutOfRange { field, value, reason, path } ->
    "config {}: field '{}' = '{}' is invalid: {}", path.display(), field, value, reason

NliThresholdInvariantViolated { path, auto_quarantine, contradiction } ->
    "config {}: nli_auto_quarantine_threshold ({}) must be strictly greater than nli_contradiction_threshold ({})",
    path.display(), auto_quarantine, contradiction
```

---

## `InferenceConfig::nli_config_for_handle` (helper method)

```
/// Extract NliConfig for NliServiceHandle construction.
/// Called during server startup after validate() passes.
pub fn nli_config_for_handle(&self, cache_dir: PathBuf) -> NliConfig
    NliConfig {
        nli_enabled:      self.nli_enabled,
        nli_model_name:   self.nli_model_name.clone(),
        nli_model_path:   self.nli_model_path.clone(),
        nli_model_sha256: self.nli_model_sha256.clone(),
        cache_dir,
    }
```

---

## Error Handling

| Violation | Error | Behavior |
|-----------|-------|----------|
| `rayon_pool_size` outside `[1, 64]` | `InferencePoolSizeOutOfRange` | Startup abort |
| Any NLI field outside range | `NliFieldOutOfRange` | Startup abort, field named in message |
| `nli_model_name` unrecognized string | `NliFieldOutOfRange` | Startup abort |
| `nli_model_sha256` not 64 hex chars | `NliFieldOutOfRange` | Startup abort |
| `nli_auto_quarantine_threshold <= nli_contradiction_threshold` | `NliThresholdInvariantViolated` | Startup abort, both fields named |
| Missing `[inference]` section in TOML | No error | All defaults applied via `#[serde(default)]` |

---

## Key Test Scenarios

1. **AC-07 / empty inference section**: Deserialize `[inference]` section with no NLI fields; assert each field has its documented default value.
2. **AC-17 / range checks**: One test per out-of-range value for all 7 validated fields; assert startup error names the offending field.
3. **AC-17 / cross-field invariant**: Set `nli_auto_quarantine_threshold = 0.6, nli_contradiction_threshold = 0.6`; assert error names both fields.
4. **AC-17 / nli_model_name unrecognized**: Set `nli_model_name = "gpt4"`; assert error names `nli_model_name` and includes the invalid value (R-15).
5. **AC-17 / nli_model_sha256 wrong length**: Set 63-char hex; assert validation error.
6. **R-02 / pool floor**: After validate() with `nli_enabled = true` and `rayon_pool_size = 4`, apply floor; assert `rayon_pool_size >= 6`.
7. **R-02 / pool floor not raised when disabled**: With `nli_enabled = false`, pool floor is NOT raised to 6.
8. **AC-19 / nli_top_k vs nli_post_store_k**: Assert the two fields are independent in the struct (setting one does not affect the other).
