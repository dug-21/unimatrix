# Test Plan: search_scoring
# `crates/unimatrix-server/src/services/search.rs`

## Component Responsibilities

Adds `phase_freq_table: PhaseFreqTableHandle` field to `SearchService` and
`current_phase: Option<String>` field to `ServiceSearchParams`. Wires the pre-loop
lock-acquire → snapshot-extract → release → scoring-loop pattern into `search()`.

The key invariant: the lock on `PhaseFreqTableHandle` is acquired once before the
scoring loop, the bucket snapshot is extracted, the lock is released, and then the
scoring loop runs against the snapshot.

---

## Unit Test Expectations

Tests in `#[cfg(test)] mod tests` inside `search.rs` or a dedicated scoring test
helper. `SearchService` construction requires a `PhaseFreqTableHandle` parameter
(ADR-005 — compile-enforced).

### AC-11 / Cold-Start Invariants (three tests, covers R-03 and R-04)

**`test_scoring_current_phase_none_sets_phase_explicit_norm_zero`**  ← AC-11 Test 1
- Arrange:
  - Build a populated `PhaseFreqTable` (`use_fallback = false`, entry 42 in `("delivery", "decision")` bucket).
  - Construct `ServiceSearchParams` with `current_phase = None`.
- Act: run the scoring pre-loop logic (or invoke `search()` with a mock store).
- Assert:
  - `phase_snapshot == None`.
  - `phase_explicit_norm == 0.0f64` for all candidates.
- Assert (score identity): all fused scores are bit-for-bit identical to scores
  computed with `w_phase_explicit = 0.0` on the same inputs.

**`test_scoring_use_fallback_true_sets_phase_explicit_norm_zero`**  ← AC-11 Test 2, R-03 primary
- Arrange:
  - Build a cold-start handle: `PhaseFreqTable::new_handle()` (use_fallback = true).
  - Construct `ServiceSearchParams` with `current_phase = Some("delivery")`.
- Act: run the pre-loop logic.
- Assert:
  - Guard fires on `use_fallback = true` BEFORE `phase_affinity_score` is called.
  - `phase_snapshot == None`.
  - `phase_explicit_norm == 0.0f64` for all candidates.
- Implementation note: since this is a unit test without a spy/mock, verify the
  guard fires correctly by asserting the snapshot is None even though `current_phase`
  is Some. The code path is: `current_phase = Some` → acquire lock → `use_fallback = true`
  → return `None` snapshot. If `phase_affinity_score` were called (wrong path), it
  would return `1.0` and produce a non-zero norm. The `== 0.0` assertion catches any
  "guard fires too late" variant.

**`test_scoring_score_identity_cold_start`**  ← score identity NFR-04, R-03
- Arrange: cold-start handle. Two candidates with known similarity scores.
- Act: compute fused scores with `current_phase = Some("delivery")` + cold-start handle.
- Act: compute fused scores with `current_phase = None` + same candidates.
- Assert: scores are bit-for-bit identical.

### AC-06 / Fused Scoring Guard Structure (covers R-06)

**`test_scoring_lock_released_before_scoring_loop`**  ← R-06
- Arrange:
  - Populate `PhaseFreqTableHandle` with a real entry.
  - Spawn a task that holds the write lock for 100ms.
  - Immediately call the scoring pre-loop extraction from another task.
- Assert: the read lock is acquired and released quickly (not held for 100ms).
- Note: this is a concurrency test verifying the read lock is not held across the
  scoring loop. If the lock is held across the loop, the write-holding task would
  block the read for the full 100ms.
- Alternative: verify structurally via code review — the guard drop site must be
  before the first loop iteration.

**`test_scoring_populated_snapshot_produces_nonzero_norm`**
- Arrange: `use_fallback = false`, entry 42 in `("delivery", "decision")` with score `1.0`.
- Arrange: `current_phase = Some("delivery")`.
- Act: run the scoring loop with candidate `{id: 42, category: "decision"}`.
- Assert: `phase_explicit_norm > 0.0f64`.

**`test_scoring_absent_entry_in_snapshot_norm_is_zero`**
- Arrange: snapshot for `("delivery", "decision")` with only entry 42.
- Act: compute norm for entry 99 (absent from bucket).
- Assert: `phase_explicit_norm == 0.0f64` (absent entry returns `1.0` from
  `phase_affinity_score`, but since snapshot is a cloned bucket, the lookup
  returns `0.0` — or `1.0` depending on snapshot design. Verify this path
  matches the architecture's intent: absent entry → `1.0` score → contributes
  `0.05 * 1.0` to fused score. The norm is NOT `0.0` for absent entries in a
  non-fallback table — this test should assert the correct behavior per ARCHITECTURE §5.)

Note on above: per AC-07, `phase_affinity_score(absent_entry) = 1.0`, so
`phase_explicit_norm = 1.0 * w_phase_explicit` for absent entries in a populated table.
The `0.0` path is ONLY for `use_fallback = true` or `current_phase = None`.

### `ServiceSearchParams` Field

**`test_service_search_params_current_phase_defaults_to_none`**
- Construct `ServiceSearchParams` without setting `current_phase`.
- Assert: `current_phase == None`.
- This verifies the new field does not break existing callers that don't set it.

---

## Integration Test Expectations (MCP-level)

See OVERVIEW.md for the two new infra-001 test scenarios:

1. `test_search_cold_start_phase_score_identity` — fresh server, verify
   `current_phase = "delivery"` produces same results as `current_phase = None`.
2. `test_search_phase_affinity_influences_ranking` — after tick, verify
   phase-biased ranking is observable.

Both planned for `suites/test_lifecycle.py`.

---

## Covered Risks

| Risk | Test |
|------|------|
| R-03 (`use_fallback` guard absent or fires too late) | `test_scoring_use_fallback_true_sets_phase_explicit_norm_zero`, `test_scoring_score_identity_cold_start` |
| R-06 (lock held across scoring loop) | `test_scoring_lock_released_before_scoring_loop`; code review |
