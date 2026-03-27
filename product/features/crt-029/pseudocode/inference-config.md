# Component: inference-config

## Purpose

Extend `InferenceConfig` in `crates/unimatrix-server/src/infra/config.rs` with four new fields
that configure the background graph inference tick path. Also covers: the three `pub(crate)`
promotions in `nli_detection.rs` and the `pub mod` declaration in `services/mod.rs`.

---

## Files Modified

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/infra/config.rs` | Add four fields, four default fns, four validate guards, update `Default` impl, update merge function |
| `crates/unimatrix-server/src/services/nli_detection.rs` | Promote three private fns to `pub(crate)` |
| `crates/unimatrix-server/src/services/mod.rs` | Add `pub mod nli_detection_tick;` |

---

## 1. Four New `InferenceConfig` Fields

### Location

Append after the existing `w_phase_explicit: f64` field (the last field in the struct before
the closing brace), within the `InferenceConfig` struct definition.

### Struct Field Additions

```
// -----------------------------------------------------------------------
// Background graph inference tick fields (crt-029)
// -----------------------------------------------------------------------

/// HNSW similarity floor for candidate pair pre-filter.
///
/// Pairs with similarity <= supports_candidate_threshold are excluded before NLI scoring.
/// Must be strictly less than supports_edge_threshold (enforced by validate()).
/// Default: 0.5. Range: (0.0, 1.0) exclusive.
///
/// Independent of nli_entailment_threshold (post-store path).
#[serde(default = "default_supports_candidate_threshold")]
pub supports_candidate_threshold: f32,

/// NLI entailment floor for writing Supports edges from the background tick.
///
/// Pairs with scores.entailment > supports_edge_threshold receive a Supports edge.
/// Must be strictly greater than supports_candidate_threshold (enforced by validate()).
/// Default: 0.7 (intentionally higher than nli_entailment_threshold = 0.6 because
/// the tick covers a larger pair space than the post-store path; C-06).
/// Range: (0.0, 1.0) exclusive.
///
/// Independent of nli_entailment_threshold (post-store path).
#[serde(default = "default_supports_edge_threshold")]
pub supports_edge_threshold: f32,

/// Maximum number of candidate pairs scored per tick.
///
/// Acts as the sole throttle on tick NLI budget. Also used as the source-candidate
/// cap: select_source_candidates returns at most max_graph_inference_per_tick source IDs
/// (ADR-003 — no separate max_source_candidates_per_tick field).
/// Default: 100. Range: [1, 1000].
#[serde(default = "default_max_graph_inference_per_tick")]
pub max_graph_inference_per_tick: usize,

/// HNSW neighbour count for tick path HNSW expansion.
///
/// Independent of nli_post_store_k (tick is background, not latency-sensitive).
/// Default: 10. Range: [1, 100].
#[serde(default = "default_graph_inference_k")]
pub graph_inference_k: usize,
```

---

## 2. Default Functions (module-level, near existing default fns)

Append alongside the existing `default_nli_post_store_k`, `default_max_contradicts_per_tick`
functions:

```
fn default_supports_candidate_threshold() -> f32 {
    0.5
}

fn default_supports_edge_threshold() -> f32 {
    0.7
}

fn default_max_graph_inference_per_tick() -> usize {
    100
}

fn default_graph_inference_k() -> usize {
    10
}
```

---

## 3. `Default` Impl Update

The `Default` impl for `InferenceConfig` uses a struct literal (SR-07 — all four fields must
appear explicitly to keep the literal exhaustive).

Find the struct literal inside `impl Default for InferenceConfig` and append the four fields:

```
// Inside InferenceConfig::default() struct literal, after w_phase_explicit:
supports_candidate_threshold: 0.5,
supports_edge_threshold: 0.7,
max_graph_inference_per_tick: 100,
graph_inference_k: 10,
```

---

## 4. `validate()` Extensions

Append four new guard blocks inside `InferenceConfig::validate()`, after the existing
`nli_auto_quarantine_threshold > nli_contradiction_threshold` cross-field guard.

### 4a. `supports_candidate_threshold` range check

```
PRECONDITION: self is InferenceConfig, path is &Path

if self.supports_candidate_threshold <= 0.0 OR self.supports_candidate_threshold >= 1.0 {
    return Err(ConfigError::NliFieldOutOfRange {
        path: path.to_path_buf(),
        field: "supports_candidate_threshold",
        value: self.supports_candidate_threshold.to_string(),
        reason: "must be in range (0.0, 1.0) exclusive",
    })
}
```

### 4b. `supports_edge_threshold` range check

```
if self.supports_edge_threshold <= 0.0 OR self.supports_edge_threshold >= 1.0 {
    return Err(ConfigError::NliFieldOutOfRange {
        path: path.to_path_buf(),
        field: "supports_edge_threshold",
        value: self.supports_edge_threshold.to_string(),
        reason: "must be in range (0.0, 1.0) exclusive",
    })
}
```

### 4c. Cross-field invariant: candidate_threshold < edge_threshold

Pattern follows the existing `nli_auto_quarantine_threshold > nli_contradiction_threshold`
guard (line ~591 of config.rs). Reject predicate uses `>=` (equal values rejected — AC-02):

```
if self.supports_candidate_threshold >= self.supports_edge_threshold {
    return Err(ConfigError::NliThresholdInvariantViolated {
        path: path.to_path_buf(),
        auto_quarantine: self.supports_edge_threshold,       // repurpose field name OR
        contradiction: self.supports_candidate_threshold,    // use new error variant
    })
}
```

IMPLEMENTATION NOTE: The existing `NliThresholdInvariantViolated` error variant uses field
names `auto_quarantine` and `contradiction`. For crt-029, the implementation agent has two
options:

Option A — Reuse the existing variant and repurpose the field names (simpler, no new error
variant):
```
NliThresholdInvariantViolated {
    auto_quarantine: self.supports_edge_threshold,
    contradiction: self.supports_candidate_threshold,
}
```
Error message: "nli_auto_quarantine_threshold (X) must be strictly greater than
nli_contradiction_threshold (Y)" — field names wrong but values correct. Acceptable for
internal validation errors.

Option B — Add a new error variant `GraphInferenceThresholdInvariantViolated` with better
field names. Preferred for clarity but requires adding a variant to `ConfigError`. Since
no new crate dependencies are added, this is a small enum addition.

The implementation agent should pick Option B if the `ConfigError` enum is straightforward
to extend; otherwise Option A is acceptable.

### 4d. `max_graph_inference_per_tick` range check

```
if self.max_graph_inference_per_tick < 1 OR self.max_graph_inference_per_tick > 1000 {
    return Err(ConfigError::NliFieldOutOfRange {
        path: path.to_path_buf(),
        field: "max_graph_inference_per_tick",
        value: self.max_graph_inference_per_tick.to_string(),
        reason: "must be in range [1, 1000]",
    })
}
```

### 4e. `graph_inference_k` range check

```
if self.graph_inference_k < 1 OR self.graph_inference_k > 100 {
    return Err(ConfigError::NliFieldOutOfRange {
        path: path.to_path_buf(),
        field: "graph_inference_k",
        value: self.graph_inference_k.to_string(),
        reason: "must be in range [1, 100]",
    })
}
```

---

## 5. Config Merge Function Update

There is a per-project/global merge function that resolves `InferenceConfig` fields using
"if project differs from default, use project; else use global" semantics for each field.

For `usize` fields (follow the `nli_post_store_k` pattern — inequality comparison):

```
supports_candidate_threshold: if (project.inference.supports_candidate_threshold
    - default.inference.supports_candidate_threshold).abs() > f32::EPSILON
{
    project.inference.supports_candidate_threshold
} else {
    global.inference.supports_candidate_threshold
},

supports_edge_threshold: if (project.inference.supports_edge_threshold
    - default.inference.supports_edge_threshold).abs() > f32::EPSILON
{
    project.inference.supports_edge_threshold
} else {
    global.inference.supports_edge_threshold
},

max_graph_inference_per_tick: if project.inference.max_graph_inference_per_tick
    != default.inference.max_graph_inference_per_tick
{
    project.inference.max_graph_inference_per_tick
} else {
    global.inference.max_graph_inference_per_tick
},

graph_inference_k: if project.inference.graph_inference_k
    != default.inference.graph_inference_k
{
    project.inference.graph_inference_k
} else {
    global.inference.graph_inference_k
},
```

---

## 6. `pub(crate)` Promotions in `nli_detection.rs`

Three functions currently `fn` (private) must become `pub(crate) fn`:

```
// Before (private):
async fn write_nli_edge(store, source_id, target_id, relation_type, weight, created_at, metadata) -> bool

fn format_nli_metadata(scores: &NliScores) -> String

fn current_timestamp_secs() -> u64

// After (pub(crate)):
pub(crate) async fn write_nli_edge(store: &Store, source_id: u64, target_id: u64, relation_type: &str, weight: f32, created_at: u64, metadata: &str) -> bool

pub(crate) fn format_nli_metadata(scores: &NliScores) -> String

pub(crate) fn current_timestamp_secs() -> u64
```

No other changes to `nli_detection.rs`. The function bodies are unchanged.

---

## 7. Module Declaration in `services/mod.rs`

Append one line alongside the existing `pub(crate) mod nli_detection;` line:

```
pub mod nli_detection_tick;
```

Note: visibility is `pub mod` (not `pub(crate) mod`) because `run_graph_inference_tick` is
called from `crate::background` which is at the crate root, not within `services`. If the
module only needs to export `run_graph_inference_tick` and the call site uses
`use crate::services::nli_detection_tick::run_graph_inference_tick`, `pub(crate) mod`
would also work. Use `pub(crate) mod` to match the existing siblings unless the linker
requires `pub mod`.

IMPLEMENTATION NOTE: Check the existing `background.rs` import pattern for
`maybe_run_bootstrap_promotion` to determine whether `pub` or `pub(crate)` is needed. Based
on the grep, `background.rs` uses:
```rust
use crate::services::nli_detection::maybe_run_bootstrap_promotion;
```
This works with `pub(crate) mod`. Use `pub(crate) mod nli_detection_tick;` to match siblings.

---

## Error Handling

- `validate()` returns `Result<(), ConfigError>` — all new guards return `Err(ConfigError::...)`
  on violation. No panics in validation.
- The merge function is infallible — it returns the resolved `InferenceConfig` value directly.

---

## Key Test Scenarios

### AC-01: Default values
```
let config = InferenceConfig::default();
assert_eq!(config.supports_candidate_threshold, 0.5);
assert_eq!(config.supports_edge_threshold, 0.7);
assert_eq!(config.max_graph_inference_per_tick, 100);
assert_eq!(config.graph_inference_k, 10);
```

### AC-17: TOML deserialization with absent fields uses defaults
```
let config: InferenceConfig = toml::from_str("").unwrap();
assert_eq!(config.supports_candidate_threshold, 0.5);
// ... etc
```

### AC-02: Cross-field invariant — equal values rejected
```
let c = InferenceConfig {
    supports_candidate_threshold: 0.7,
    supports_edge_threshold: 0.7,
    ..InferenceConfig::default()
};
assert!(c.validate(dummy_path).is_err());
```

### AC-02 boundary: strict inequality
```
let c_ok = InferenceConfig {
    supports_candidate_threshold: 0.69,
    supports_edge_threshold: 0.7,
    ..InferenceConfig::default()
};
assert!(c_ok.validate(dummy_path).is_ok());

let c_fail = InferenceConfig {
    supports_candidate_threshold: 0.71,
    supports_edge_threshold: 0.7,
    ..InferenceConfig::default()
};
assert!(c_fail.validate(dummy_path).is_err());
```

### AC-03: Out-of-range boundary values
```
// 0.0 and 1.0 are both invalid (exclusive range)
for val in [0.0f32, 1.0f32] {
    let c = InferenceConfig { supports_candidate_threshold: val, ..InferenceConfig::default() };
    assert!(c.validate(dummy_path).is_err());
    let c = InferenceConfig { supports_edge_threshold: val, ..InferenceConfig::default() };
    assert!(c.validate(dummy_path).is_err());
}
```

### AC-04: `max_graph_inference_per_tick` bounds
```
let c_zero = InferenceConfig { max_graph_inference_per_tick: 0, ..InferenceConfig::default() };
assert!(c_zero.validate(dummy_path).is_err());
let c_max = InferenceConfig { max_graph_inference_per_tick: 1001, ..InferenceConfig::default() };
assert!(c_max.validate(dummy_path).is_err());
let c_ok = InferenceConfig { max_graph_inference_per_tick: 1000, ..InferenceConfig::default() };
assert!(c_ok.validate(dummy_path).is_ok());
```

### AC-04b: `graph_inference_k` bounds
```
let c = InferenceConfig { graph_inference_k: 0, ..InferenceConfig::default() };
assert!(c.validate(dummy_path).is_err());
let c = InferenceConfig { graph_inference_k: 101, ..InferenceConfig::default() };
assert!(c.validate(dummy_path).is_err());
```

### C-11 pre-merge check
```bash
grep -rn 'InferenceConfig {' crates/unimatrix-server/src/
# Every occurrence must include the four new fields or ..InferenceConfig::default() tail.
# Current count: 52 occurrences.
```
