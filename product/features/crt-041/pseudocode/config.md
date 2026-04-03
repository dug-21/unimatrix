# Pseudocode: config

## Purpose

Add five new fields to `InferenceConfig` in `crates/unimatrix-server/src/infra/config.rs`.
The dual-maintenance invariant (ADR-005) requires identical default values at BOTH:
1. The `#[serde(default = "default_fn")]` backing function (TOML-absent path)
2. The `impl Default for InferenceConfig` struct literal (programmatic construction path)

Divergence between these two sites is a historical bug class (entry #3817, crt-032).

## File Modified

`crates/unimatrix-server/src/infra/config.rs`

## Four Modification Sites (all must be updated atomically)

### Site 1: Struct field declarations (in `pub struct InferenceConfig`)

Insert after the `max_co_access_promotion_per_tick` field block (currently ~line 437).
Use the comment prefix `// crt-041: graph enrichment tick fields` to mark the block.

```
// crt-041: graph enrichment tick fields

/// S2 vocabulary — domain terms for structural vocabulary matching.
/// Default: empty (S2 is a no-op by default). Operator opt-in.
/// Recommended software-engineering starting point (from ASS-038):
///   ["migration", "schema", "performance", "async", "authentication",
///    "cache", "api", "confidence", "graph"]
/// To enable S2, add terms to [inference] s2_vocabulary = [...] in config.toml.
#[serde(default = "default_s2_vocabulary")]
pub s2_vocabulary: Vec<String>,

/// S1 per-tick edge write cap. Default: 200. Range: [1, 10000].
/// Applies to the top-N pairs ordered by shared tag count DESC.
#[serde(default = "default_max_s1_edges_per_tick")]
pub max_s1_edges_per_tick: usize,

/// S2 per-tick edge write cap. Default: 200. Range: [1, 10000].
/// No-op when s2_vocabulary is empty.
#[serde(default = "default_max_s2_edges_per_tick")]
pub max_s2_edges_per_tick: usize,

/// S8 batch frequency: run every N ticks. Default: 10. Range: [1, 1000].
/// At default tick interval (~15 min), N=10 means once per ~150 minutes.
/// Zero is forbidden — causes `tick % 0` integer division panic.
#[serde(default = "default_s8_batch_interval_ticks")]
pub s8_batch_interval_ticks: u32,

/// S8 per-batch pair cap. Default: 500. Range: [1, 10000].
/// Cap applies to pairs expanded from audit_log rows, NOT to row count.
/// Partial-row: watermark advances only to last fully-processed row's event_id.
#[serde(default = "default_max_s8_pairs_per_batch")]
pub max_s8_pairs_per_batch: usize,
```

### Site 2: `impl Default for InferenceConfig` struct literal

The `impl Default` block ends with `supports_cosine_threshold: default_supports_cosine_threshold()`
(currently ~line 637). Extend it with the crt-041 fields before the closing `}`:

```
// crt-041: graph enrichment tick fields
s2_vocabulary: vec![],
max_s1_edges_per_tick: 200,
max_s2_edges_per_tick: 200,
s8_batch_interval_ticks: 10,
max_s8_pairs_per_batch: 500,
```

Values must be identical to what the `default_*()` backing functions return.

### Site 3: `default_*()` backing functions

Insert in the backing-functions section (currently ~line 760+), following the established pattern
of one function per field. Place them after `default_max_co_access_promotion_per_tick`:

```
fn default_s2_vocabulary() -> Vec<String> {
    vec![]
}

fn default_max_s1_edges_per_tick() -> usize {
    200
}

fn default_max_s2_edges_per_tick() -> usize {
    200
}

fn default_s8_batch_interval_ticks() -> u32 {
    10
}

fn default_max_s8_pairs_per_batch() -> usize {
    500
}
```

### Site 4: `validate()` method

The `validate()` method has range checks for all bounded numeric fields. Add five new checks
after the existing `max_co_access_promotion_per_tick` check block (~line 964).

```
// -- crt-041: S1/S2/S8 graph enrichment field range checks --
// Lower bound is 1, not 0: zero causes LIMIT 0 (silent disable) or % 0 (panic).

if self.max_s1_edges_per_tick < 1 || self.max_s1_edges_per_tick > 10000 {
    return Err(ConfigError::OutOfRange {
        field: "max_s1_edges_per_tick",
        value: self.max_s1_edges_per_tick.to_string(),
        allowed: "[1, 10000]",
    });
}

if self.max_s2_edges_per_tick < 1 || self.max_s2_edges_per_tick > 10000 {
    return Err(ConfigError::OutOfRange {
        field: "max_s2_edges_per_tick",
        value: self.max_s2_edges_per_tick.to_string(),
        allowed: "[1, 10000]",
    });
}

if self.s8_batch_interval_ticks < 1 || self.s8_batch_interval_ticks > 1000 {
    return Err(ConfigError::OutOfRange {
        field: "s8_batch_interval_ticks",
        value: self.s8_batch_interval_ticks.to_string(),
        allowed: "[1, 1000]",
    });
}

if self.max_s8_pairs_per_batch < 1 || self.max_s8_pairs_per_batch > 10000 {
    return Err(ConfigError::OutOfRange {
        field: "max_s8_pairs_per_batch",
        value: self.max_s8_pairs_per_batch.to_string(),
        allowed: "[1, 10000]",
    });
}
// s2_vocabulary has no range check: empty vec is valid (S2 becomes a no-op)
```

Note: Delivery agent must check the exact `ConfigError` variant name and field names
used in the existing `validate()` method to match the pattern precisely.

### Site 5: `merge_configs()` function

The `merge_configs` function uses the project-overrides-global pattern
`if project.field != default.field { project.field } else { global.field }`.
Add five entries following the `max_co_access_promotion_per_tick` block (~line 2354):

```
// crt-041: graph enrichment tick fields
s2_vocabulary: if project.inference.s2_vocabulary != default.inference.s2_vocabulary {
    project.inference.s2_vocabulary
} else {
    global.inference.s2_vocabulary
},
max_s1_edges_per_tick: if project.inference.max_s1_edges_per_tick
    != default.inference.max_s1_edges_per_tick
{
    project.inference.max_s1_edges_per_tick
} else {
    global.inference.max_s1_edges_per_tick
},
max_s2_edges_per_tick: if project.inference.max_s2_edges_per_tick
    != default.inference.max_s2_edges_per_tick
{
    project.inference.max_s2_edges_per_tick
} else {
    global.inference.max_s2_edges_per_tick
},
s8_batch_interval_ticks: if project.inference.s8_batch_interval_ticks
    != default.inference.s8_batch_interval_ticks
{
    project.inference.s8_batch_interval_ticks
} else {
    global.inference.s8_batch_interval_ticks
},
max_s8_pairs_per_batch: if project.inference.max_s8_pairs_per_batch
    != default.inference.max_s8_pairs_per_batch
{
    project.inference.max_s8_pairs_per_batch
} else {
    global.inference.max_s8_pairs_per_batch
},
```

## Error Handling

`validate()` returns `Err(ConfigError)` for out-of-range values. The server startup path
calls `validate()` and refuses to start if any field is out of range. This is the hard
guard preventing `% 0` or `LIMIT 0` at runtime.

## Key Test Scenarios

### T-CFG-01: serde-match test (R-03, ADR-005) — MANDATORY pre-PR
This test must be present in the `config.rs::tests` module before the PR is opened:

```
fn test_inference_config_s1_s2_s8_defaults_match_serde() {
    let default_config = InferenceConfig::default();
    let serde_config: InferenceConfig = toml::from_str("").unwrap();

    assert_eq!(
        default_config.s2_vocabulary, serde_config.s2_vocabulary,
        "s2_vocabulary: impl Default and serde default must agree"
    );
    assert_eq!(
        default_config.max_s1_edges_per_tick, serde_config.max_s1_edges_per_tick,
        "max_s1_edges_per_tick: impl Default and serde default must agree"
    );
    assert_eq!(
        default_config.max_s2_edges_per_tick, serde_config.max_s2_edges_per_tick,
        "max_s2_edges_per_tick: impl Default and serde default must agree"
    );
    assert_eq!(
        default_config.s8_batch_interval_ticks, serde_config.s8_batch_interval_ticks,
        "s8_batch_interval_ticks: impl Default and serde default must agree"
    );
    assert_eq!(
        default_config.max_s8_pairs_per_batch, serde_config.max_s8_pairs_per_batch,
        "max_s8_pairs_per_batch: impl Default and serde default must agree"
    );

    // Verify empty vocabulary is the default, NOT the 9-term ASS-038 list
    assert!(
        default_config.s2_vocabulary.is_empty(),
        "s2_vocabulary default must be empty vec (operator opt-in, W0-3)"
    );
}
```

### T-CFG-02: validate() zero-value rejection (R-17, C-08, AC-24)
For each of the four numeric fields, construct a config with value=0 and assert validate() errors:

```
// Example for max_s1_edges_per_tick:
let config = InferenceConfig { max_s1_edges_per_tick: 0, ..InferenceConfig::default() };
let result = config.validate();
assert!(result.is_err());
assert!(result.unwrap_err().to_string().contains("max_s1_edges_per_tick"));
```
Repeat for `max_s2_edges_per_tick`, `s8_batch_interval_ticks`, `max_s8_pairs_per_batch`.

### T-CFG-03: validate() over-limit rejection
For each numeric field, value = upper bound + 1 must fail.

### T-CFG-04: validate() boundary values pass
For each field: lower bound (1) and upper bound (10000 or 1000) must pass.

### T-CFG-05: merge_configs prefers project override
Set project field to non-default value; global to different value; assert merged uses project.

### T-CFG-06: merge_configs falls back to global when project is default
Set project field to default value; global to non-default; assert merged uses global.
