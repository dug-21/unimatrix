# crt-024: InferenceConfig Pseudocode

## Purpose

Add six f64 fusion weight fields to `InferenceConfig` in `crates/unimatrix-server/src/infra/config.rs`,
and extend `InferenceConfig::validate()` with per-field range checks and a six-term sum check.

These fields are the operator interface for the ranking signal weights defined in ADR-003. They
supply `SearchService` with the weight values used to construct `FusionWeights`. No other component
is modified by Wave 1.

## File: `crates/unimatrix-server/src/infra/config.rs`

---

## New Fields on `InferenceConfig`

Add six f64 fields to the `InferenceConfig` struct, following the existing NLI field block.
Each uses a named default function (required by serde for `#[serde(default = "fn")`).

```
// Existing struct InferenceConfig { ... } gains:

    // -----------------------------------------------------------------------
    // Ranking signal fusion weights (crt-024, ADR-003)
    // -----------------------------------------------------------------------

    /// Fusion weight for cosine similarity signal (bi-encoder recall). Default: 0.25.
    #[serde(default = "default_w_sim")]
    pub w_sim: f64,

    /// Fusion weight for NLI entailment signal (cross-encoder precision). Default: 0.35.
    /// When NLI is disabled or absent, this term contributes 0.0 and remaining weights
    /// are re-normalized by FusionWeights::effective(nli_available: false).
    #[serde(default = "default_w_nli")]
    pub w_nli: f64,

    /// Fusion weight for confidence signal (Wilson score composite). Default: 0.15.
    #[serde(default = "default_w_conf")]
    pub w_conf: f64,

    /// Fusion weight for co-access affinity signal (normalized usage pattern). Default: 0.10.
    #[serde(default = "default_w_coac")]
    pub w_coac: f64,

    /// Fusion weight for utility delta signal (effectiveness classification). Default: 0.05.
    #[serde(default = "default_w_util")]
    pub w_util: f64,

    /// Fusion weight for provenance signal (boosted-category hint). Default: 0.05.
    /// Defaults sum to 0.95; the remaining 0.05 is headroom for WA-2's phase boost term.
    #[serde(default = "default_w_prov")]
    pub w_prov: f64,
```

---

## Default Value Functions

Add six new default functions alongside the existing NLI default functions:

```
fn default_w_sim()  -> f64 { 0.25 }
fn default_w_nli()  -> f64 { 0.35 }
fn default_w_conf() -> f64 { 0.15 }
fn default_w_coac() -> f64 { 0.10 }
fn default_w_util() -> f64 { 0.05 }
fn default_w_prov() -> f64 { 0.05 }
```

---

## `InferenceConfig::Default` impl

Extend the existing `impl Default for InferenceConfig` to include the six new fields:

```
impl Default for InferenceConfig {
    fn default() -> Self {
        InferenceConfig {
            // ... existing fields unchanged ...
            rayon_pool_size: (num_cpus::get() / 2).max(4).min(8),
            nli_enabled: false,
            nli_model_name: None,
            nli_model_path: None,
            nli_model_sha256: None,
            nli_top_k: 20,
            nli_post_store_k: 10,
            nli_entailment_threshold: 0.6,
            nli_contradiction_threshold: 0.6,
            max_contradicts_per_tick: 10,
            nli_auto_quarantine_threshold: 0.85,
            // new fields:
            w_sim:  0.25,
            w_nli:  0.35,
            w_conf: 0.15,
            w_coac: 0.10,
            w_util: 0.05,
            w_prov: 0.05,
        }
    }
}
```

---

## Extended `InferenceConfig::validate()`

Add two new validation blocks to the existing `validate(&self, path: &Path) -> Result<(), ConfigError>`
method. Insert AFTER the existing cross-field `NliThresholdInvariantViolated` check and BEFORE the
final `Ok(())`.

### Block 1: Per-field range check [0.0, 1.0]

Reuse the existing `NliFieldOutOfRange` error variant (same structured pattern as NLI fields).
Each weight must be in [0.0, 1.0] inclusive — negative weights or weights > 1.0 are rejected.

```
// Per-field fusion weight range checks [0.0, 1.0] inclusive
let fusion_weight_checks: &[(&'static str, f64)] = &[
    ("w_sim",  self.w_sim),
    ("w_nli",  self.w_nli),
    ("w_conf", self.w_conf),
    ("w_coac", self.w_coac),
    ("w_util", self.w_util),
    ("w_prov", self.w_prov),
];

for (field, value) in fusion_weight_checks {
    if *value < 0.0 || *value > 1.0 {
        return Err(ConfigError::NliFieldOutOfRange {
            path: path.to_path_buf(),
            field,
            value: value.to_string(),
            reason: "fusion weight must be in range [0.0, 1.0]",
        });
    }
}
```

### Block 2: Six-term sum check (<= 1.0)

Use the new `FusionWeightSumExceeded` error variant (defined below) for the cross-field invariant:

```
// Sum-of-six constraint: w_sim + w_nli + w_conf + w_coac + w_util + w_prov <= 1.0
let fusion_weight_sum = self.w_sim + self.w_nli + self.w_conf
    + self.w_coac + self.w_util + self.w_prov;

if fusion_weight_sum > 1.0 {
    return Err(ConfigError::FusionWeightSumExceeded {
        path: path.to_path_buf(),
        sum: fusion_weight_sum,
        w_sim:  self.w_sim,
        w_nli:  self.w_nli,
        w_conf: self.w_conf,
        w_coac: self.w_coac,
        w_util: self.w_util,
        w_prov: self.w_prov,
    });
}
```

---

## New `ConfigError` Variant

Add to the `ConfigError` enum:

```
/// Six fusion weight fields sum to > 1.0.
///
/// Reports the computed sum and all six field values so operators can diagnose
/// which weights to reduce (AC-02, FR-03, ADR-003 crt-024).
FusionWeightSumExceeded {
    path: PathBuf,
    sum: f64,
    w_sim:  f64,
    w_nli:  f64,
    w_conf: f64,
    w_coac: f64,
    w_util: f64,
    w_prov: f64,
},
```

Add a `Display` arm in `impl fmt::Display for ConfigError`:

```
ConfigError::FusionWeightSumExceeded {
    path, sum, w_sim, w_nli, w_conf, w_coac, w_util, w_prov,
} => write!(
    f,
    "config error in {}: [inference] fusion weights sum to {:.6} which exceeds 1.0; \
     reduce one or more of: w_sim={w_sim}, w_nli={w_nli}, w_conf={w_conf}, \
     w_coac={w_coac}, w_util={w_util}, w_prov={w_prov}",
    path.display(),
    sum,
),
```

---

## Validation Order in `validate()`

The extended validate() now executes checks in this order (new checks added at the end):
1. `rayon_pool_size` range [1, 64]
2. `nli_top_k` range [1, 100]
3. `nli_post_store_k` range [1, 100]
4. `max_contradicts_per_tick` range [1, 100]
5. `nli_entailment_threshold` range (0.0, 1.0) exclusive
6. `nli_contradiction_threshold` range (0.0, 1.0) exclusive
7. `nli_auto_quarantine_threshold` range (0.0, 1.0) exclusive
8. `nli_model_name` recognized variant when Some
9. `nli_model_sha256` 64-char hex when Some
10. Cross-field: `nli_auto_quarantine_threshold > nli_contradiction_threshold`
11. **NEW**: per-field fusion weight range [0.0, 1.0] (for each of the six weights)
12. **NEW**: six-term fusion weight sum <= 1.0

---

## Error Handling

- All errors return `Err(ConfigError::...)` — no panics, no `unwrap()`
- Validation is fail-fast: the first violation returns immediately; later fields are not checked
- The sum check runs after all per-field checks pass, so `FusionWeightSumExceeded` is only
  reached when all six individual weights are individually valid
- Both new error variants match the existing structured error pattern used by NLI field validation

---

## Key Test Scenarios

### T-IC-01: Default deserialization (AC-01)

Deserialize an `InferenceConfig` from a TOML string with no fusion weight fields. Assert all six
fields equal their defaults: w_sim=0.25, w_nli=0.35, w_conf=0.15, w_coac=0.10, w_util=0.05,
w_prov=0.05. Assert their sum == 0.95 (strictly < 1.0, confirming headroom).

### T-IC-02: Sum validation rejection (AC-02)

Construct `InferenceConfig` with w_sim=0.5, w_nli=0.4, w_conf=0.15, w_coac=0.0, w_util=0.0,
w_prov=0.0 (sum=1.05). Call `validate(path)`. Assert returns `Err(FusionWeightSumExceeded)`.
Assert the Display string contains all six field names and the computed sum.

### T-IC-03: Per-field negative rejection (AC-03)

Six separate tests, one per field. Set that field to -0.01, all others to valid values. Assert
`validate()` returns `Err(NliFieldOutOfRange)` naming the offending field.

### T-IC-04: Per-field over-1.0 rejection (AC-03)

Six separate tests, one per field. Set that field to 1.01, all others to 0.0. Assert
`validate()` returns `Err(NliFieldOutOfRange)` naming the offending field.

### T-IC-05: Sum exactly 1.0 is valid (AC-13, EC-02)

Construct config with w_sim=0.40, w_nli=0.35, w_conf=0.15, w_coac=0.10, w_util=0.0, w_prov=0.0
(sum=1.0). Assert `validate()` returns `Ok(())`. Sum == 1.0 is not an error.

### T-IC-06: Sum in (0.0, 1.0) with headroom is valid

Default config. Assert `validate()` returns `Ok(())`. Sum=0.95.

### T-IC-07: All weights zero is valid (EC-01)

All six fields set to 0.0 (sum=0.0 <= 1.0). Assert `validate()` returns `Ok(())`. This is
degenerate but not a config error — produces `fused_score = 0.0` for all candidates.

### T-IC-08: Existing NLI validation still works

Set `nli_auto_quarantine_threshold = 0.5` and `nli_contradiction_threshold = 0.7` (violates
cross-field invariant). Assert `validate()` returns `Err(NliThresholdInvariantViolated)`. Confirms
new validation blocks do not break existing checks.

### T-IC-09: Default sum invariant (R-06)

In the `Default` impl, assert `w_sim + w_nli + w_conf + w_coac + w_util + w_prov <= 0.95`.
This is the W3-1 initialization guard: a typo in the defaults corrupts W3-1's training baseline.
