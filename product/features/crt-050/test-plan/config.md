# Test Plan: config
# Component: unimatrix-server — src/infra/config.rs

---

## Scope

Tests in this file cover:
1. `InferenceConfig::phase_freq_lookback_days` rename with serde alias (R-06 / AC-10)
2. `InferenceConfig::min_phase_session_pairs` new field (R-04 / AC-14)
3. `MILLIS_PER_DAY` constant (R-05 — if the constant lives in `config.rs` rather than
   `query_log.rs`/`phase_freq.rs`; most likely it lives in the store crate, but if it
   is referenced or re-exported here, test it here too)

All tests live in the existing `#[cfg(test)]` block in `config.rs`, extending the existing
serde deserialization tests.

---

## AC-10 / R-06: Renamed Field and Serde Alias

### test_inference_config_phase_freq_lookback_days_new_name_deserializes
```
Arrange:
  - JSON: r#"{"phase_freq_lookback_days": 30}"#
Act:
  - let cfg: InferenceConfig = serde_json::from_str(json).unwrap()
Assert:
  - cfg.phase_freq_lookback_days == 30
```
*Covers: AC-10 (new name works)*

### test_inference_config_query_log_lookback_days_alias_deserializes
```
Arrange:
  - JSON: r#"{"query_log_lookback_days": 45}"#
    (old name, must be accepted via serde alias for backward compat)
Act:
  - let cfg: InferenceConfig = serde_json::from_str(json).unwrap()
Assert:
  - cfg.phase_freq_lookback_days == 45
  - (serde alias routes old TOML config key to renamed field)
```
*Covers: AC-10 (alias works), ADR-004 backward-compat contract*

### test_inference_config_old_field_name_not_present_as_field
```
// Compiler-level verification — not a runtime test.
// The implementer must confirm no struct literal in the codebase still uses
// 'query_log_lookback_days:' as a field name after the rename.
// Stage 3c grep check:
//   grep -r 'query_log_lookback_days' crates/ --include='*.rs'
// Must return only the #[serde(alias = "query_log_lookback_days")] line.
```
*Covers: R-06 scenario 3 (grep gate)*

---

## AC-10 / AC-14: min_phase_session_pairs New Field

### test_inference_config_min_phase_session_pairs_default_is_5
```
Arrange:
  - InferenceConfig::default() (or minimal JSON without min_phase_session_pairs)
Assert:
  - cfg.min_phase_session_pairs == 5
  - (ADR-007 / NFR-04: authoritative default is 5, not 10)
```
*Covers: AC-14 field present with correct default, NFR-04*

### test_inference_config_min_phase_session_pairs_deserializes
```
Arrange:
  - JSON: r#"{"min_phase_session_pairs": 10}"#
Act:
  - let cfg: InferenceConfig = serde_json::from_str(json).unwrap()
Assert:
  - cfg.min_phase_session_pairs == 10
```
*Covers: AC-14 field is configurable*

### test_inference_config_min_phase_session_pairs_validates_range
```
// InferenceConfig::validate() must enforce [1, 1000].
Arrange:
  - Cfg with min_phase_session_pairs = 0   → expect validation error
  - Cfg with min_phase_session_pairs = 1001 → expect validation error
  - Cfg with min_phase_session_pairs = 1   → valid
  - Cfg with min_phase_session_pairs = 1000 → valid
Assert:
  - validate() returns Err for out-of-range values
  - validate() returns Ok for boundary values 1 and 1000
```
*Covers: AC-14 range [1, 1000], FR-17*

---

## Validation Error Messages (Code Review)

The validation error message for `phase_freq_lookback_days` must reference the new field name,
not `query_log_lookback_days`. This is a code-review check, not a runtime assertion, but the
Stage 3c tester should confirm:

- `InferenceConfig::validate()` error text contains "phase_freq_lookback_days"
- No remaining error text contains "query_log_lookback_days"

*Covers: ADR-004 "update validation error message field name string"*
