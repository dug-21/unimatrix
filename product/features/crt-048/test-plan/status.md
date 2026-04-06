# Test Plan: `services/status.rs` — Status Orchestration

## Component Scope

Orchestration layer: calls `infra/coherence.rs` pure functions, assembles
`StatusReport`, drives Phase 5 of `context_status`. Contains two `compute_lambda()`
call sites (main path and `coherence_by_source` loop) that are the highest-risk
asymmetric update surface in the feature.

---

## Risks Owned by This Component

| Risk | Coverage Requirement |
|------|---------------------|
| R-01 | Both call sites pass arguments in correct order (distinct-value test) |
| R-06 (Critical) | Both call sites updated identically — exactly 2 `compute_lambda(` calls with 4 args each; per-source Lambda consistency |
| R-07 | `coherence_by_source` per-source values are consistent with main-path for same structural inputs |

---

## Static Analysis Assertions (Grep — MANDATORY in Stage 3c)

These assertions are not code tests; they are pre-flight grep checks that
detect the most likely silent failure mode.

**Assertion 1 — exactly 2 call sites, 4 arguments each (R-06, AC-13):**
```bash
grep -n "compute_lambda(" crates/unimatrix-server/src/services/status.rs
```
Must return exactly 2 matching lines. Both lines must show 4 arguments
(graph, embedding, contradiction, weights) — not 5 (old signature with freshness).

**Assertion 2 — no freshness function calls remain:**
```bash
grep -n "confidence_freshness_score\|oldest_stale_age" \
    crates/unimatrix-server/src/services/status.rs
```
Must return zero matches.

**Assertion 3 — `generate_recommendations` call has 5 args, not 7:**
```bash
grep -n "generate_recommendations(" crates/unimatrix-server/src/services/status.rs
```
Must return exactly 1 match. Inspect the call: must have 5 arguments
(`lambda`, `threshold`, `graph_stale_ratio`, `embedding_inconsistent_count`,
`total_quarantined`). The removed arguments (`stale_confidence_count`,
`oldest_stale_age_secs`) must not be present.

**Assertion 4 — `load_active_entries_with_tags()` retained (FR-11):**
```bash
grep -n "load_active_entries_with_tags" crates/unimatrix-server/src/services/status.rs
```
Must return at least 1 match. This call serves the `coherence_by_source` grouping.

---

## Unit Test Expectations

### Main-path `compute_lambda()` call correctness (R-01, AC-07)

The main-path call at line 771 must pass arguments in the order:
`(graph_quality_score, Some(embedding_consistency_score), contradiction_density_score, &weights)`.

**Detection test (in `coherence.rs` — feeds into this component's behavior):**
`lambda_specific_three_dimensions` uses inputs (0.8, Some(0.5), 0.3) and expects 0.576.
If the main-path call in `status.rs` passes arguments in wrong order, the per-source
and main Lambda values will diverge in any integration test that checks specific
numeric Lambda values.

### `coherence_by_source` per-source consistency (R-06, AC-13)

**Test name:** `coherence_by_source_uses_three_dim_lambda`

**Arrangement:** Construct a synthetic set of `EntryRecord` instances grouped by two
different `trust_source` values. Arrange them to have distinct structural dimension
scores. Call through `compute_lambda()` directly (not through the full Phase 5
pipeline, which requires a live database) for each group using the same 3-dimension
signature as the main path.

**Assertion:** The per-source Lambda for source A and source B are not equal (they
have different structural profiles). Neither equals a value that would result from
the 4-dimension computation (which would require a `freshness` argument).

**Why this test is required:** R-06 identifies that the `coherence_by_source` loop
at lines 793–804 is an independent call site. If only the main-path call is updated,
the per-source loop still compiles (as long as a local variable of type `f64` is
still available at that scope to fill the freshness position). The test catches
semantic mismatch even though the build succeeds.

**Concrete assertion:**
```rust
// Source A: strong graph (0.9), weak contradiction (0.3), embedding absent
let lambda_a = compute_lambda(0.9, None, 0.3, &DEFAULT_WEIGHTS);
// Source B: weak graph (0.3), strong contradiction (0.9), embedding absent
let lambda_b = compute_lambda(0.3, None, 0.9, &DEFAULT_WEIGHTS);
// With new weights (0.46 graph, 0.31 contradiction):
// lambda_a = 0.9*(0.46/0.77) + 0.3*(0.31/0.77) ≈ 0.538 + 0.121 = 0.659
// lambda_b = 0.3*(0.46/0.77) + 0.9*(0.31/0.77) ≈ 0.179 + 0.362 = 0.541
assert!(lambda_a > lambda_b,
    "source A (strong graph) should have higher lambda than source B (strong contradiction): {} vs {}",
    lambda_a, lambda_b);
```

**Relationship test:** If both sources had identical structural profiles, their
per-source Lambda must equal the main-path Lambda computed from those same inputs.

---

### `run_maintenance()` compile correctness (R-03, AC-11)

No functional unit test is required for `run_maintenance()` itself — the function
is unchanged. The indirect test is:

1. `cargo build --workspace` succeeds — `run_maintenance()` references
   `DEFAULT_STALENESS_THRESHOLD_SECS` by name. If the constant were deleted,
   this would be a compile error.

2. Grep assertion (in coherence.md) confirms exactly one definition of the constant.

---

## Integration Test Expectations

The `services/status.rs` behavior surfaces through the `context_status` MCP tool.
Integration tests in `suites/test_tools.py` and `suites/test_confidence.py` cover
the end-to-end path.

**Expected from existing `test_tools.py`:** Any test that calls `context_status`
and asserts on the response shape must pass. If a test previously asserted that
`confidence_freshness_score` or `stale_confidence_count` are present in the response,
it must be updated (R-05 / R-08).

**Expected from `test_confidence.py`:** Any test that asserts a specific numerical
Lambda value must have its expected value re-derived with 3-dimension weights if the
value was originally computed with 4-dimension weights (R-07).

---

## Edge Cases

| Scenario | Expected Behavior |
|----------|-----------------|
| `coherence_by_source` with all sources having same structural profile | Per-source Lambda equals main-path Lambda |
| `coherence_by_source` with one source having no embedded entries (`embedding = None`) | Per-source Lambda uses 2-of-3 re-normalization; main-path uses 3-of-3 (different result is correct) |
| Zero active entries | Phase 5 skips freshness scan (already removed); `coherence_by_source` is empty map; `compute_lambda` not called (or called with 0.0 defaults); function does not panic |
| `generate_recommendations()` with `graph_stale_ratio = 0.0, embedding_inconsistent_count = 0, total_quarantined = 0` and `lambda > threshold` | Returns empty recommendation list (no stale-confidence branch to fire) |
