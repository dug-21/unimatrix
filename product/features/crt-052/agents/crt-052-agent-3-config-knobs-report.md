# crt-052 Agent 3 — C9 Config Knobs (Wave A)

## Outcome
Added the three Wave A config knobs to `RetentionConfig` in `infra/config.rs` using the
existing `serde(default)` / `validate()` / `merge_configs` pattern (beside
`transcript_buffer_max_bytes`). Wave B hold knobs deliberately NOT added (R-11 boundary
clean — zero reference to `transcript_hold.rs`).

## Files modified
- `/workspaces/unimatrix/crates/unimatrix-server/src/infra/config.rs`

## Knobs added
| Knob | Type | Default | Validation |
|------|------|---------|------------|
| `transcript_candidate_session_cap_bytes` | usize | 24576 (24 KiB) | > 0; <= cycle cap |
| `transcript_candidate_cycle_cap_bytes` | usize | 262144 (256 KiB) | > 0 |
| `transcript_fallback_hole_fraction` | f64 | 0.5 | in [0.0, 1.0] (rejects NaN/Inf) |

Five sites updated atomically: struct field, default fn, `Default` impl literal,
`merge_configs` per-field project-wins arm, `validate()` bounds.

## Tests (in `infra/config.rs` `#[cfg(test)]`)
- `test_config_defaults_candidate_knobs` — serde defaults (absent + partial block) + Default impl
- `test_config_serde_roundtrip_candidate_knobs` — explicit values parse
- `test_config_merge_candidate_knobs_project_wins` — project-wins / global-fallback
- `test_config_validate_rejects_zero_session_cap`
- `test_config_validate_rejects_zero_cycle_cap`
- `test_config_validate_rejects_session_cap_exceeding_cycle_cap`
- `test_config_validate_rejects_hole_fraction_out_of_range` (-0.1, 1.1, NaN, Inf)
- `test_config_validate_accepts_hole_fraction_boundaries` (0.0, 0.5, 1.0)
- `test_config_validate_defaults_pass`

Result: 9 new tests pass. Full `config::` module: 435 passed, 0 failed.
`cargo build --workspace` clean; `cargo fmt` applied; clippy adds zero hits in edited regions.

## Issues / blockers
- None. config.rs is 11k lines (over the 500-line rule) but per brief Constraint 10 / #693
  this is thin additive wiring only — splitting out of scope for a knob add.
- Test-order coupling caught during dev: validate() returns on first failure, so the
  zero-cycle-cap test had to assert the `transcript_candidate_cycle_cap_bytes` field (which
  fires before the session>cycle ordering check). Fixed; captured as a pattern.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing/context_search — found #4070 (InferenceConfig
  five-site extension), #3817 (dual-site serde/Default), #646 (serde(default) back-compat),
  and crt-052 ADRs #4857/#4847/#4855. Applied the five-site pattern.
- Stored: entry #4859 "Config validate() check order couples to which-field-rejected tests;
  use RangeInclusive::contains for f64 NaN rejection" via /uni-store-pattern. (Five-site
  mechanics already covered by #4070 — stored only the novel validate-order + f64 NaN trap.)
