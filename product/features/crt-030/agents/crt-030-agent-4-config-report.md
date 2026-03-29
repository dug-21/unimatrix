# Agent Report: crt-030-agent-4-config

## Task

Add five PPR fields to `InferenceConfig` in `crates/unimatrix-server/src/infra/config.rs`.

## Files Modified

- `crates/unimatrix-server/src/infra/config.rs`

## Changes Made

### Struct fields added (section `// Personalized PageRank fields (crt-030)`)

Five new fields appended to `InferenceConfig` after the `query_log_lookback_days` field:
- `ppr_alpha: f64` — default 0.85, range (0.0, 1.0) exclusive
- `ppr_iterations: usize` — default 20, range [1, 100] inclusive
- `ppr_inclusion_threshold: f64` — default 0.05, range (0.0, 1.0) exclusive
- `ppr_blend_weight: f64` — default 0.15, range [0.0, 1.0] inclusive
- `ppr_max_expand: usize` — default 50, range [1, 500] inclusive

Each field has `#[serde(default = "default_ppr_X")]` and a doc-comment per the pseudocode spec.

### Default functions added

Five `fn default_ppr_*()` functions in the `// Personalized PageRank default value functions (crt-030)` section, inserted after `default_query_log_lookback_days`.

### `Default::default()` impl updated

All five fields initialized via their default functions, after the `query_log_lookback_days` line.

### `validate()` updated

Five range checks added after the `query_log_lookback_days` check, all using `ConfigError::NliFieldOutOfRange` (existing variant — no new error variants needed).

### `merge_configs()` updated

Five merge entries added in the `inference: InferenceConfig { ... }` block after `query_log_lookback_days`. f64 fields use `abs() > f64::EPSILON` comparison; usize fields use direct `!=` (consistent with existing patterns).

### Tests added (30 new tests)

| Test | Coverage |
|------|----------|
| `test_inference_config_ppr_defaults` | AC-09: default values |
| `test_inference_config_ppr_serde_round_trip` | AC-09: explicit TOML → deserialize |
| `test_inference_config_ppr_serde_absent_fields_use_defaults` | zero-downtime upgrade path |
| `test_inference_config_ppr_serde_explicit_override` | AC-09: non-default TOML values |
| `test_ppr_alpha_zero_rejected` | AC-10: exclusive lower bound |
| `test_ppr_alpha_one_rejected` | AC-10: exclusive upper bound |
| `test_ppr_alpha_valid_boundary_low` | f64::EPSILON passes |
| `test_ppr_alpha_valid_boundary_high` | 1.0 - f64::EPSILON passes |
| `test_ppr_alpha_typical_value` | default 0.85 passes |
| `test_ppr_iterations_zero_rejected` | AC-10: below floor |
| `test_ppr_iterations_101_rejected` | AC-10: above ceiling |
| `test_ppr_iterations_valid_min` | inclusive min = 1 |
| `test_ppr_iterations_valid_max` | inclusive max = 100 |
| `test_ppr_iterations_default_valid` | default 20 passes |
| `test_ppr_inclusion_threshold_zero_rejected` | R-06: exclusive lower bound |
| `test_ppr_inclusion_threshold_one_rejected` | exclusive upper bound |
| `test_ppr_inclusion_threshold_valid_boundary_low` | f64::EPSILON passes |
| `test_ppr_inclusion_threshold_default_valid` | default 0.05 passes |
| `test_ppr_blend_weight_negative_rejected` | below 0.0 |
| `test_ppr_blend_weight_above_one_rejected` | above 1.0 |
| `test_ppr_blend_weight_zero_valid` | R-03: inclusive lower bound = 0.0 |
| `test_ppr_blend_weight_one_valid` | R-11: inclusive upper bound = 1.0 |
| `test_ppr_blend_weight_default_valid` | default 0.15 passes |
| `test_ppr_max_expand_zero_rejected` | below floor |
| `test_ppr_max_expand_501_rejected` | above ceiling |
| `test_ppr_max_expand_valid_min` | inclusive min = 1 |
| `test_ppr_max_expand_valid_max` | inclusive max = 500 |
| `test_ppr_max_expand_default_valid` | default 50 passes |
| `test_ppr_validation_error_names_field` | AC-10: error names specific field |
| `test_ppr_fields_merged_from_project_config` | merge: project override wins |

## Test Results

```
cargo test -p unimatrix-server config
test result: ok. 227 passed; 0 failed
```

All 30 new PPR tests pass. Full `unimatrix-server` lib suite: **2303 passed, 0 failed**.

Workspace failures: 5 tests in `unimatrix-engine/src/graph_ppr.rs` — these are pre-existing failures from another agent's in-progress implementation, not caused by config changes.

## Call Sites

`SearchService::new` does not yet accept PPR fields — it passes individual config fields by name. PPR field wiring into `SearchService` is the responsibility of the `search_step_6d` agent. No call sites needed updating for this component.

## Issues / Blockers

None. The pseudocode matched the codebase patterns exactly. All validation tests confirmed correct boundary semantics (exclusive vs inclusive per spec).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entries #3662 (InferenceConfig TOML test pattern), #2730 (struct literal extension), and crt-030 ADRs. Applied entry #3662 to avoid the section-header trap in TOML tests.
- Stored: entry #3743 "InferenceConfig TOML tests must use flat top-level fields — no [section] header" via superseding #3662. Extended with the `Deserialize`-only constraint: `InferenceConfig` does not derive `Serialize`, so `toml::to_string` fails at compile time. Round-trip tests must use explicit TOML strings.
