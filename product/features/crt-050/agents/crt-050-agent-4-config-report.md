# Agent Report: crt-050-agent-4-config

**Component**: config
**Feature**: crt-050 (#542)
**Agent ID**: crt-050-agent-4-config

---

## Files Modified

- `crates/unimatrix-server/src/infra/config.rs` — primary component file
- `crates/unimatrix-server/src/background.rs` — field access update (in-scope per IMPLEMENTATION-BRIEF)
- `crates/unimatrix-server/src/services/status.rs` — field access update (single call-site fix, consequence of rename)
- `crates/unimatrix-server/src/services/phase_freq_table.rs` — minimal unblock: `query_phase_freq_table` → `query_phase_freq_observations` (one line; phase-freq-table agent's domain, updated to unblock build/tests)

---

## Changes Made

### config.rs

1. **Field rename**: `query_log_lookback_days` → `phase_freq_lookback_days` with `#[serde(alias = "query_log_lookback_days")]` for backward compat (ADR-004)
2. **New field**: `min_phase_session_pairs: u32` (default 5, range [1, 1000]) added after `phase_freq_lookback_days`
3. **Default fn rename**: `default_query_log_lookback_days` → `default_phase_freq_lookback_days`; new `default_min_phase_session_pairs() -> u32 { 5 }`
4. **Default impl**: Updated struct literal with both new fields
5. **validate()**: Updated field name + error string; added `min_phase_session_pairs` range check [1, 1000]
6. **RetentionConfig doc comment**: Updated cross-reference to `phase_freq_lookback_days` with historical note
7. **merge_configs**: Updated struct literal block for both fields
8. **Tests**: Replaced 3 old tests; added 9 new tests covering T-CFG-01 through T-CFG-09

### background.rs

- Line 622: `inference_config.query_log_lookback_days` → `inference_config.phase_freq_lookback_days`

### status.rs

- Line 1407: `inference_config.query_log_lookback_days` → `inference_config.phase_freq_lookback_days` (single call-site fix; full status-diagnostics component belongs to another agent)

### phase_freq_table.rs

- Line 119: `store.query_phase_freq_table(lookback_days)` → `store.query_phase_freq_observations(lookback_days)` — minimal unblock because the store-queries agent deleted `query_phase_freq_table` from `SqlxStore`, leaving the call site broken and the entire crate uncompilable. Full rebuild logic update belongs to the phase-freq-table component agent.

---

## Test Results

- **358 passed, 0 failed** (`cargo test -p unimatrix-server -- config`)
- New tests passing (confirmed in test output):
  - `test_inference_config_phase_freq_lookback_days_default`
  - `test_phase_freq_lookback_days_default_from_empty_toml`
  - `test_inference_config_phase_freq_lookback_days_new_name_deserializes`
  - `test_inference_config_query_log_lookback_days_alias_deserializes`
  - `test_inference_config_crt050_defaults`
  - `test_inference_config_min_phase_session_pairs_deserializes`
  - `test_validate_lookback_days_zero_is_error`
  - `test_validate_lookback_days_3651_is_error`
  - `test_validate_lookback_days_boundary_values_pass`
  - `test_validate_min_phase_session_pairs_zero_is_error`
  - `test_validate_min_phase_session_pairs_1001_is_error`
  - `test_validate_min_phase_session_pairs_boundary_values_pass`

---

## Grep Gate (T-CFG-10)

After implementation, remaining `query_log_lookback_days` occurrences in config.rs:
- Line 459: doc comment (historical reference in field doc)
- Line 463: `#[serde(alias = "query_log_lookback_days")]` — correct, this is the alias

All struct-literal and field-access usages removed. Remaining occurrences in `status.rs` (comment lines and `warn_phase_freq_lookback_mismatch` function parameter — those are in the status-diagnostics component's domain) and test strings (verifying alias behavior) are intentional.

---

## Issues / Blockers

None for the config component. The phase-freq-table component's `phase_freq_table.rs` had a pre-existing compile error (calling deleted `query_phase_freq_table`), which was patched minimally to unblock the build. The phase-freq-table agent should complete the full two-query rebuild implementation per the IMPLEMENTATION-BRIEF.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #4226 (crt-050 ADR rename decision), #3934 (adding config section procedure), #4132 (lesson: new InferenceConfig fields require validate() — useful reminder), #3743 (unit-testing InferenceConfig via toml::from_str — confirmed test approach). Applied: used `serde_json::from_str` for `min_phase_session_pairs` deserialization test and `toml::from_str` for lookback days alias test per entry #3743.
- Stored: entry #4237 "InferenceConfig field rename: serde alias covers TOML deserialization only — field-access callers in non-struct-literal sites are NOT caught by the compiler" via `/uni-store-pattern`
