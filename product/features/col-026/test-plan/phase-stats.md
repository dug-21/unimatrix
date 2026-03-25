# Test Plan: phase-stats

**Crate**: `unimatrix-server/src/mcp/tools.rs` (+ `services/observation.rs` visibility change)
**Risks covered**: R-01, R-02, R-03, R-05, R-09, R-12
**ACs covered**: AC-04, AC-05, AC-06, AC-07

---

## Component Scope

This component implements:
- `compute_phase_stats(events: &[CycleEventRecord], attributed: &[ObservationRecord]) -> Vec<PhaseStats>`
- Handler steps 10h (PhaseStats computation) and 10i (`get_cycle_start_goal`, `is_in_progress`
  derivation, `attribution_path` assignment).
- `cycle_ts_to_obs_millis` visibility change to `pub(crate)`.

The function is pure (no async, no DB) and testable in isolation in a `#[cfg(test)]` block in
`tools.rs`. Handler-level behavior (10i) requires integration tests.

---

## Unit Test Expectations

All unit tests live in `crates/unimatrix-server/src/mcp/tools.rs` `#[cfg(test)] mod tests`.

### R-01: Timestamp Conversion Boundary Tests

#### Test: `test_phase_stats_obs_in_correct_window_millis_boundary` (R-01, AC-06)

**Scenario**: Verify `cycle_ts_to_obs_millis` is used, not inline `* 1000`.
**Setup**:
- `cycle_start.timestamp = 1700000000` (epoch-seconds)
- `cycle_phase_end.timestamp = 1700000100` (100 seconds later)
- `cycle_stop.timestamp = 1700000200`
- One observation with `ts = cycle_ts_to_obs_millis(1700000100)` (exactly at boundary)
- One observation with `ts = cycle_ts_to_obs_millis(1700000100) - 1` (one millisecond before)

**Assert**: The observation at the boundary timestamp falls in phase 2 (start-inclusive).
The observation before the boundary falls in phase 1. Phase 1 `record_count == 1`.

#### Test: `test_phase_stats_static_no_inline_multiply` (R-01)

**Scenario**: Static grep / source scan that `compute_phase_stats` code contains no `* 1000`.
**Implementation**: `#[test]` that reads the source file via `include_str!` and asserts no
match for pattern `\* 1000`.
**Assert**: Zero occurrences of `* 1000` in the PhaseStats computation code block.

#### Test: `test_cycle_ts_to_obs_millis_overflow_guard` (R-01)

**Scenario**: Pass `i64::MAX / 1000 + 1` to `cycle_ts_to_obs_millis`.
**Assert**: Does not panic; returns a saturated or clamped value (per the helper's behavior).

### R-02: Phase Window Extraction Edge Cases

#### Test: `test_phase_stats_no_phase_end_events` (R-02, AC-06)

**Scenario**: `events` contains only `cycle_start` and `cycle_stop`, no `cycle_phase_end`.
**Assert**:
- Result has exactly one `PhaseStats` entry.
- `duration_secs == cycle_stop.timestamp - cycle_start.timestamp`.
- `phase` field is `""` or a sentinel — formatter renders `—` instead of empty cell (verify
  with formatter test `test_phase_timeline_empty_phase_name`).

#### Test: `test_phase_stats_zero_duration_no_panic` (R-02)

**Scenario**: `cycle_start.timestamp == cycle_stop.timestamp` (same second).
**Assert**:
- `PhaseStats[0].duration_secs == 0`.
- No panic or division-by-zero.
- `format_duration(0)` returns `"0m"` (existing test already covers this, but verify via
  formatter that the Phase Timeline renders `0m` without error).

#### Test: `test_phase_stats_empty_phase_name_on_phase_end` (R-02)

**Scenario**: `cycle_phase_end` event with `phase = None` (or empty string).
**Assert**: `PhaseStats.phase` is `""` or a defined sentinel. No panic. `record_count` computed
normally.

#### Test: `test_phase_stats_no_observations_in_window` (R-02, AC-06)

**Scenario**: Phase window with zero observations in the time window.
**Assert**: `record_count == 0`, `knowledge_served == 0`, `knowledge_stored == 0`,
`gate_result == GateResult::Unknown`. No panic.

#### Test: `test_phase_stats_empty_attributed_slice` (Integration risk)

**Scenario**: `attributed = vec![]`, non-empty `events`.
**Assert**: `PhaseStats` rows produced with `record_count = 0`, `knowledge_served = 0`. No panic.

### R-03: GateResult Inference

All tests call the `GateResult` inference directly or via `compute_phase_stats` with synthetic
events.

#### Test: `test_gate_result_pass_case_insensitive` (R-03)

**Scenarios**: `outcome = "PASS"` → `Pass`; `outcome = "pass"` → `Pass`.

#### Test: `test_gate_result_fail_patterns` (R-03)

**Scenarios**:
- `"failed: type errors"` → `Fail`
- `"error in gate 2b"` → `Fail`

#### Test: `test_gate_result_rework` (R-03)

**Scenarios**:
- `"rework required"` → `Rework`
- `"REWORK"` → `Rework`

#### Test: `test_gate_result_empty_string` (R-03)

**Scenario**: `outcome = ""` → `Unknown`.

#### Test: `test_gate_result_none` (R-03)

**Scenario**: `outcome = None` (no `cycle_phase_end` event) → `Unknown`.

#### Test: `test_gate_result_multi_keyword_pass_rework` (R-03)

**Scenario**: `"pass after rework"` — contains both "pass" and "rework".
**Assert**: Evaluation order matches spec. Document which variant wins and assert it explicitly.
Per the risk strategy: `Rework` takes precedence if pass_count > 1 and final pass succeeds.
If inference is keyword-order-first (not pass_count-based), `Pass` would win. This test
pins the actual behavior at design time.

#### Test: `test_gate_result_compass_substring` (R-03)

**Scenario**: `"compass"` — contains "pass" as substring.
**Assert**: Documents whether this produces `Pass` or `Unknown`. This is a known fragility
(ARCHITECTURE.md §Note). If `contains("pass")` is used, `"compass"` → `Pass`. Test asserts
the actual behavior and documents it in a `// KNOWN: contains() matches embedded words` comment.

### R-05: `is_in_progress` Derivation (handler step 10i)

These tests verify the derivation logic, not just the struct. They belong in tools.rs as unit
tests of an extracted helper function `derive_is_in_progress(events: &[CycleEventRecord]) -> Option<bool>`.

#### Test: `test_is_in_progress_empty_events` (R-05, AC-05)

`events = vec![]` → `None`.

#### Test: `test_is_in_progress_start_only` (R-05, AC-05)

`events` has `cycle_start`, no `cycle_stop` → `Some(true)`.

#### Test: `test_is_in_progress_start_and_stop` (R-05, AC-05)

`events` has both `cycle_start` and `cycle_stop` → `Some(false)`.

### R-09: Attribution Path Labels (handler integration)

These require the handler to be invoked, making them higher-effort unit tests that mock the DB
layer. If such mocking is not feasible within unit test scope, these move to infra-001
integration tests.

#### Test: `test_attribution_path_primary` (R-09, AC-04)

`load_cycle_observations` returns non-empty → `attribution_path = "cycle_events-first (primary)"`.

#### Test: `test_attribution_path_legacy` (R-09, AC-04)

Primary returns empty; `load_feature_observations` returns non-empty →
`attribution_path = "sessions.feature_cycle (legacy)"`.

#### Test: `test_attribution_path_fallback` (R-09, AC-04)

Both primary and legacy empty; `load_unattributed_sessions` used →
`attribution_path = "content-scan (fallback)"`.

#### Test: `test_attribution_path_all_empty` (R-09, AC-04)

All three return empty → `attribution_path = None` (or defined sentinel per spec).

### R-12: `phase_stats = Some(vec![])` Canonicalization

#### Test: `test_phase_stats_empty_events_produces_none` (R-12)

**Scenario**: `compute_phase_stats` called with `events = vec![]`.
**Assert**: Handler sets `report.phase_stats = None`, not `Some(vec![])`.
The function itself may return an empty vec, but the handler must convert it to `None` before
setting the field.

#### Test: `test_phase_stats_error_path_produces_none` (R-12)

**Scenario**: Computation step encounters an error (e.g., timestamp overflow, malformed event).
**Assert**: Handler sets `phase_stats = None` and emits `tracing::warn!`. Report continues.

### R-02 / AC-07: Rework Detection

#### Test: `test_phase_stats_rework_pass_count` (R-02, AC-07)

**Scenario**: `events` contains two `cycle_phase_end` events both with `phase = "design"`.
**Assert**:
- Two `PhaseStats` rows for `phase = "design"`.
- Row 1: `pass_number = 1`, `pass_count = 2`.
- Row 2: `pass_number = 2`, `pass_count = 2`.
- `pass_count > 1` is the rework signal for the formatter's footnote.

### AC-06: Phase Aggregation Fields

#### Test: `test_phase_stats_knowledge_served_counted` (AC-06)

**Scenario**: Window contains 3 `PreToolUse` observations with `tool = "context_search"`,
1 with `tool = "context_store"`, 2 with `tool = "Read"`.
**Assert**: `knowledge_served = 3`, `knowledge_stored = 1`.

#### Test: `test_phase_stats_tool_distribution` (AC-06)

**Scenario**: Window observations have mixed event_types.
**Assert**: `tool_distribution.read`, `execute`, `write`, `search` counts match event_type
bucket mapping (same as `compute_session_summaries`).

#### Test: `test_phase_stats_session_count` (AC-06)

**Scenario**: 4 observations in window, across 2 distinct `session_id` values.
**Assert**: `session_count = 2`.

#### Test: `test_phase_stats_agent_deduplication` (AC-06)

**Scenario**: 3 `SubagentStart` events in window, 2 with same agent name, 1 different.
**Assert**: `agents.len() == 2`, first-seen order preserved.

---

## Integration Test Expectations

infra-001 `test_tools.py`:

- **Test**: `test_cycle_review_phase_timeline_present`
  - Seed `cycle_start` + `cycle_phase_end` + `cycle_stop` via `context_cycle`.
  - Call `context_cycle_review(format="markdown")`.
  - Assert `## Phase Timeline` present in response.
  - Assert at least one `|` row below the header row.

- **Test**: `test_cycle_review_is_in_progress_json`
  - Seed `cycle_start` only.
  - Call `context_cycle_review(format="json")`.
  - Parse JSON; assert `is_in_progress == true`.

See OVERVIEW.md Integration Tests 1 and 2.

---

## Edge Cases

- `cycle_start` and first `cycle_phase_end` share the same timestamp: `duration_secs = 0`.
  No panic. `format_duration(0) == "0m"`.
- Multiple phases in alphabetical order (not all sequential): `pass_number` tracks occurrence
  order, not alphabetical position.
- Single-session feature: `session_count = 1` per phase window. No crash.
- Phase window spanning midnight UTC: no impact (timestamps are epoch-millis, not wall-clock).
