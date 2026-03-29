# config_ppr_fields.md — InferenceConfig PPR Field Extension

## Purpose

Add five new fields to `InferenceConfig` in `crates/unimatrix-server/src/infra/config.rs`
for the PPR step (crt-030). Follows the established pattern used by NLI and crt-029 fields:
- `#[serde(default = "default_fn_name")]` attribute on each field
- Private `fn default_ppr_*()` returning the compiled default
- Range check in `InferenceConfig::validate()`
- Field initialized in `Default::default()` block
- Field merged in the global+project config merge block

No schema changes. No new dependency. No changes outside `config.rs`.

---

## New Fields in `InferenceConfig` (in the `// crt-030` section)

Add a new section after the existing `// col-031` section, before the closing `}`:

```
    // -----------------------------------------------------------------------
    // Personalized PageRank fields (crt-030)
    // -----------------------------------------------------------------------

    /// Damping factor α for Personalized PageRank power iteration.
    ///
    /// At each step, proportion α of relevance mass flows through graph edges;
    /// proportion (1 - α) teleports back to the personalization (seed) distribution.
    ///
    /// Higher α: more diffusion through graph, lower personalization recall.
    /// Lower α: mass stays closer to seeds.
    ///
    /// Default: 0.85. Valid range: (0.0, 1.0) exclusive.
    /// Distinct from crt-029 tick fields (supports_candidate_threshold etc.) —
    /// PPR operates at query time on the pre-built TypedRelationGraph.
    #[serde(default = "default_ppr_alpha")]
    pub ppr_alpha: f64,

    /// Number of power-iteration steps.
    ///
    /// Runs exactly this many steps — no early-exit convergence check.
    /// Determinism requirement (ADR-004 crt-030): fixed count ensures identical
    /// outputs for identical inputs across process restarts.
    ///
    /// Default: 20. Valid range: [1, 100] inclusive.
    #[serde(default = "default_ppr_iterations")]
    pub ppr_iterations: usize,

    /// PPR score floor for injecting new entries into the candidate pool.
    ///
    /// An entry NOT already in the HNSW pool is injected only if its PPR score
    /// strictly exceeds this threshold (> not >=, AC-13 crt-030).
    ///
    /// Default: 0.05. Valid range: (0.0, 1.0) exclusive.
    #[serde(default = "default_ppr_inclusion_threshold")]
    pub ppr_inclusion_threshold: f64,

    /// PPR trust weight — dual role (ADR-007 crt-030):
    ///
    /// Role 1 (blend for existing HNSW candidates):
    ///   new_sim = (1 - ppr_blend_weight) * hnsw_sim + ppr_blend_weight * ppr_score
    ///
    /// Role 2 (initial similarity for PPR-only injected entries):
    ///   initial_sim = ppr_blend_weight * ppr_score
    ///
    /// Both roles express "how much to trust the PPR signal." The dual role is
    /// intentional; a separate ppr_inject_weight is deferred (ADR-007).
    ///
    /// Default: 0.15. Valid range: [0.0, 1.0] inclusive.
    /// NOTE: This field does NOT add a new FusionWeights term — PPR influence
    /// enters only through pool expansion and the similarity field.
    #[serde(default = "default_ppr_blend_weight")]
    pub ppr_blend_weight: f64,

    /// Maximum number of PPR-only entries to fetch and inject into the pool.
    ///
    /// After filtering by ppr_inclusion_threshold, candidate entries are sorted
    /// by PPR score descending and the top ppr_max_expand are fetched sequentially.
    ///
    /// Default: 50. Valid range: [1, 500] inclusive.
    #[serde(default = "default_ppr_max_expand")]
    pub ppr_max_expand: usize,
```

---

## New `fn default_*()` Functions

Add in the existing `// default value functions` section, after `default_query_log_lookback_days`:

```
// ---------------------------------------------------------------------------
// Personalized PageRank default value functions (crt-030)
// ---------------------------------------------------------------------------

fn default_ppr_alpha() -> f64 {
    0.85
}

fn default_ppr_iterations() -> usize {
    20
}

fn default_ppr_inclusion_threshold() -> f64 {
    0.05
}

fn default_ppr_blend_weight() -> f64 {
    0.15
}

fn default_ppr_max_expand() -> usize {
    50
}
```

---

## `Default::default()` Addition

Inside `impl Default for InferenceConfig`, in the `InferenceConfig { ... }` block,
after the `query_log_lookback_days` line:

```
            // crt-030: Personalized PageRank fields
            ppr_alpha: default_ppr_alpha(),
            ppr_iterations: default_ppr_iterations(),
            ppr_inclusion_threshold: default_ppr_inclusion_threshold(),
            ppr_blend_weight: default_ppr_blend_weight(),
            ppr_max_expand: default_ppr_max_expand(),
```

---

## `validate()` Additions

Inside `impl InferenceConfig { fn validate(&self, path: &Path) -> Result<(), ConfigError> }`,
after the existing crt-029 and col-031 validation checks:

```
        // -- PPR f64 range checks (crt-030) --

        // ppr_alpha: (0.0, 1.0) exclusive
        if self.ppr_alpha <= 0.0 || self.ppr_alpha >= 1.0 {
            return Err(ConfigError::NliFieldOutOfRange {
                path: path.to_path_buf(),
                field: "ppr_alpha",
                value: self.ppr_alpha.to_string(),
                reason: "must be in range (0.0, 1.0) exclusive",
            });
        }

        // ppr_iterations: [1, 100] inclusive
        if self.ppr_iterations < 1 || self.ppr_iterations > 100 {
            return Err(ConfigError::NliFieldOutOfRange {
                path: path.to_path_buf(),
                field: "ppr_iterations",
                value: self.ppr_iterations.to_string(),
                reason: "must be in range [1, 100] inclusive",
            });
        }

        // ppr_inclusion_threshold: (0.0, 1.0) exclusive
        if self.ppr_inclusion_threshold <= 0.0 || self.ppr_inclusion_threshold >= 1.0 {
            return Err(ConfigError::NliFieldOutOfRange {
                path: path.to_path_buf(),
                field: "ppr_inclusion_threshold",
                value: self.ppr_inclusion_threshold.to_string(),
                reason: "must be in range (0.0, 1.0) exclusive",
            });
        }

        // ppr_blend_weight: [0.0, 1.0] inclusive
        if self.ppr_blend_weight < 0.0 || self.ppr_blend_weight > 1.0 {
            return Err(ConfigError::NliFieldOutOfRange {
                path: path.to_path_buf(),
                field: "ppr_blend_weight",
                value: self.ppr_blend_weight.to_string(),
                reason: "must be in range [0.0, 1.0] inclusive",
            });
        }

        // ppr_max_expand: [1, 500] inclusive
        if self.ppr_max_expand < 1 || self.ppr_max_expand > 500 {
            return Err(ConfigError::NliFieldOutOfRange {
                path: path.to_path_buf(),
                field: "ppr_max_expand",
                value: self.ppr_max_expand.to_string(),
                reason: "must be in range [1, 500] inclusive",
            });
        }
```

---

## Global+Project Config Merge Block Addition

Inside `merge_configs()` (the function containing the `inference: InferenceConfig { ... }` block),
after the `query_log_lookback_days` merge entry:

```
            // crt-030: PPR fields
            ppr_alpha: if (project.inference.ppr_alpha - default.inference.ppr_alpha).abs()
                > f64::EPSILON
            {
                project.inference.ppr_alpha
            } else {
                global.inference.ppr_alpha
            },
            ppr_iterations: if project.inference.ppr_iterations != default.inference.ppr_iterations {
                project.inference.ppr_iterations
            } else {
                global.inference.ppr_iterations
            },
            ppr_inclusion_threshold: if (project.inference.ppr_inclusion_threshold
                - default.inference.ppr_inclusion_threshold)
                .abs()
                > f64::EPSILON
            {
                project.inference.ppr_inclusion_threshold
            } else {
                global.inference.ppr_inclusion_threshold
            },
            ppr_blend_weight: if (project.inference.ppr_blend_weight
                - default.inference.ppr_blend_weight)
                .abs()
                > f64::EPSILON
            {
                project.inference.ppr_blend_weight
            } else {
                global.inference.ppr_blend_weight
            },
            ppr_max_expand: if project.inference.ppr_max_expand != default.inference.ppr_max_expand {
                project.inference.ppr_max_expand
            } else {
                global.inference.ppr_max_expand
            },
```

Pattern note: f64 comparisons use `abs() > f64::EPSILON` (same as `w_sim`, `w_nli`, etc.).
usize comparisons use direct `!=` (same as `nli_top_k`, `max_contradicts_per_tick`, etc.).

---

## Error Handling

All five validation errors return `ConfigError::NliFieldOutOfRange` — the existing
config error variant for out-of-range InferenceConfig fields. Field name strings must
match the Rust field name exactly (used in test assertions and error messages).

No new error variants are introduced.

---

## Key Test Scenarios

### T-CFG-PPR-01: Default InferenceConfig passes validate() (regression guard)
```
let config = InferenceConfig::default()
ASSERT config.validate(path).is_ok()
ASSERT config.ppr_alpha == 0.85
ASSERT config.ppr_iterations == 20
ASSERT config.ppr_inclusion_threshold == 0.05
ASSERT config.ppr_blend_weight == 0.15
ASSERT config.ppr_max_expand == 50
```

### T-CFG-PPR-02: ppr_alpha boundaries
```
// Fails at 0.0 (exclusive lower)
InferenceConfig { ppr_alpha: 0.0, ..Default::default() } → validate() fails, field = "ppr_alpha"
// Fails at 1.0 (exclusive upper)
InferenceConfig { ppr_alpha: 1.0, ..Default::default() } → validate() fails, field = "ppr_alpha"
// Passes at 0.5
InferenceConfig { ppr_alpha: 0.5, ..Default::default() } → validate() passes (E-06)
```

### T-CFG-PPR-03: ppr_iterations boundaries
```
InferenceConfig { ppr_iterations: 0, ..Default::default() }   → validate() fails
InferenceConfig { ppr_iterations: 101, ..Default::default() } → validate() fails
InferenceConfig { ppr_iterations: 1, ..Default::default() }   → validate() passes
InferenceConfig { ppr_iterations: 100, ..Default::default() } → validate() passes
```

### T-CFG-PPR-04: ppr_inclusion_threshold boundaries
```
InferenceConfig { ppr_inclusion_threshold: 0.0, ..Default::default() } → validate() fails
InferenceConfig { ppr_inclusion_threshold: 1.0, ..Default::default() } → validate() fails
InferenceConfig { ppr_inclusion_threshold: 0.5, ..Default::default() } → validate() passes
```

### T-CFG-PPR-05: ppr_blend_weight boundaries (inclusive 0.0 and 1.0 both valid)
```
InferenceConfig { ppr_blend_weight: 0.0, ..Default::default() }   → validate() passes (R-03/AC-14)
InferenceConfig { ppr_blend_weight: 1.0, ..Default::default() }   → validate() passes (R-11)
InferenceConfig { ppr_blend_weight: -0.01, ..Default::default() } → validate() fails
InferenceConfig { ppr_blend_weight: 1.01, ..Default::default() }  → validate() fails
```

### T-CFG-PPR-06: ppr_max_expand boundaries
```
InferenceConfig { ppr_max_expand: 0, ..Default::default() }   → validate() fails
InferenceConfig { ppr_max_expand: 501, ..Default::default() } → validate() fails
InferenceConfig { ppr_max_expand: 1, ..Default::default() }   → validate() passes
InferenceConfig { ppr_max_expand: 500, ..Default::default() } → validate() passes
```

### T-CFG-PPR-07: TOML deserialization — absent PPR fields use defaults
```
toml_str = ""   // no [inference] section at all
let config: InferenceConfig = toml::from_str(toml_str).unwrap()
ASSERT config.ppr_alpha == 0.85
ASSERT config.ppr_iterations == 20
ASSERT config.ppr_inclusion_threshold == 0.05
ASSERT config.ppr_blend_weight == 0.15
ASSERT config.ppr_max_expand == 50
```

### T-CFG-PPR-08: Merge — project override takes precedence over global
```
project has ppr_alpha = 0.9
global has ppr_alpha = 0.8 (differs from default 0.85)
merged = merge_configs(global, project)
ASSERT merged.inference.ppr_alpha == 0.9   // project override wins
```
