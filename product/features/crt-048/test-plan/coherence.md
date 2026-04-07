# Test Plan: `infra/coherence.rs` — Coherence Computation

## Component Scope

Pure-function math layer. No I/O, no database, no async. All tests are
synchronous unit tests. Test file: `crates/unimatrix-server/src/infra/coherence.rs`
(inline `#[cfg(test)]` module).

---

## Risks Owned by This Component

| Risk | Coverage Requirement |
|------|---------------------|
| R-01 | Distinct-value inputs to detect positional transposition |
| R-03 | `DEFAULT_STALENESS_THRESHOLD_SECS` retained with updated comment |
| R-04 | `lambda_weight_sum_invariant` uses epsilon, not exact `==` |
| R-07 | Re-normalization expected values re-derived from new 3-dim weights |
| R-09 | Build gate (deleted recommendation branch compiles cleanly) |

---

## Unit Test Expectations

### Struct Correctness

**`CoherenceWeights` has exactly three fields**

The struct definition must compile with only `graph_quality`, `contradiction_density`,
`embedding_consistency`. Any struct literal that includes `confidence_freshness` produces
a compile error — this is the primary safety net for R-01 and related risks.

Assert: `cargo build --workspace` succeeds (compile-time proof of field count).

---

### `lambda_weight_sum_invariant` (R-04, AC-02)

**Purpose:** Guard that `DEFAULT_WEIGHTS` sums to 1.0 within f64 epsilon.

**Arrangement:**
```rust
let sum = DEFAULT_WEIGHTS.graph_quality
    + DEFAULT_WEIGHTS.contradiction_density
    + DEFAULT_WEIGHTS.embedding_consistency;
```

**Assertion — MUST use:**
```rust
assert!((sum - 1.0_f64).abs() < f64::EPSILON,
    "DEFAULT_WEIGHTS sum deviates from 1.0 by {}", (sum - 1.0_f64).abs());
```

**Forbidden form:** `assert_eq!(sum, 1.0_f64)` — exact comparison is banned per NFR-04
even though 0.46 + 0.31 + 0.23 happens to be exactly representable; the epsilon guard
is a robustness requirement, not a compensation for inexactness.

References struct constants directly, not inline float literals. A weight value change
is automatically caught without updating the test body.

---

### `lambda_all_ones` (AC-07)

**Input:** `compute_lambda(1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS)`

**Expected output:** `1.0`

**Why:** When all three dimensions are 1.0, the weighted sum is
`1.0 * 0.46 + 1.0 * 0.31 + 1.0 * 0.23 = 1.00`. Verifies the 3-dimension
signature is correct and the weights sum to 1.0 in practice.

---

### `lambda_all_zeros` (sanity)

**Input:** `compute_lambda(0.0, Some(0.0), 0.0, &DEFAULT_WEIGHTS)`

**Expected output:** `0.0`

**Why:** Boundary case. With all 3-dimension inputs, the result must be 0.0.

---

### `lambda_specific_three_dimensions` (R-01, RENAMED from `lambda_specific_four_dimensions`)

**Purpose:** Detect positional transposition. Uses distinct values for all three
dimension slots so that any two-argument swap produces a detectably different result.

**Input:**
```rust
compute_lambda(
    0.8,       // graph_quality
    Some(0.5), // embedding_consistency
    0.3,       // contradiction_density
    &DEFAULT_WEIGHTS,
)
```

**Expected output (hand-derived):**
```
0.8 * 0.46 + 0.5 * 0.23 + 0.3 * 0.31
= 0.368 + 0.115 + 0.093
= 0.576
```

**Assertion:**
```rust
let result = compute_lambda(0.8, Some(0.5), 0.3, &DEFAULT_WEIGHTS);
assert!((result - 0.576_f64).abs() < 1e-10,
    "expected 0.576, got {}", result);
```

**Why distinct values detect transposition:**
- If `graph` and `contradiction` are swapped: `0.3*0.46 + 0.5*0.23 + 0.8*0.31 = 0.138+0.115+0.248 = 0.501` ≠ 0.576
- If `graph` and `embedding` are swapped: `0.5*0.46 + 0.8*0.23 + 0.3*0.31 = 0.230+0.184+0.093 = 0.507` ≠ 0.576
- If `contradiction` and `embedding` are swapped: `0.8*0.46 + 0.3*0.23 + 0.5*0.31 = 0.368+0.069+0.155 = 0.592` ≠ 0.576

Any two-argument swap produces a value at least 0.015 away from the correct answer.

---

### `lambda_single_dimension_deviation` (R-01 triangulation)

**Purpose:** Hold two dimensions at 1.0, vary the third independently for each
slot. Assert each deviation produces a different magnitude change.

**Three sub-cases:**

1. Vary graph: `compute_lambda(0.5, Some(1.0), 1.0, &DEFAULT_WEIGHTS)`
   Expected: `0.5*0.46 + 1.0*0.23 + 1.0*0.31 = 0.23 + 0.23 + 0.31 = 0.77`

2. Vary embedding: `compute_lambda(1.0, Some(0.5), 1.0, &DEFAULT_WEIGHTS)`
   Expected: `1.0*0.46 + 0.5*0.23 + 1.0*0.31 = 0.46 + 0.115 + 0.31 = 0.885`

3. Vary contradiction: `compute_lambda(1.0, Some(1.0), 0.5, &DEFAULT_WEIGHTS)`
   Expected: `1.0*0.46 + 1.0*0.23 + 0.5*0.31 = 0.46 + 0.23 + 0.155 = 0.845`

Assert all three results are distinct: `0.77 != 0.885 != 0.845`. This confirms
each argument lands in the correct weight slot.

---

### `lambda_weighted_sum` (basic correctness)

**Purpose:** Verify weighted-sum arithmetic with mid-range values.

**Input:** `compute_lambda(0.6, Some(0.7), 0.4, &DEFAULT_WEIGHTS)`

**Expected:**
```
0.6*0.46 + 0.7*0.23 + 0.4*0.31
= 0.276 + 0.161 + 0.124
= 0.561
```

Assertion uses `< 1e-10` epsilon.

---

### `lambda_renormalization_without_embedding` (R-07, AC-08)

**Two sub-cases both required:**

**Case 1 — trivial (AC-08):**
```rust
let result = compute_lambda(1.0, None, 1.0, &DEFAULT_WEIGHTS);
assert_eq!(result, 1.0);
```
When both remaining dimensions are 1.0, re-normalized weights must still sum
to 1.0. This is the AC-08 criterion.

**Case 2 — non-trivial (R-07 critical):**
```rust
let result = compute_lambda(0.8, None, 0.6, &DEFAULT_WEIGHTS);
// Re-normalized weights: graph = 0.46/0.77, contradiction = 0.31/0.77
// 0.8 * (0.46/0.77) + 0.6 * (0.31/0.77)
// = 0.8 * 0.597402... + 0.6 * 0.402597...
// = 0.477922... + 0.241558...
// = 0.719480...
let expected = 0.8 * (0.46_f64 / 0.77_f64) + 0.6 * (0.31_f64 / 0.77_f64);
assert!((result - expected).abs() < 1e-10);
```

**Why case 2 is required for R-07:** A test using only the all-ones case passes
trivially for any weight values that sum to 1.0. The non-trivial case verifies
that 0.46 and 0.31 (not 0.35 and 0.30 from the old 4-dimension era) are the
re-normalization base. If the implementation uses stale weights, the result
differs detectably from `expected`.

---

### `lambda_renormalization_partial` (2-of-3 partial test)

**Input:** `compute_lambda(0.4, None, 0.9, &DEFAULT_WEIGHTS)`

**Expected:**
```rust
let expected = 0.4 * (0.46_f64 / 0.77_f64) + 0.9 * (0.31_f64 / 0.77_f64);
```

Assert `(result - expected).abs() < 1e-10`.

---

### `lambda_renormalized_weights_sum_to_one` (2-of-3 weight-sum check)

**Input:** Any two non-zero inputs with `embedding = None`.

**Assertion:** Re-normalized weights sum to 1.0 within epsilon:
```rust
let w_graph = DEFAULT_WEIGHTS.graph_quality;
let w_contra = DEFAULT_WEIGHTS.contradiction_density;
let sum = w_graph / (w_graph + w_contra) + w_contra / (w_graph + w_contra);
assert!((sum - 1.0_f64).abs() < f64::EPSILON);
```

---

### `lambda_embedding_excluded_specific` (non-trivial 2-of-3)

**Input:** `compute_lambda(0.7, None, 0.8, &DEFAULT_WEIGHTS)`

**Expected:**
```rust
let expected = 0.7 * (0.46_f64 / 0.77_f64) + 0.8 * (0.31_f64 / 0.77_f64);
```

Assert `(result - expected).abs() < 1e-10`.

---

### `lambda_custom_weights_zero_embedding` (struct literal update)

**Purpose:** Verify that a `CoherenceWeights` struct with `embedding_consistency: 0.0`
still compiles and produces correct output.

**Updated struct literal (no `confidence_freshness` field):**
```rust
let weights = CoherenceWeights {
    graph_quality: 0.5,
    contradiction_density: 0.5,
    embedding_consistency: 0.0,
};
let result = compute_lambda(0.8, Some(0.6), 0.4, &weights);
// With embedding_consistency = 0.0, embedding contributes 0.0 * 0.6 = 0.0
// result = 0.8*0.5 + 0.0 + 0.4*0.5 = 0.4 + 0.0 + 0.2 = 0.6
assert!((result - 0.6_f64).abs() < 1e-10);
```

---

### `DEFAULT_STALENESS_THRESHOLD_SECS` presence assertion (R-03, AC-11)

Not a code-level unit test. Verified by grep in Stage 3c:

```bash
grep -n "DEFAULT_STALENESS_THRESHOLD_SECS" \
    crates/unimatrix-server/src/infra/coherence.rs
```

Must return exactly one match (the `pub const` definition line).

The comment on that line must include the phrase "Not a Lambda input" (or equivalent)
as per ADR-002. Build success implies `run_maintenance()` still references the constant
by name.

---

### Deleted Freshness Tests (R-09)

The following tests must NOT exist post-delivery. Their absence verifies that
`confidence_freshness_score()` and `oldest_stale_age()` were fully removed:

`freshness_empty_entries`, `freshness_all_stale`, `freshness_none_stale`,
`freshness_uses_max_of_timestamps`, `freshness_recently_accessed_not_stale`,
`freshness_both_timestamps_older_than_threshold`, `oldest_stale_no_stale`,
`oldest_stale_one_stale`, `oldest_stale_both_timestamps_zero`,
`staleness_threshold_constant_value`, `recommendations_below_threshold_stale_confidence`

In Stage 3c, confirm none of these appear in `cargo test -- --list` output for
`unimatrix-server`.

---

### `generate_recommendations` — remaining branches (AC-09)

The stale-confidence recommendation branch is deleted. The remaining branches
(graph stale ratio, embedding inconsistencies, quarantined entries) must still
fire correctly.

**Test: below-threshold no stale (simplified):**
```rust
// Lambda below threshold, no stale entries
let recs = generate_recommendations(0.5, 0.8, 0.0, 0, 0);
// Should recommend: Lambda below threshold
assert!(!recs.is_empty());
// Must not contain stale-confidence language
assert!(!recs.iter().any(|r| r.contains("confidence") && r.contains("stale")));
```

**Test: embedding inconsistencies still fire:**
```rust
let recs = generate_recommendations(0.9, 0.8, 0.0, 5, 0);
assert!(recs.iter().any(|r| r.contains("embedding")));
```

---

## Integration Test Expectations

No MCP-level integration tests are required for Component A. The `infra/coherence.rs`
module has no MCP interface; its behavior surfaces through `context_status` tool
responses tested in Component B and the `confidence` suite.

---

## Edge Cases

| Input | Expected | Notes |
|-------|----------|-------|
| `(0.0, Some(0.0), 0.0, &DEFAULT_WEIGHTS)` | 0.0 | All-zero boundary |
| `(1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS)` | 1.0 | All-one boundary (AC-07) |
| `(1.0, None, 1.0, &DEFAULT_WEIGHTS)` | 1.0 | 2-of-3 boundary (AC-08) |
| `(0.8, None, 0.6, &DEFAULT_WEIGHTS)` | `0.8*(0.46/0.77)+0.6*(0.31/0.77)` | Non-trivial 2-of-3 (R-07) |
| `(0.8, Some(0.5), 0.3, &DEFAULT_WEIGHTS)` | 0.576 | Distinct-value (R-01) |
