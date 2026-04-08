# Component: infra/coherence.rs
# crt-051 Pseudocode

## Purpose

`coherence.rs` contains all pure Lambda scoring functions. This feature changes one of
those functions: `contradiction_density_score()`. The change corrects the input from a
quarantine-status counter (which has no relationship to contradictions) to the raw count
of contradiction pairs detected by the background heuristic scan.

No other functions in this file change. `generate_recommendations()` is verified
unchanged — it is spatially close and takes `total_quarantined` as a parameter; its
signature and body must not be altered.

---

## New/Modified Function: contradiction_density_score()

### Location

`crates/unimatrix-server/src/infra/coherence.rs` — approximately line 64.

### Old Signature (to be removed)

```
pub fn contradiction_density_score(total_quarantined: u64, total_active: u64) -> f64
```

### New Signature (to be implemented)

```
pub fn contradiction_density_score(
    contradiction_pair_count: usize,
    total_active: u64,
) -> f64
```

### Doc Comment (full replacement — must replace old quarantine-based comment)

```
/// Contradiction density dimension: complement of contradiction pair ratio.
///
/// Returns 1.0 if `total_active` is zero (empty database guard).
/// Returns 1.0 if `contradiction_pair_count` is zero (cold-start or no contradictions
/// detected — optimistic default until the scan produces evidence).
/// Score is `1.0 - contradiction_pair_count / total_active`, clamped to [0.0, 1.0].
/// When `contradiction_pair_count > total_active` (degenerate: many pairs from a
/// small active set), the clamp produces 0.0.
///
/// `contradiction_pair_count` comes from `ContradictionScanCacheHandle` read in Phase 2
/// of `compute_report()`. It reflects detected contradiction pairs from the background
/// heuristic scan (HNSW nearest-neighbour + negation/directive/sentiment signals).
/// The cache is rebuilt approximately every 60 minutes. A stale cache is a known
/// limitation (SR-07); this function is not responsible for cache freshness.
```

### Function Body

```
FUNCTION contradiction_density_score(contradiction_pair_count: usize, total_active: u64) -> f64:

    IF total_active == 0:
        RETURN 1.0          // empty-database guard; also covers contradiction_pair_count == 0
    END IF

    // Both zero-pairs (cold-start / no contradictions) and degenerate cases fall through
    // to the formula. When contradiction_pair_count == 0, formula produces 1.0 - 0.0 = 1.0.
    // When contradiction_pair_count > total_active, formula produces < 0.0, clamped to 0.0.

    LET score = 1.0 - (contradiction_pair_count AS f64 / total_active AS f64)
    RETURN score.clamp(0.0, 1.0)

END FUNCTION
```

### Implementation Notes

- The `as f64` cast on `contradiction_pair_count: usize` is safe. On 64-bit targets
  `usize` is 64-bit. At expected knowledge base scales (thousands of entries, tens of
  pairs), f64 precision is not at risk. This follows the existing pattern in
  `graph_quality_score()` where `stale_count as f64` is used.
- The body is structurally identical to the old function. Only the parameter name,
  parameter type (`u64` -> `usize`), the `AS f64` cast source, and the doc comment change.
- The empty-database guard returns 1.0 before any division. This covers the degenerate
  input `(0, 0)` without requiring a separate early return for zero pair count.
- Cold-start: when `contradiction_pair_count == 0` and `total_active > 0`, the formula
  computes `1.0 - 0.0 = 1.0`. No separate guard is needed.

---

## Unchanged Function: generate_recommendations() (verification required)

### Location

`crates/unimatrix-server/src/infra/coherence.rs` — approximately line 114.

### Signature (must remain exactly this)

```
pub fn generate_recommendations(
    lambda: f64,
    threshold: f64,
    graph_stale_ratio: f64,
    embedding_inconsistent_count: usize,
    total_quarantined: u64,
) -> Vec<String>
```

`total_quarantined: u64` is the fifth parameter. It must remain. This function governs
quarantine management recommendations — a separate concern from Lambda scoring. AC-08
requires it unchanged; AC-09's grep must be scoped to not flag this parameter.

---

## Unit Tests: Full Rewrite Required

### Existing Tests to Rewrite (3 tests)

These tests encode quarantine semantics in their names. Rename and update comments.
Numeric values may happen to be identical (the formula structure is unchanged) but the
semantic meaning of the first argument changes from quarantine count to pair count.

#### Test 1 — zero_active guard

Old name: `contradiction_density_zero_active`
New name: `contradiction_density_zero_active` (acceptable to keep — "zero active" is
still the correct description of this test's scenario)

```
#[test]
fn contradiction_density_zero_active():
    // Empty database: any pair count with zero active entries returns 1.0.
    ASSERT contradiction_density_score(0, 0) == 1.0
```

Note: the argument `0` for `contradiction_pair_count` is now typed as `usize`, not `u64`.
Untyped literal `0` is fine; Rust infers `usize` from the function signature. Do not
annotate the literal as `0_u64`.

#### Test 2 — pair count exceeds active (clamped to 0.0)

Old name: `contradiction_density_quarantined_exceeds_active` (MUST be removed; contains
"quarantined")
New name: `contradiction_density_pairs_exceed_active`

```
#[test]
fn contradiction_density_pairs_exceed_active():
    // Degenerate: more detected pairs than active entries — clamped to 0.0.
    ASSERT contradiction_density_score(200, 100) == 0.0
```

Arguments `(200, 100)` are numerically unchanged. The first argument now means
"200 detected contradiction pairs", not "200 quarantined entries". Update any inline
comment or doc to reflect this.

#### Test 3 — zero pairs with active entries

Old name: `contradiction_density_no_quarantined` (MUST be removed; contains "quarantined")
New name: `contradiction_density_no_pairs`

```
#[test]
fn contradiction_density_no_pairs():
    // No detected pairs with active entries: maximum health score.
    ASSERT contradiction_density_score(0, 100) == 1.0
```

### New Tests to Add (2 tests)

#### New Test 4 — cold-start scenario (AC-17)

Name: `contradiction_density_cold_start`

This test is distinct from `contradiction_density_zero_active` (which tests `total_active
== 0`). This test models the period after server start but before the first contradiction
scan completes: active entries exist, no pairs are known yet.

```
#[test]
fn contradiction_density_cold_start():
    // Cold-start: scan not yet run. Active entries exist but no pairs detected.
    // Optimistic default: score 1.0 until scan provides evidence.
    LET result = contradiction_density_score(0, 50)
    ASSERT result == 1.0
```

Tolerance: use `assert_eq!` (exact) or `assert!((result - 1.0).abs() < 1e-10)`. Either
is acceptable — the value is exact.

#### New Test 5 — partial contradiction density (AC-05)

Name: `contradiction_density_partial`

```
#[test]
fn contradiction_density_partial():
    // Mid-range: 5 pairs in a 100-entry database.
    // Expected: 1.0 - 5/100 = 0.95
    LET result = contradiction_density_score(5, 100)
    ASSERT result > 0.0
    ASSERT result < 1.0
    ASSERT (result - 0.95).abs() < 1e-10
```

The `1e-10` epsilon is required by NFR-04 for exact calculations. `5.0 / 100.0` is
representable in f64 without rounding error, making this an exact assertion.

---

## Test Order in the File

After the rewrite, the five tests under `// -- contradiction_density_score tests --`
should appear in this order:

1. `contradiction_density_zero_active` — empty-database guard (AC-03)
2. `contradiction_density_no_pairs` — zero pairs, active entries (AC-02)
3. `contradiction_density_cold_start` — cold-start scenario (AC-17)
4. `contradiction_density_pairs_exceed_active` — clamped at 0.0 (SR-01)
5. `contradiction_density_partial` — mid-range formula verification (AC-05)

---

## Error Handling

`contradiction_density_score()` is a pure function over two numeric inputs. It cannot
fail. No `Result` return type. No panics (no division — `total_active == 0` is guarded).

---

## Key Test Scenarios Summary

| Scenario | Call | Expected | AC/Risk |
|---|---|---|---|
| Empty database | `score(0, 0)` | `1.0` | AC-03 |
| Zero pairs, active entries | `score(0, 100)` | `1.0` | AC-02 |
| Cold-start | `score(0, 50)` | `1.0` | AC-17, R-05 |
| Pairs exceed active | `score(200, 100)` | `0.0` | SR-01, R-07 |
| Mid-range | `score(5, 100)` | `0.95 ± 1e-10` | AC-05, R-07 |
| Test names have no "quarantined" | — | static check | AC-14, R-03 |
| First arg type is usize not u64 | — | static check | R-03 |
