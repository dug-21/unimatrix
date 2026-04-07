# Test Plan: store-queries
# Component: unimatrix-store — query_log.rs (or phase_freq.rs)

---

## Scope

All tests in this file exercise the two new store query functions and the
`MILLIS_PER_DAY` constant. They live in `query_log_tests.rs` (following the
existing `#[path = "query_log_tests.rs"]` pattern) or a new `phase_freq_tests.rs`
if the implementer moves the functions to `phase_freq.rs`.

All DB-backed tests use `open_test_store()` from `crate::test_helpers` and insert
rows via direct `sqlx::query` calls (the existing pattern from `query_log_tests.rs`).

---

## Test Helpers Needed

```rust
// Insert an observations row directly for test control
async fn insert_observation(
    store: &SqlxStore,
    session_id: &str,
    phase: Option<&str>,
    hook: &str,          // "PreToolUse" or "PostToolUse"
    tool: &str,
    input: Option<&str>, // JSON string e.g. r#"{"id":42}"#
    ts_millis: i64,
)

// Insert a sessions row (with or without feature_cycle)
async fn insert_session(
    store: &SqlxStore,
    session_id: &str,
    feature_cycle: Option<&str>,
)

// Insert a cycle_events row
async fn insert_cycle_event(
    store: &SqlxStore,
    cycle_id: &str,
    phase: &str,
    event_type: &str,
    outcome: Option<&str>,
)
```

---

## AC-SV-01 / R-01: Write-Path Contract (Critical)

**Risk:** If the hook-listener write path produces double-encoded input,
`json_extract(input, '$.id')` returns NULL for all hook-path rows and Query A
returns zero rows silently. ADR-005 confirms no double-encoding exists, but a
regression test is required.

### test_observation_input_json_extract_returns_id_for_hook_path
```
Arrange:
  - Insert observation with input = r#"{"id": 42}"# (as stored by the hook listener)
  - Use hook = 'PreToolUse', tool = 'context_get', phase = 'delivery'
  - ts_millis = now_millis - 1000 (within any reasonable window)
Act:
  - Execute: SELECT json_extract(input, '$.id') FROM observations WHERE session_id = ?
Assert:
  - Result is non-NULL
  - Cast to integer equals 42
```
*Covers: AC-SV-01, R-01 scenario 1*

---

## AC-01 / AC-13a: Rebuild Source Is observations, Not query_log

### test_query_phase_freq_observations_returns_rows_when_observations_populated
```
Arrange:
  - Insert entry (category = "decision")
  - Insert 5 observations for that entry (hook='PreToolUse', tool='context_get',
    phase='delivery', ts_millis within window)
  - query_log is empty
Act:
  - call store.query_phase_freq_observations(30)
Assert:
  - Result is non-empty (len >= 1)
  - Row has phase="delivery", category="decision", entry_id=<inserted_id>, freq=5
```
*Covers: AC-01(a), AC-13a partial*

### test_query_phase_freq_observations_returns_empty_when_observations_empty
```
Arrange:
  - Insert rows into query_log only; observations table empty
Act:
  - call store.query_phase_freq_observations(30)
Assert:
  - Result is empty Vec
  - (Caller in phase_freq_table.rs will set use_fallback = true)
```
*Covers: AC-01(b), AC-13a*

---

## AC-02 / AC-13f: Tool Name Filter — Four Variants

### test_query_phase_freq_observations_includes_all_four_tool_variants
```
Arrange:
  - Insert entry (category = "decision")
  - Insert one observation each for:
    - tool = 'context_get'
    - tool = 'mcp__unimatrix__context_get'
    - tool = 'context_lookup', input = r#"{"id": <id>}"#
    - tool = 'mcp__unimatrix__context_lookup', input = r#"{"id": <id>}"#
  - All: hook='PreToolUse', phase='delivery', ts_millis within window
Act:
  - call store.query_phase_freq_observations(30)
Assert:
  - freq column sums to 4 for the (delivery, decision, <id>) group
    (or 4 distinct rows if grouped separately — depends on SQL GROUP BY)
```
*Covers: AC-02, AC-13f, FR-02*

### test_query_phase_freq_observations_excludes_context_search_tool
```
Arrange:
  - Insert entry (category = "decision")
  - Insert observation with tool = 'context_search', phase = 'delivery',
    hook = 'PreToolUse', ts_millis within window
    input = r#"{"id": <id>}"# (even if present, tool name not in IN clause)
Act:
  - call store.query_phase_freq_observations(30)
Assert:
  - Result is empty Vec
```
*Covers: AC-02 exclusion case, FR-02*

---

## AC-02 / R-10: hook Column Filter (Not hook_event)

### test_query_phase_freq_observations_filters_pretooluse_only
```
Arrange:
  - Insert entry (category = "decision")
  - Insert observation with hook='PreToolUse', tool='context_get',
    phase='delivery', ts_millis within window
  - Insert observation with hook='PostToolUse', tool='context_get',
    phase='delivery', ts_millis within window (same entry_id)
Act:
  - call store.query_phase_freq_observations(30)
Assert:
  - freq = 1 (only the PreToolUse row is counted)
  - No runtime SQL error (confirms column name is 'hook' not 'hook_event')
```
*Covers: FR-04, R-10, AC-02 (PreToolUse filter)*

---

## AC-03: CAST and String-Form IDs

### test_query_phase_freq_observations_cast_handles_string_form_id
```
Arrange:
  - Insert entry (category = "pattern") — note its assigned id
  - Insert observation with input = r#"{"id": "<id_as_string>"}"#
    (string-form ID, e.g. r#"{"id": "7"}"# when entry_id is 7)
  - hook='PreToolUse', tool='context_get', phase='delivery', ts_millis within window
Act:
  - call store.query_phase_freq_observations(30)
Assert:
  - Result contains a row with entry_id = <numeric_id>
  - (Confirms CAST handles both integer-form and string-form '$.id')
```
*Covers: AC-03, FR-05, C-03*

### test_query_phase_freq_observations_excludes_null_id_observations
```
Arrange:
  - Insert observation with input = r#"{"filter": "topic"}"# (no $.id field)
  - hook='PreToolUse', tool='context_lookup', phase='delivery', ts_millis within window
Act:
  - call store.query_phase_freq_observations(30)
Assert:
  - Result is empty Vec (json_extract IS NOT NULL guard excludes it)
```
*Covers: AC-13g, FR-03 (filter-based lookup excluded)*

---

## AC-07 / R-05: ts_millis Lookback Boundary

### test_millis_per_day_constant_value
```
Assert:
  - MILLIS_PER_DAY == 86_400_000i64
  - (Compile-time constant: assert_eq!(MILLIS_PER_DAY, 86_400 * 1_000))
```
*Covers: R-05 scenario 3*

### test_query_phase_freq_observations_respects_ts_millis_boundary
```
Arrange:
  - Insert entry (category = "decision")
  - Compute cutoff_millis for lookback_days = 1:
    cutoff = now_millis - MILLIS_PER_DAY
  - Insert observation_inside:  ts_millis = cutoff + 500    (inside window)
  - Insert observation_outside: ts_millis = cutoff - 500    (outside window)
  - Both: hook='PreToolUse', tool='context_get', phase='delivery'
Act:
  - call store.query_phase_freq_observations(1)
Assert:
  - Result contains exactly 1 row
  - Row corresponds to observation_inside (freq = 1)
```
*Covers: AC-07, R-05 scenarios 1–2, ADR-006*

### test_query_phase_freq_observations_lookback_30_days_arithmetic
```
Arrange:
  - Insert entry
  - Insert observation at now_millis - (30 * MILLIS_PER_DAY) + 5_000 (inside 30d window)
Act:
  - call store.query_phase_freq_observations(30)
Assert:
  - Result non-empty
  - (Validates the 30-day multiplication doesn't overflow or produce seconds instead of ms)
```
*Covers: R-05 scenario 1 (30-day boundary arithmetic)*

---

## AC-15 / R-08: Query B — NULL feature_cycle Degradation

### test_query_phase_outcome_map_excludes_null_feature_cycle_sessions
```
Arrange:
  - Insert session with feature_cycle = NULL
  - Insert cycle_events row for a phase (event_type='cycle_phase_end', outcome='PASS')
    with cycle_id pointing to above session
Act:
  - call store.query_phase_outcome_map()
Assert:
  - Result is empty Vec
  - (NULL feature_cycle filtered by s.feature_cycle IS NOT NULL)
  - No error returned
```
*Covers: AC-15(a), FR-10, R-08, ADR-001 IS NOT NULL predicate*

### test_query_phase_outcome_map_returns_rows_for_non_null_sessions
```
Arrange:
  - Insert session with feature_cycle = "crt-050"
  - Insert cycle_events row: event_type='cycle_phase_end', phase='delivery',
    outcome='PASS', cycle_id='crt-050'
Act:
  - call store.query_phase_outcome_map()
Assert:
  - Result contains 1 row: phase="delivery", feature_cycle="crt-050", outcome="PASS"
```
*Covers: Query B happy path; input to apply_outcome_weights*

---

## AC-09 Verification (not a test — grep check)

At Stage 3c, confirm:
```bash
grep -r 'query_phase_freq_table' /workspaces/unimatrix/crates/ --include='*.rs'
```
Must return zero results (function deleted).
*Covers: AC-09, FR-13*
