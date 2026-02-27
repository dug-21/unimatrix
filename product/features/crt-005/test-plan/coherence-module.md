# Test Plan: C4 Coherence Module

## Component

C4: Coherence Module (`crates/unimatrix-server/src/coherence.rs`)

## Risks Covered

| Risk | Description | Priority |
|------|-------------|----------|
| R-05 | Lambda weight re-normalization edge cases | High |
| R-10 | Dimension score boundary values | High |
| R-14 | Weight sum invariant (coherence weights) | High |
| R-16 | Staleness detection false positives | Med |
| R-20 | Recommendation generation correctness | Low |

## Unit Tests: Dimension Scores (R-10)

### UT-C4-01: confidence_freshness_score -- empty entries
- Input: empty slice, any now, any threshold
- Assert: score == 1.0, stale_count == 0
- Covers: R-10 scenario 1

### UT-C4-02: confidence_freshness_score -- all entries stale
- Input: 5 entries all with reference timestamps older than threshold
- Assert: score == 0.0, stale_count == 5
- Covers: R-10 scenario 2

### UT-C4-03: confidence_freshness_score -- no entries stale
- Input: 5 entries all with recent timestamps (within threshold)
- Assert: score == 1.0, stale_count == 0
- Covers: R-10 scenario 3

### UT-C4-04: confidence_freshness_score -- uses max(updated_at, last_accessed_at)
- Input: entry with updated_at=old, last_accessed_at=recent
- Assert: entry is NOT stale (last_accessed_at is more recent)
- Input: entry with updated_at=recent, last_accessed_at=old
- Assert: entry is NOT stale (updated_at is more recent)
- Covers: R-16 scenario 2

### UT-C4-05: confidence_freshness_score -- both timestamps zero
- Input: entry with updated_at=0, last_accessed_at=0
- Assert: entry IS stale
- Covers: R-16 scenario 4

### UT-C4-06: confidence_freshness_score -- recently accessed not stale
- Input: entry with last_accessed_at = now - 1 hour (3600 secs)
- Threshold: 86400 secs (24 hours)
- Assert: entry is NOT stale
- Covers: R-16 scenario 1

### UT-C4-07: confidence_freshness_score -- both timestamps older than threshold
- Input: entry with updated_at = now - 48h, last_accessed_at = now - 36h
- Threshold: 86400 secs
- Assert: entry IS stale
- Covers: R-16 scenario 3

### UT-C4-08: graph_quality_score -- zero point_count
- Input: stale_count=0, point_count=0
- Assert: score == 1.0 (no division by zero)
- Covers: R-10 scenario 4

### UT-C4-09: graph_quality_score -- stale_count > point_count
- Input: stale_count=10, point_count=5
- Assert: score clamped to 0.0
- Covers: R-10 scenario 5

### UT-C4-10: graph_quality_score -- zero stale
- Input: stale_count=0, point_count=100
- Assert: score == 1.0
- Covers: R-10 scenario 6

### UT-C4-11: graph_quality_score -- mid-range
- Input: stale_count=20, point_count=100
- Assert: score == 0.8, within [0.0, 1.0]
- Covers: R-10 scenario 11

### UT-C4-12: embedding_consistency_score -- zero checked
- Input: inconsistent=0, total_checked=0
- Assert: score == 1.0
- Covers: R-10 scenario 7

### UT-C4-13: embedding_consistency_score -- all inconsistent
- Input: inconsistent=10, total_checked=10
- Assert: score == 0.0
- Covers: R-10 scenario 8

### UT-C4-14: embedding_consistency_score -- single entry checked
- Input: inconsistent=0, total_checked=1 -> score == 1.0
- Input: inconsistent=1, total_checked=1 -> score == 0.0
- Covers: EC-08

### UT-C4-15: contradiction_density_score -- zero active
- Input: quarantined=0, active=0
- Assert: score == 1.0
- Covers: R-10 scenario 9

### UT-C4-16: contradiction_density_score -- quarantined > active
- Input: quarantined=10, active=5
- Assert: score clamped to 0.0
- Covers: R-10 scenario 10

## Unit Tests: Lambda Computation (R-05)

### UT-C4-17: Lambda with all four dimensions
- Input: freshness=0.9, graph=0.8, embed=Some(1.0), contradiction=0.7
- Weights: DEFAULT_WEIGHTS
- Expected: 0.35*0.9 + 0.30*0.8 + 0.15*1.0 + 0.20*0.7 = 0.315 + 0.24 + 0.15 + 0.14 = 0.845
- Covers: R-05 scenario 1

### UT-C4-18: Lambda with embedding excluded (None)
- Input: freshness=0.9, graph=0.8, embed=None, contradiction=0.7
- Remaining weight sum: 0.35 + 0.30 + 0.20 = 0.85
- Expected: (0.35*0.9 + 0.30*0.8 + 0.20*0.7) / 0.85 = 0.695 / 0.85 = 0.81765...
- Covers: R-05 scenario 2

### UT-C4-19: Lambda re-normalized weights verify
- When embedding excluded, verify effective weights:
  - freshness: 0.35/0.85 ~= 0.4118
  - graph: 0.30/0.85 ~= 0.3529
  - contradiction: 0.20/0.85 ~= 0.2353
- Assert these sum to 1.0 (within f64::EPSILON)
- Covers: R-05 scenario 6

### UT-C4-20: Lambda all dimensions 1.0
- All scores = 1.0, with and without embedding dimension
- Assert: lambda == 1.0 in both cases
- Covers: R-05 scenario 3

### UT-C4-21: Lambda all dimensions 0.0
- All scores = 0.0, with and without embedding dimension
- Assert: lambda == 0.0 in both cases
- Covers: R-05 scenario 4

### UT-C4-22: Lambda single dimension deviation
- freshness=0.5, all others=1.0
- Assert: lambda = 0.35*0.5 + 0.30*1.0 + 0.15*1.0 + 0.20*1.0 = 0.175 + 0.65 = 0.825
- Covers: R-05 scenario 5

### UT-C4-23: Lambda custom weights with zero embedding weight
- CoherenceWeights with embedding_consistency=0.0
- embed=None
- Remaining = freshness_w + graph_w + contra_w
- Assert: no division by zero, lambda computed correctly
- Covers: R-05 scenario 7

## Unit Tests: Weight Invariants (R-14)

### UT-C4-24: DEFAULT_WEIGHTS sum to 1.0
- Assert: 0.35 + 0.30 + 0.15 + 0.20 == 1.0 exactly
- Covers: R-14 scenario 4

### UT-C4-25: DEFAULT_WEIGHTS sum invariant (compile-time guard)
- Const assertion or test that weights sum to 1.0
- Covers: R-05 scenario 8

## Unit Tests: Recommendations (R-20)

### UT-C4-26: Lambda >= threshold produces empty recommendations
- lambda=0.85, threshold=0.8
- Assert: empty Vec
- Covers: R-20 scenario 1

### UT-C4-27: Lambda exactly at threshold (0.8)
- lambda=0.8, threshold=0.8
- Assert: empty Vec (strict less-than comparison)
- Covers: EC-04

### UT-C4-28: Lambda < threshold with stale confidence
- lambda=0.7, stale_confidence_count=5, oldest_stale_age=172800 (2 days)
- Assert: recommendation mentions stale count and days
- Covers: R-20 scenario 2

### UT-C4-29: Lambda < threshold with high stale ratio
- lambda=0.7, graph_stale_ratio=0.25
- Assert: recommendation mentions stale node percentage
- Covers: R-20 scenario 3

### UT-C4-30: Lambda < threshold with embedding inconsistencies
- lambda=0.7, embedding_inconsistent_count=3
- Assert: recommendation mentions inconsistency count
- Covers: R-20 scenario 4

### UT-C4-31: Lambda < threshold with quarantined entries
- lambda=0.7, total_quarantined=5
- Assert: recommendation mentions quarantine count
- Covers: R-20 scenario 5

### UT-C4-32: All dimensions degraded produces multiple recommendations
- lambda=0.5, all degradation inputs non-zero
- Assert: Vec has 4 recommendations (one per dimension)
- Covers: R-20 scenario 6

## Unit Tests: Staleness Helper

### UT-C4-33: oldest_stale_age returns correct age
- 3 entries: ages 1 day, 3 days, 5 days (all stale)
- Assert: oldest_stale_age returns 5 days in seconds

### UT-C4-34: oldest_stale_age with no stale entries
- All entries recent
- Assert: returns 0

### UT-C4-35: Staleness threshold is named constant
- Assert: DEFAULT_STALENESS_THRESHOLD_SECS == 86400
- Covers: R-16 scenario 5

## Dependencies

- None (coherence.rs is a new module with pure functions, no external dependencies)

## Estimated Test Count

- 35 unit tests (all pure functions, no I/O)
