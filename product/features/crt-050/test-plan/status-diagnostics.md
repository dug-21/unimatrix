# Test Plan: status-diagnostics
# Component: unimatrix-server — src/services/status.rs

---

## Scope

Tests in this file cover the two diagnostic functions modified/added in `status.rs`:

1. `warn_phase_freq_lookback_mismatch()` — renamed from crt-036; field reference updated
2. `warn_observations_coverage()` — new; emits `tracing::warn!` when distinct
   `(phase, session_id)` pair count falls below `min_phase_session_pairs`

These are tick-time advisory diagnostics. They do not block rebuild or return errors.

---

## AC-11: crt-036 Diagnostic Updated

### test_warn_phase_freq_lookback_mismatch_uses_new_field_name
```
// Code review / static verification:
// The warn_phase_freq_lookback_mismatch function must reference
// cfg.phase_freq_lookback_days, not cfg.query_log_lookback_days.
// The warning message text must say "phase_freq_lookback_days", not "query_log_lookback_days".
// Also: the warning message must note it governs the 'observations' window (not 'query_log').
//
// Verification method: code review + grep at Stage 3c:
//   grep -r 'query_log_lookback_days' crates/unimatrix-server/src/services/status.rs
// Must return zero results.
```
*Covers: AC-11 (first part), FR-15, ADR-004*

---

## AC-11 / AC-14 / R-04: warn_observations_coverage() Threshold Gate

The coverage threshold test requires knowing the distinct `(phase, session_id)` count
within the lookback window. The implementation queries this via SQL (a scalar subquery
or dedicated store function). Tests use an in-memory store fixture.

### test_warn_observations_coverage_emits_warn_below_threshold
```
Arrange:
  - Store with N = 5 (default min_phase_session_pairs)
  - Insert N-1 = 4 distinct (phase, session_id) pairs in observations
    (each pair: unique session_id + phase, PreToolUse hook, tool in IN clause,
     ts_millis within window)
  - cfg.min_phase_session_pairs = 5
Act:
  - call warn_observations_coverage(&store, &cfg).await
    (or trigger via rebuild path if coverage check is inline)
Assert:
  - tracing::warn! is emitted with the count (4) and threshold (5)
  - use_fallback is set to true in the returned/modified PhaseFreqTable
    (if the gate is inside rebuild) OR the caller observes use_fallback = true
```
*Covers: AC-14 (N-1 scenario), R-04 scenario 1, FR-17*

### test_warn_observations_coverage_no_warn_at_threshold
```
Arrange:
  - Insert exactly N = 5 distinct (phase, session_id) pairs
  - cfg.min_phase_session_pairs = 5
Act:
  - call warn_observations_coverage (or rebuild path)
Assert:
  - No tracing::warn! emitted for coverage
  - use_fallback = false (assuming observations also meet other conditions)
```
*Covers: AC-14 (N scenario), R-04 scenario 2, FR-17*

### test_warn_observations_coverage_threshold_1_with_1_pair_is_normal_operation
```
// R-04 edge case: threshold = 1 with exactly 1 pair must not trigger spurious fallback.
Arrange:
  - Insert 1 distinct (phase, session_id) pair
  - cfg.min_phase_session_pairs = 1
Assert:
  - use_fallback = false (pair count meets threshold)
  - No coverage warning emitted
```
*Covers: R-04 edge case 3*

### test_warn_observations_coverage_single_session_many_observations
```
// R-04 edge case: many observations from one session = 1 distinct pair.
Arrange:
  - Insert 50 observations all with session_id="sess-001", phase="delivery"
  - cfg.min_phase_session_pairs = 5
Assert:
  - Distinct (phase, session_id) count = 1
  - Coverage warning emitted (1 < 5)
  - use_fallback = true
```
*Covers: R-04 edge case 4 (count is pairs not rows), AC-14*

---

## AC-11: warn_observations_coverage Warning Message Content (Code Review)

The warning message emitted by `warn_observations_coverage()` must include:
- The actual distinct pair count
- The configured threshold value (`min_phase_session_pairs`)
- A reference to the `phase_freq_lookback_days` window that was searched

Example expected message format:
```
warn!("observations coverage below threshold: {} distinct (phase, session_id) \
       pairs in {} day window (min_phase_session_pairs={}); using fallback scoring",
      count, lookback_days, threshold);
```

*Covers: AC-11 (warning emitted with count and threshold), FR-16*

---

## Test Infrastructure Note

The `warn_observations_coverage()` function relies on a store query for the pair count.
If the function uses a dedicated store method (e.g., `count_phase_session_pairs()`), that
method should have a standalone unit test in `query_log_tests.rs` (or equivalent):

### test_count_phase_session_pairs_returns_distinct_pair_count
```
Arrange:
  - Insert 3 observations: (sess-1, delivery), (sess-2, delivery), (sess-1, scope)
    — all within window, PreToolUse, valid tool name
  - Also insert 2 duplicate observations (sess-1, delivery again) — same pair
Act:
  - count = store.count_phase_session_pairs(lookback_days=30).await
Assert:
  - count == 3  (distinct pairs: (sess-1, delivery), (sess-2, delivery), (sess-1, scope))
```
*Covers: correctness of distinct pair count (SQL DISTINCT behavior)*

If the count is computed inline inside `rebuild()` using the Query A rows (not a separate
SQL query), this test moves to the `phase-freq-table.md` plan as a rebuild-input test.
