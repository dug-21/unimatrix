# Component A — infra/coherence.rs

## Purpose

Pure-function computation layer for Lambda, the composite coherence health metric.
After crt-048 this module owns exactly three dimension scorers (graph, embedding,
contradiction), the 3-dimension weighted `compute_lambda()`, and the pruned
`generate_recommendations()`. The freshness-related functions are deleted entirely.

---

## Module-Level Changes

### Module doc comment (line 1-7)

Update the opening comment to reflect 3-dimension Lambda:

```
BEFORE: "Computes four dimension scores that combine into a single coherence value"
AFTER:  "Computes three dimension scores that combine into a single coherence value"
```

---

## Struct: CoherenceWeights

### Current state (4 fields)
```rust
pub struct CoherenceWeights {
    pub confidence_freshness: f64,
    pub graph_quality: f64,
    pub embedding_consistency: f64,
    pub contradiction_density: f64,
}
```

### Post-crt-048 state (3 fields — AC-01)
```rust
pub struct CoherenceWeights {
    pub graph_quality: f64,
    pub embedding_consistency: f64,
    pub contradiction_density: f64,
}
```

Action: Delete the `pub confidence_freshness: f64` field line.

---

## Constant: DEFAULT_WEIGHTS

### Current state
```rust
pub const DEFAULT_WEIGHTS: CoherenceWeights = CoherenceWeights {
    confidence_freshness: 0.35,
    graph_quality: 0.30,
    embedding_consistency: 0.15,
    contradiction_density: 0.20,
};
```

### Post-crt-048 state (AC-02, weight literals locked per OQ-1)
```rust
/// Default weights (ADR-001 crt-048): graph 0.46, contradiction 0.31, embedding 0.23.
/// Derived by proportional re-normalization of original 0.30:0.20:0.15 structural
/// ratio (2:1.33:1). Freshness dimension removed in crt-048 (see GH #520).
pub const DEFAULT_WEIGHTS: CoherenceWeights = CoherenceWeights {
    graph_quality: 0.46,
    embedding_consistency: 0.23,
    contradiction_density: 0.31,
};
```

Action: Remove `confidence_freshness: 0.35` line. Update `graph_quality`, `embedding_consistency`,
`contradiction_density` to new literals. Update doc comment reference from ADR-003 to ADR-001.

Weight values are LOCKED — do not re-derive. The exact sum 0.46 + 0.31 + 0.23 = 1.00 is
exactly representable in IEEE 754 but the test uses epsilon guard per NFR-04.

---

## Constant: DEFAULT_STALENESS_THRESHOLD_SECS (RETAINED)

### Current state (line 11)
```rust
/// Default staleness threshold for confidence freshness: 24 hours in seconds.
pub const DEFAULT_STALENESS_THRESHOLD_SECS: u64 = 24 * 3600;
```

### Post-crt-048 state (AC-11, ADR-002 — hard constraint, must not be removed)
```rust
/// Staleness threshold for confidence refresh: 24 hours in seconds.
///
/// Used by run_maintenance() in services/status.rs to identify entries eligible
/// for confidence score re-computation. NOT a Lambda input — the Lambda freshness
/// dimension was removed in crt-048.
pub const DEFAULT_STALENESS_THRESHOLD_SECS: u64 = 24 * 3600;
```

Action: Update doc comment only. The constant value and visibility are unchanged.
The surviving caller is `services/status.rs` `run_maintenance()` at ~line 1242.

---

## Function: compute_lambda (UPDATED SIGNATURE)

### Current signature (5 parameters)
```rust
pub fn compute_lambda(
    freshness: f64,
    graph_quality: f64,
    embedding_consistency: Option<f64>,
    contradiction_density: f64,
    weights: &CoherenceWeights,
) -> f64
```

### Post-crt-048 signature (4 parameters — AC-05, FR-05)
```rust
pub fn compute_lambda(
    graph_quality: f64,
    embedding_consistency: Option<f64>,
    contradiction_density: f64,
    weights: &CoherenceWeights,
) -> f64
```

### Updated body pseudocode

```
FUNCTION compute_lambda(graph_quality, embedding_consistency, contradiction_density, weights):
    MATCH embedding_consistency:
        Some(embed_score):
            lambda = weights.graph_quality * graph_quality
                   + weights.embedding_consistency * embed_score
                   + weights.contradiction_density * contradiction_density
            RETURN lambda.clamp(0.0, 1.0)

        None:
            // Re-normalize over 2 remaining dimensions
            // With DEFAULT_WEIGHTS: remaining = 0.46 + 0.31 = 0.77
            // graph effective weight:         0.46 / 0.77 ≈ 0.5974
            // contradiction effective weight: 0.31 / 0.77 ≈ 0.4026
            remaining = weights.graph_quality + weights.contradiction_density
            IF remaining <= 0.0:
                RETURN 1.0
            lambda = (weights.graph_quality * graph_quality
                    + weights.contradiction_density * contradiction_density)
                   / remaining
            RETURN lambda.clamp(0.0, 1.0)
```

Action: Remove the `freshness: f64` first parameter and all references to
`weights.confidence_freshness * freshness` from both arms. The re-normalization
formula is structurally unchanged — only the weight field references change.

Note on argument ordering (R-01 risk): The four remaining parameters have types
`f64`, `Option<f64>`, `f64`, `&CoherenceWeights`. The two bare `f64` params
(`graph_quality` first, `contradiction_density` third) must land at the correct
positions. Verify each call site at `services/status.rs` semantically, not just
by arity.

---

## Function: generate_recommendations (UPDATED SIGNATURE)

### Current signature (7 parameters)
```rust
pub fn generate_recommendations(
    lambda: f64,
    threshold: f64,
    stale_confidence_count: u64,
    oldest_stale_age_secs: u64,
    graph_stale_ratio: f64,
    embedding_inconsistent_count: usize,
    total_quarantined: u64,
) -> Vec<String>
```

### Post-crt-048 signature (5 parameters — AC-09, FR-06)
```rust
pub fn generate_recommendations(
    lambda: f64,
    threshold: f64,
    graph_stale_ratio: f64,
    embedding_inconsistent_count: usize,
    total_quarantined: u64,
) -> Vec<String>
```

### Updated body pseudocode

```
FUNCTION generate_recommendations(lambda, threshold, graph_stale_ratio,
                                  embedding_inconsistent_count, total_quarantined):
    IF lambda >= threshold:
        RETURN []

    recs = []

    // [DELETED] stale-confidence branch:
    //   IF stale_confidence_count > 0:
    //       days = oldest_stale_age_secs / 86400
    //       recs.push("{stale_count} entries have stale confidence ...")
    // This entire block is removed. No replacement.

    IF graph_stale_ratio > DEFAULT_STALE_RATIO_TRIGGER:
        pct = (graph_stale_ratio * 100.0) as u64
        recs.push("{pct}% stale nodes -- background maintenance will compact automatically")

    IF embedding_inconsistent_count > 0:
        recs.push("{embedding_inconsistent_count} embedding inconsistencies detected")

    IF total_quarantined > 0:
        recs.push("{total_quarantined} entries quarantined -- review for resolution")

    RETURN recs
```

Action: Remove `stale_confidence_count: u64` and `oldest_stale_age_secs: u64` from the
parameter list and delete the stale-confidence `if` branch and its format string. The
three remaining recommendation branches (graph stale ratio, embedding inconsistencies,
quarantined entries) are unchanged.

Note on existing recommendation strings: The string for the stale-confidence branch
referenced in `make_coherence_status_report()` fixture at line 1444 must also be removed
from that fixture (see Component D). That fixture's `maintenance_recommendations` vec
currently contains two entries — the first references "stale confidence" and must be
removed; the second references "HNSW graph" and is retained.

---

## Functions: DELETED ENTIRELY

### confidence_freshness_score (lines 44-68) — DELETE (AC-03, FR-03)

```
// DELETED — no replacement, no dead-code stub
fn confidence_freshness_score(
    entries: &[EntryRecord],
    now: u64,
    staleness_threshold_secs: u64,
) -> (f64, u64)
```

Action: Delete the entire function including its doc comment. After deletion,
`grep -r "confidence_freshness_score" crates/` must return zero matches.

### oldest_stale_age (lines 145-161) — DELETE (AC-04, FR-04)

```
// DELETED — no replacement, no dead-code stub
fn oldest_stale_age(
    entries: &[EntryRecord],
    now: u64,
    staleness_threshold_secs: u64,
) -> u64
```

Action: Delete the entire function including its doc comment. After deletion,
`grep -r "oldest_stale_age" crates/` must return zero matches.

---

## Retained Functions (Unchanged)

These functions are not modified:
- `graph_quality_score(stale_count: usize, point_count: usize) -> f64`
- `embedding_consistency_score(inconsistent_count: usize, total_checked: usize) -> f64`
- `contradiction_density_score(total_quarantined: u64, total_active: u64) -> f64`

Their signatures, bodies, and tests are untouched.

---

## Test Module Changes

### Tests DELETED (~11 tests, FR-15)

Delete these entire `#[test]` functions:

1. `freshness_empty_entries` (line ~250)
2. `freshness_all_stale` (line ~256)
3. `freshness_none_stale` (line ~270)
4. `freshness_uses_max_of_timestamps` (line ~283)
5. `freshness_recently_accessed_not_stale` (line ~464)
6. `freshness_both_timestamps_older_than_threshold` (line ~477)
7. `oldest_stale_no_stale` (line ~402)
8. `oldest_stale_one_stale` (line ~409)
9. `oldest_stale_both_timestamps_zero` (line ~416)
10. `staleness_threshold_constant_value` (line ~584)
11. `recommendations_below_threshold_stale_confidence` (line ~439)

Also delete the `make_entry_with_timestamps()` helper function if it is no longer
referenced by any retained test. Check: the helper is currently used by tests 1-9 and
freshness tests. After deleting those, verify whether any retained test still needs it.
If no retained test calls `make_entry_with_timestamps`, delete the helper too.

### Tests UPDATED (value/signature changes only, FR-16)

Each updated test removes the `freshness` positional argument and updates expected values.
The existing test names are preserved unless noted.

**lambda_all_ones**
```
BEFORE: compute_lambda(1.0, 1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS)
AFTER:  compute_lambda(1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS)
expected: (lambda - 1.0).abs() < 0.001  [unchanged]
```

**lambda_all_zeros**
```
BEFORE: compute_lambda(0.0, 0.0, Some(0.0), 0.0, &DEFAULT_WEIGHTS)
AFTER:  compute_lambda(0.0, Some(0.0), 0.0, &DEFAULT_WEIGHTS)
expected: lambda == 0.0  [unchanged]
```

**lambda_weighted_sum**
```
BEFORE: compute_lambda(0.5, 0.5, Some(0.5), 0.5, &DEFAULT_WEIGHTS)
        comment: "0.35*0.5 + 0.30*0.5 + 0.15*0.5 + 0.20*0.5 = 0.5"
AFTER:  compute_lambda(0.5, Some(0.5), 0.5, &DEFAULT_WEIGHTS)
        // 0.46*0.5 + 0.23*0.5 + 0.31*0.5 = 0.5
expected: (lambda - 0.5).abs() < 0.001  [unchanged — uniform 0.5 still sums to 0.5]
```

**lambda_specific_four_dimensions → RENAME to lambda_specific_three_dimensions**
```
BEFORE: compute_lambda(0.9, 0.8, Some(1.0), 0.7, &DEFAULT_WEIGHTS)
        // 0.35*0.9 + 0.30*0.8 + 0.15*1.0 + 0.20*0.7 = 0.845
AFTER:  compute_lambda(0.8, Some(1.0), 0.7, &DEFAULT_WEIGHTS)
        // graph=0.8, embed=1.0, contradiction=0.7
        // 0.46*0.8 + 0.23*1.0 + 0.31*0.7 = 0.368 + 0.23 + 0.217 = 0.815
expected: (lambda - 0.815).abs() < 0.001
```

**lambda_single_dimension_deviation**
```
BEFORE: compute_lambda(0.5, 1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS)
        // 0.35*0.5 + 0.30*1.0 + 0.15*1.0 + 0.20*1.0 = 0.825
AFTER:  compute_lambda(0.5, Some(1.0), 1.0, &DEFAULT_WEIGHTS)
        // graph=0.5, embed=1.0, contradiction=1.0
        // 0.46*0.5 + 0.23*1.0 + 0.31*1.0 = 0.23 + 0.23 + 0.31 = 0.77
expected: (lambda - 0.77).abs() < 0.001
```

**lambda_weight_sum_invariant (NFR-04 — epsilon guard mandatory)**
```
BEFORE:
  total = DEFAULT_WEIGHTS.confidence_freshness
        + DEFAULT_WEIGHTS.graph_quality
        + DEFAULT_WEIGHTS.embedding_consistency
        + DEFAULT_WEIGHTS.contradiction_density;
  assert!((total - 1.0).abs() < 0.001, ...)

AFTER:
  total = DEFAULT_WEIGHTS.graph_quality
        + DEFAULT_WEIGHTS.embedding_consistency
        + DEFAULT_WEIGHTS.contradiction_density;
  // Must use f64::EPSILON per NFR-04, not exact ==
  assert!((total - 1.0_f64).abs() < f64::EPSILON,
          "weight sum should be 1.0, got {total}")
```

Note: The sum 0.46 + 0.23 + 0.31 = 1.00 is exactly representable in IEEE 754,
but epsilon guard is mandatory per ADR-001 and NFR-04 for robustness.

**lambda_renormalization_without_embedding**
```
BEFORE: compute_lambda(1.0, 1.0, None, 1.0, &DEFAULT_WEIGHTS)
        // remaining = 0.35 + 0.30 + 0.20 = 0.85
        // lambda = 0.85/0.85 = 1.0
AFTER:  compute_lambda(1.0, None, 1.0, &DEFAULT_WEIGHTS)
        // remaining = 0.46 + 0.31 = 0.77
        // lambda = (0.46*1.0 + 0.31*1.0) / 0.77 = 0.77/0.77 = 1.0
expected: (lambda - 1.0).abs() < 0.001  [unchanged — all-1.0 still gives 1.0]
```

**lambda_renormalization_partial**
```
BEFORE: compute_lambda(0.5, 0.5, None, 0.5, &DEFAULT_WEIGHTS)
        // remaining = 0.85, weighted_sum = 0.85*0.5 = 0.425
        // lambda = 0.425/0.85 = 0.5
AFTER:  compute_lambda(0.5, None, 0.5, &DEFAULT_WEIGHTS)
        // remaining = 0.77, weighted_sum = (0.46*0.5 + 0.31*0.5) = 0.385
        // lambda = 0.385/0.77 = 0.5
expected: (lambda - 0.5).abs() < 0.001  [unchanged — uniform 0.5 still gives 0.5]
```

**lambda_renormalized_weights_sum_to_one**
```
BEFORE:
  remaining = DEFAULT_WEIGHTS.confidence_freshness
            + DEFAULT_WEIGHTS.graph_quality
            + DEFAULT_WEIGHTS.contradiction_density;
  w_freshness    = DEFAULT_WEIGHTS.confidence_freshness / remaining;
  w_graph        = DEFAULT_WEIGHTS.graph_quality / remaining;
  w_contradiction = DEFAULT_WEIGHTS.contradiction_density / remaining;
  sum = w_freshness + w_graph + w_contradiction;

AFTER:
  remaining = DEFAULT_WEIGHTS.graph_quality + DEFAULT_WEIGHTS.contradiction_density;
  // remaining = 0.46 + 0.31 = 0.77
  w_graph         = DEFAULT_WEIGHTS.graph_quality / remaining;         // 0.46/0.77
  w_contradiction = DEFAULT_WEIGHTS.contradiction_density / remaining; // 0.31/0.77
  sum = w_graph + w_contradiction;
  assert!((sum - 1.0).abs() < f64::EPSILON * 10.0, ...)
```

**lambda_embedding_excluded_specific**
```
BEFORE: compute_lambda(0.9, 0.8, None, 0.7, &DEFAULT_WEIGHTS)
        // remaining = 0.85
        // weighted_sum = 0.35*0.9 + 0.30*0.8 + 0.20*0.7 = 0.315 + 0.24 + 0.14 = 0.695
        // lambda = 0.695 / 0.85 = 0.81765
AFTER:  compute_lambda(0.8, None, 0.7, &DEFAULT_WEIGHTS)
        // graph=0.8, contradiction=0.7
        // remaining = 0.46 + 0.31 = 0.77
        // weighted_sum = 0.46*0.8 + 0.31*0.7 = 0.368 + 0.217 = 0.585
        // lambda = 0.585 / 0.77 = 0.75974...
expected: (lambda - 0.75974).abs() < 0.001
```

**lambda_custom_weights_zero_embedding**
```
BEFORE:
  weights = CoherenceWeights {
      confidence_freshness: 0.5,
      graph_quality: 0.3,
      embedding_consistency: 0.0,
      contradiction_density: 0.2,
  };
  compute_lambda(0.8, 0.6, None, 0.4, &weights)

AFTER (remove confidence_freshness from struct literal):
  weights = CoherenceWeights {
      graph_quality: 0.3,
      embedding_consistency: 0.0,
      contradiction_density: 0.2,
  };
  // Note: weights.graph_quality + weights.contradiction_density = 0.3 + 0.2 = 0.5
  // weighted_sum = 0.3*0.8 + 0.2*0.4 = 0.24 + 0.08 = 0.32
  // lambda = 0.32 / 0.5 = 0.64
  compute_lambda(0.6, None, 0.4, &weights)
  // graph=0.6 (was 0.8 when freshness was 0.8), contradiction=0.4
  // Wait — re-derive: graph=0.6, contradiction=0.4
  // weighted_sum = 0.3*0.6 + 0.2*0.4 = 0.18 + 0.08 = 0.26
  // lambda = 0.26 / 0.5 = 0.52
expected: (lambda - 0.52).abs() < 0.001
```

Note on lambda_custom_weights_zero_embedding argument alignment: The original test
passes `(0.8, 0.6, None, 0.4, &weights)` where 0.8=freshness, 0.6=graph, 0.4=contradiction.
After removing freshness, the call becomes `(0.6, None, 0.4, &weights)` where 0.6=graph,
0.4=contradiction. The expected value changes accordingly.

### Tests retained unchanged

The following tests do not reference freshness and require no changes:
- `graph_quality_zero_points`, `graph_quality_no_stale`, `graph_quality_stale_exceeds_total_clamped`,
  `graph_quality_half_stale`
- `embedding_consistency_zero_checked`, `embedding_consistency_all_inconsistent`,
  `embedding_consistency_none_inconsistent`, `embedding_consistency_single_entry_consistent`,
  `embedding_consistency_single_entry_inconsistent`
- `contradiction_density_zero_active`, `contradiction_density_quarantined_exceeds_active`,
  `contradiction_density_no_quarantined`
- `recommendations_above_threshold_empty`, `recommendations_at_threshold_empty`
- `recommendations_below_threshold_high_stale_ratio`
- `recommendations_below_threshold_all_issues` — UPDATE: currently passes 7 args, now 5;
  remove stale_confidence_count and oldest_stale_age_secs; verify the new 4-recommendation
  count becomes 3 (the stale-confidence branch no longer fires) or keep at expected-3
  if original test asserts 4 and that included the freshness recommendation.

**recommendations_below_threshold_all_issues — requires audit**

Current test: `generate_recommendations(0.3, 0.8, 5, 86400, 0.15, 3, 2)` asserts `recs.len() == 4`.
The 4 recommendations were: stale-confidence, graph-ratio, embedding, quarantined.
After removing the stale-confidence branch, the same inputs yield 3 recommendations.

Post-crt-048 call: `generate_recommendations(0.3, 0.8, 0.15, 3, 2)` — recs.len() == 3.

Also update `recommendations_below_threshold_embedding_inconsistencies` and
`recommendations_below_threshold_quarantined`: these currently call with 7 args; update
to 5 args removing `stale_confidence_count` (0) and `oldest_stale_age_secs` (0). The
expected assertions are unchanged (these tests already pass 0 for the now-removed params).

- `test_max_confidence_refresh_batch_is_500` — unchanged

---

## Error Handling

`compute_lambda()` and `generate_recommendations()` are pure functions with no I/O and
no error returns. The only defensive path is the `remaining <= 0.0` guard in `compute_lambda()`
(returns 1.0 if re-normalization denominator is zero — preserves the crt-005 behavior).

---

## Key Test Scenarios

1. `compute_lambda(1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS) == 1.0` — AC-07
2. `compute_lambda(0.0, Some(0.0), 0.0, &DEFAULT_WEIGHTS) == 0.0` — all-zero edge case
3. `compute_lambda(1.0, None, 1.0, &DEFAULT_WEIGHTS) == 1.0` — AC-08, embedding absent
4. `compute_lambda(0.8, None, 0.6, &DEFAULT_WEIGHTS) ≈ 0.8*(0.46/0.77) + 0.6*(0.31/0.77)` — R-07
5. Three-dimension specific value test with distinct per-dimension inputs — R-01
6. Lambda weight sum invariant using epsilon — NFR-04, R-04
7. `generate_recommendations()` with all-zero stale inputs returns max 3 recs — confirms branch deleted
