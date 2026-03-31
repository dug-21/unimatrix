# Agent Report: crt-036-agent-3-retention-config

## Task

Implement the `RetentionConfig` component for crt-036.

## Files Modified

- `crates/unimatrix-server/src/infra/config.rs` — added `RetentionConfig` struct, three default fns, `Default` impl, `validate()` impl, `RetentionFieldOutOfRange` `ConfigError` variant with `Display` arm, wired into `UnimatrixConfig` (`pub retention: RetentionConfig`), added `retention.validate(path)?` call in `validate_config()`, added `retention` field to `merge_configs()`, added 5 test functions covering AC-10/AC-11/AC-12/AC-12b
- `config.toml` — created with `[retention]` block documenting all three fields with ranges and defaults

## Tests

- 6 RetentionConfig-specific tests added and passing
- Full unimatrix-server suite: 2619 passed, 0 failed

| Test | AC | Result |
|------|-----|--------|
| `test_retention_config_defaults_and_override` | AC-10 | PASS |
| `test_retention_config_validate_rejects_zero_retention_cycles` | AC-11 | PASS |
| `test_retention_config_validate_rejects_zero_audit_days` | AC-12 | PASS |
| `test_retention_config_validate_rejects_invalid_max_cycles` | AC-12b | PASS |
| `test_retention_config_defaults_pass_validate` | edge case | PASS |
| `test_retention_config_validate_called_by_validate_config` | validate_config wiring | PASS |

## AC-13 Compliance

The `activity_detail_retention_cycles` doc comment contains both "PhaseFreqTable lookback" and "GNN training window" as required.

## Implementation Notes

- Followed `InferenceConfig` pattern exactly: `#[serde(default)]` at struct level, per-field `#[serde(default = "fn")]`, separate default fns, `Default` impl delegates to default fns, `validate()` returns first failure
- `RetentionFieldOutOfRange` Display format mirrors `NliFieldOutOfRange`: `"config error in {path}: [retention] field '{field}' = '{value}' is invalid: {reason}"`
- `merge_configs` uses per-field project-wins semantics matching the pattern used by all other `u32`/`usize` fields in the function
- `PartialEq` added to `RetentionConfig` derive to enable merge logic comparisons against `UnimatrixConfig::default()`

## Issues / Blockers

None. All pre-resolved decisions from IMPLEMENTATION-BRIEF and ARCHITECTURE applied cleanly.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entry #3743 (InferenceConfig flat TOML test pattern) as directly relevant; applied the complementary distinction for nested-section configs
- Stored: entry #3928 "Test new nested config sections via toml::from_str::<UnimatrixConfig> with [section] header, not flat fields" via /uni-store-pattern — captures the contrast with #3743 that future agents adding new nested config structs will need
