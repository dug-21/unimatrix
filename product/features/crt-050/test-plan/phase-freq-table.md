# Test Plan: phase-freq-table
# Component: unimatrix-server — src/services/phase_freq_table.rs

---

## Scope

All tests in this file are pure-Rust unit tests. No DB fixture needed.
They live in the existing `#[cfg(test)] mod tests` block in `phase_freq_table.rs`
and extend the existing helpers: `table_with()` and `rank_bucket()`.

New helpers needed:

```rust
// Build a Vec<PhaseFreqRow> for apply_outcome_weights input
fn make_freq_rows(entries: &[(&str, &str, u64, u32)]) -> Vec<PhaseFreqRow>
// entries: (phase, category, entry_id, freq)

// Build a Vec<PhaseOutcomeRow> for apply_outcome_weights input
fn make_outcome_rows(entries: &[(&str, &str, &str)]) -> Vec<PhaseOutcomeRow>
// entries: (phase, feature_cycle, outcome)
```

---

## R-02: outcome_weight() — Vocabulary Coverage and Priority Order (High)

`outcome_weight()` is private to the module. Tests use it directly from within
the `#[cfg(test)]` block via `super::outcome_weight()`.

### test_outcome_weight_pass_variants_return_1_0
```
Assert:
  - outcome_weight("pass")  == 1.0
  - outcome_weight("PASS")  == 1.0
  - outcome_weight("Pass")  == 1.0
```
*Covers: R-02 scenario 1 (pass → 1.0), AC-13b*

### test_outcome_weight_rework_variants_return_0_5
```
Assert:
  - outcome_weight("rework") == 0.5
  - outcome_weight("REWORK") == 0.5
  - outcome_weight("Rework") == 0.5
```
*Covers: R-02 scenario 1 (rework → 0.5), AC-13c*

### test_outcome_weight_fail_variants_return_0_5
```
Assert:
  - outcome_weight("fail")   == 0.5
  - outcome_weight("FAIL")   == 0.5
  - outcome_weight("FAILED") == 0.5
```
*Covers: R-02 scenario 1 (fail → 0.5), AC-13d*

### test_outcome_weight_unknown_and_empty_return_1_0
```
Assert:
  - outcome_weight("unknown")   == 1.0
  - outcome_weight("abandoned") == 1.0
  - outcome_weight("")          == 1.0
```
*Covers: R-02 scenario 1 (unknown/empty → 1.0 graceful degradation), AC-13e*

### test_outcome_weight_rework_checked_before_fail
```
// Guards against substring collision in hypothetical compound strings.
// "rework" contains neither "fail" — but "rework-and-fail" contains both.
// The rework branch must fire first (priority order ADR-003).
Assert:
  - outcome_weight("rework-and-fail") == 0.5  (rework branch fires, not fail)
  - outcome_weight("rework_fail")     == 0.5  (same)
```
*Covers: R-02 scenario 2, R-02 scenario 3 (priority order)*

---

## R-03 / AC-04: apply_outcome_weights() — Per-Phase Mean Aggregation (High)

### test_apply_outcome_weights_single_cycle_pass_weights_1_0
```
Arrange:
  - freq_rows = [(phase="delivery", category="decision", entry_id=1, freq=10)]
  - outcome_rows = [(phase="delivery", feature_cycle="c-1", outcome="PASS")]
Act:
  - result = apply_outcome_weights(freq_rows, outcome_rows)
Assert:
  - result[0].freq == 10  (10 * 1.0 = 10, cast to u32 via round())
```
*Covers: AC-13b, AC-04*

### test_apply_outcome_weights_single_cycle_rework_weights_0_5
```
Arrange:
  - freq_rows = [(phase="delivery", category="decision", entry_id=1, freq=10)]
  - outcome_rows = [(phase="delivery", feature_cycle="c-1", outcome="REWORK")]
Act:
  - result = apply_outcome_weights(freq_rows, outcome_rows)
Assert:
  - result[0].freq == 5  (10 * 0.5 = 5)
```
*Covers: AC-13c, AC-04*

### test_apply_outcome_weights_no_outcome_rows_defaults_to_1_0
```
Arrange:
  - freq_rows = [(phase="delivery", category="decision", entry_id=1, freq=8)]
  - outcome_rows = []  (empty Query B result)
Act:
  - result = apply_outcome_weights(freq_rows, outcome_rows)
Assert:
  - result[0].freq == 8  (default weight 1.0)
```
*Covers: AC-05, AC-13e*

### test_apply_outcome_weights_missing_phase_defaults_to_1_0
```
Arrange:
  - freq_rows = [(phase="delivery", category="decision", entry_id=1, freq=6)]
  - outcome_rows = [(phase="scope", feature_cycle="c-1", outcome="REWORK")]
    (different phase — no match for "delivery")
Act:
  - result = apply_outcome_weights(freq_rows, outcome_rows)
Assert:
  - result[0].freq == 6  (default 1.0 for unmatched phase)
```
*Covers: AC-13e*

### test_apply_outcome_weights_mixed_cycles_uses_per_phase_mean (R-03 key test)
```
// Concrete R-03 scenario:
// Phase "delivery": cycle-A (pass, weight 1.0), cycle-B (rework, weight 0.5)
// Per-phase mean = (1.0 + 0.5) / 2 = 0.75
// Entry X: total freq 18 (10 from cycle-A + 8 from cycle-B, but Query A already summed)
// Entry Y: total freq 15 (6 from cycle-A + 9 from cycle-B, Query A summed)
Arrange:
  - freq_rows = [
      (phase="delivery", category="decision", entry_id=10, freq=18),
      (phase="delivery", category="decision", entry_id=20, freq=15),
    ]
  - outcome_rows = [
      (phase="delivery", feature_cycle="cycle-A", outcome="PASS"),
      (phase="delivery", feature_cycle="cycle-B", outcome="REWORK"),
    ]
Act:
  - result = apply_outcome_weights(freq_rows, outcome_rows)
  - // per-phase mean for "delivery" = (1.0 + 0.5) / 2 = 0.75
Assert:
  - result[0].freq == 14  (round(18 * 0.75) = 13 or 14 — accept 13 or 14)
  - result[1].freq == 11  (round(15 * 0.75) = 11)
  - result[0].freq > result[1].freq  (rank ordering preserved)
```
*Covers: R-03 scenario 1 — per-phase mean preserves relative rank ordering*

### test_apply_outcome_weights_per_phase_mean_not_per_cycle (R-03 ordering invariant)
```
// Demonstrates that per-phase mean (not per-cycle weights) is used.
// Constructs a case where per-cycle weights would produce a different ordering.
// Phase "scope": cycle-A (pass=1.0), cycle-B (rework=0.5)
// Entry X: freq 10 (all from cycle-A)
// Entry Y: freq 8  (all from cycle-B)
// Per-phase mean: both get 0.75 → X weighted = 7.5, Y weighted = 6.0 → X > Y (correct)
// Wrong (per-cycle): X = 10*1.0=10, Y = 8*0.5=4 → same ordering (but different magnitudes)
// The key assertion is that MEAN is used, not best-weight.
Arrange:
  - freq_rows = [
      (phase="scope", category="decision", entry_id=1, freq=10),
      (phase="scope", category="decision", entry_id=2, freq=8),
    ]
  - outcome_rows = [
      (phase="scope", feature_cycle="ca", outcome="PASS"),
      (phase="scope", feature_cycle="cb", outcome="REWORK"),
    ]
Act:
  - result = apply_outcome_weights(freq_rows, outcome_rows)
Assert:
  - result[0].entry_id == 1  (entry 1 still ranks higher)
  - result[0].freq > result[1].freq
  - result[0].freq < 10  (weight applied; 10*1.0=10 would indicate no weighting or wrong path)
```
*Covers: R-03 scenario 2 — confirms mean aggregation, not per-cycle weights*

---

## AC-05: Empty cycle_events — No use_fallback Escalation

Covered by `test_apply_outcome_weights_no_outcome_rows_defaults_to_1_0` above.
Additional rebuild-level test:

### test_rebuild_with_empty_cycle_events_completes_without_error
```
// This test requires a minimal DB fixture or mocked store.
// If the store trait supports an in-memory SqlxStore (test_helpers pattern):
Arrange:
  - Store with 1 entry (category="decision")
  - 3 observations for that entry (PreToolUse, context_get, phase="delivery",
    ts_millis within window)
  - No cycle_events rows
Act:
  - PhaseFreqTable::rebuild(&store, lookback_days=30).await
Assert:
  - rebuild returns Ok
  - table.use_fallback == false
  - table.table not empty
  - All freq values unmodified (weight = 1.0)
```
*Covers: AC-05, FR-09 (NULL feature_cycle / no history → weight 1.0)*

---

## AC-06: Existing PhaseFreqTable Contracts Preserved

All existing tests in the `#[cfg(test)]` block must pass without modification:

| Existing Test | Contract |
|---------------|----------|
| `test_phase_freq_table_new_returns_cold_start` | cold-start `use_fallback=true` |
| `test_phase_affinity_score_use_fallback_returns_one` | cold-start returns 1.0 |
| `test_phase_affinity_score_absent_phase_returns_one` | absent phase returns 1.0 |
| `test_phase_affinity_score_absent_entry_returns_one` | absent entry returns 1.0 |
| `test_phase_freq_table_handle_poison_recovery` | `unwrap_or_else` poison pattern |
| `test_rebuild_normalization_three_entry_bucket_exact_scores` | rank formula |
| `test_rebuild_normalization_last_entry_in_five_bucket` | last rank > 0.0 |

These tests MUST NOT be modified. If any fail after implementation, it is a
regression — fix the code, not the tests.
*Covers: AC-06, FR-11*

---

## AC-08 / AC-13h: phase_category_weights()

### test_phase_category_weights_cold_start_returns_empty_map
```
Arrange:
  - t = PhaseFreqTable::new()  // use_fallback = true
Act:
  - weights = t.phase_category_weights()
Assert:
  - weights.is_empty()
```
*Covers: AC-08(a), AC-13h cold-start case*

### test_phase_category_weights_single_category_returns_1_0
```
Arrange:
  - table with phase="delivery", category="decision", bucket = [(1, 1.0)]
    (use_fallback = false)
Act:
  - weights = t.phase_category_weights()
Assert:
  - weights.get(&("delivery".to_string(), "decision".to_string())) == Some(&1.0)
  - (1 entry / 1 total = 1.0; edge case from R-07 / RISK-TEST-STRATEGY)
```
*Covers: AC-08 edge case, R-07*

### test_phase_category_weights_two_categories_sums_to_1_0
```
Arrange:
  - Phase "delivery" has two categories:
    - "decision": 2 entries → bucket = [(1, 1.0), (2, 0.5)]
    - "pattern":  1 entry  → bucket = [(3, 1.0)]
  - Total entries for phase "delivery" = 3
  - Expected: "decision" weight = 2/3, "pattern" weight = 1/3
  - (ADR-008: breadth-based; bucket.len() / total_entries_for_phase)
Act:
  - weights = t.phase_category_weights()
Assert:
  - weights[("delivery", "decision")] ≈ 2.0/3.0 (within 1e-6)
  - weights[("delivery", "pattern")]  ≈ 1.0/3.0 (within 1e-6)
  - Sum of all "delivery" values ≈ 1.0
```
*Covers: AC-08(b), AC-13h populated case, R-07*

### test_phase_category_weights_breadth_not_freq_sum
```
// R-07 explicit test: 1 entry in category A (freq=10), 10 entries in category B (freq=1 each)
// Breadth (ADR-008): A=1/11, B=10/11
// (NOT frequency-weighted which would give A=10/20, B=10/20)
Arrange:
  - Phase "scope":
    - "decision": 1 entry  → bucket len = 1
    - "pattern": 10 entries → bucket len = 10
  - Total = 11
Act:
  - weights = t.phase_category_weights()
Assert:
  - weights[("scope", "decision")] ≈ 1.0/11.0 (within 1e-5)
  - weights[("scope", "pattern")]  ≈ 10.0/11.0 (within 1e-5)
  - Doc comment visible in implementation confirming "breadth-based" distribution
```
*Covers: R-07 scenario 1 — breadth-based formula documented and tested*

### test_phase_category_weights_multiple_phases_independent
```
// Each phase must sum to 1.0 independently.
Arrange:
  - Phase "delivery": "decision"=2 entries, "pattern"=1 entry (total 3)
  - Phase "scope":    "decision"=1 entry, "lesson-learned"=1 entry (total 2)
Act:
  - weights = t.phase_category_weights()
Assert:
  - delivery_sum = sum of all weights where phase="delivery" ≈ 1.0
  - scope_sum    = sum of all weights where phase="scope" ≈ 1.0
```
*Covers: AC-08(b) per-phase sum = 1.0 invariant*
