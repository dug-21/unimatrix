# Test Plan: phase_freq_table.rs
# `crates/unimatrix-server/src/services/phase_freq_table.rs`

## Component Responsibilities

New file. Provides `PhaseFreqTable` struct, `PhaseFreqTableHandle` type alias,
`new()`, `new_handle()`, `rebuild()`, and `phase_affinity_score()`. Analogous to
`TypedGraphState` in `services/typed_graph.rs` — that module's test suite is the
structural template.

---

## Unit Test Expectations

All tests in `#[cfg(test)] mod tests` inside `phase_freq_table.rs`.

### AC-01 / Cold-Start Construction

**`test_phase_freq_table_new_returns_cold_start`**
- Arrange: call `PhaseFreqTable::new()`.
- Assert: `table.is_empty() == true`.
- Assert: `use_fallback == true`.

**`test_new_handle_wraps_cold_start_state`**
- Arrange: call `PhaseFreqTable::new_handle()`.
- Act: acquire read lock with `.unwrap_or_else(|e| e.into_inner())`.
- Assert: `guard.use_fallback == true`.
- Assert: `guard.table.is_empty() == true`.

### AC-03 / Handle Mechanics (mirrors TypedGraphState pattern)

**`test_new_handle_write_then_read_reflects_change`**
- Write `use_fallback = false` under write lock.
- Read: assert `use_fallback == false`.

**`test_new_handle_returns_independent_handles`**
- Two separate `new_handle()` calls produce independent Arc instances.
- Write to handle1. Read from handle2: still cold-start.

**`test_arc_clone_shares_state`**
- Clone handle. Write through original. Read through clone: sees the write.

**`test_phase_freq_table_handle_poison_recovery`**
- Poison the write lock by panicking while holding it (`std::panic::catch_unwind`).
- Subsequent read via `.unwrap_or_else(|e| e.into_inner())` must not panic.
- (Mirrors `test_typed_graph_state_handle_poison_recovery` in `typed_graph.rs`.)

### AC-07 / phase_affinity_score — Three `1.0` Return Paths (covers R-04)

**`test_phase_affinity_score_use_fallback_returns_one`**  ← R-04 primary
- Arrange: `PhaseFreqTable { table: HashMap::new(), use_fallback: true }`.
- Act: `score = table.phase_affinity_score(42, "decision", "delivery")`.
- Assert: `score == 1.0f32` (exact equality).
- This is the PPR cold-start contract: neutral multiplier.

**`test_phase_affinity_score_absent_phase_returns_one`**
- Arrange: populated table with `use_fallback = false`, containing `("scope", "decision")` bucket only.
- Act: call with `phase = "delivery"` (absent from table).
- Assert: returns `1.0f32`.

**`test_phase_affinity_score_absent_entry_returns_one`**
- Arrange: populated table with `use_fallback = false`, `("delivery", "decision")` bucket containing `entry_id = 42` only.
- Act: call with `entry_id = 99` (absent from bucket).
- Assert: returns `1.0f32`.

**`test_phase_affinity_score_present_entry_returns_rank_score`**
- Arrange: populated table with `("delivery", "decision")` bucket: `[(42, 0.666)]`.
- Act: call with `entry_id = 42, category = "decision", phase = "delivery"`.
- Assert: `(score - 0.666f32).abs() < f32::EPSILON`.

### AC-13 / AC-14 / Rank Normalization Formula (covers R-07)

**`test_phase_affinity_score_single_entry_bucket_returns_one`**  ← R-07 primary
- Arrange: `table[("scope", "decision")] = vec![(7, 1.0f32)]`.
- Act: `phase_affinity_score(7, "decision", "scope")`.
- Assert: `== 1.0f32`.
- This test catches the off-by-one formula `1 - rank/N` which returns `0.0` for N=1.

**`test_rebuild_normalization_three_entry_bucket_exact_scores`**  ← AC-14
- Arrange: synthetic `PhaseFreqRow` list — `(delivery, decision, 10, 10)`,
  `(delivery, decision, 20, 5)`, `(delivery, decision, 30, 1)` — N=3 bucket.
- Act: call the normalization logic (either via `rebuild` with a mock store, or by
  constructing a `Vec<PhaseFreqRow>` and calling an internal helper).
- Assert:
  - `entry_id=10, rank=1 → score == 1.0f32`.
  - `entry_id=20, rank=2 → score ≈ 0.6666f32` (tolerance `1e-5`).
  - `entry_id=30, rank=3 → score ≈ 0.3333f32` (tolerance `1e-5`).
- Assert Vec is sorted descending by score.

**`test_rebuild_normalization_last_entry_in_five_bucket`**
- 5-entry bucket with frequencies 100, 80, 60, 40, 20.
- Entry at rank 5: expected score = `(5-1)/5 = 0.8f32`.
- Assert not `0.0` — catches the alternative off-by-one where last entry collapses.

**`test_rebuild_normalization_two_entry_bucket`**
- N=2: rank-1 → `1.0`, rank-2 → `0.5`.

### R-10 / Phase Vocabulary Staleness

**`test_phase_affinity_score_unknown_phase_returns_one`**
- Populated table with `("delivery", "decision")` bucket.
- Call with `phase = "implement"` (not present — simulates rename).
- Assert: returns `1.0f32` (graceful degradation, not an error).

### AC-15 / File Size (structural check, not a Rust test)

`wc -l crates/unimatrix-server/src/services/phase_freq_table.rs` must report ≤ 500
lines. Verified at merge time.

---

## Integration Test Expectations

`PhaseFreqTable::rebuild` requires a live `Store`. These tests use `TestDb` and live in
`unimatrix-store/src/query_log.rs` tests or a dedicated integration test file.

See `query_log_store_method.md` (AC-08) for the `TestDb`-based rebuild test — that test
validates that the store method returns the rows `rebuild()` needs to populate the table.

A full end-to-end `rebuild` test that goes from seeded `query_log` rows through
`query_phase_freq_table` → `rebuild` → populated `PhaseFreqTable` lives in
`background_tick.md` (AC-04 success path).

---

## Edge Cases

- `result_entry_ids = NULL` row in `query_log`: must not appear in rebuilt table
  (filtered by SQL `WHERE result_entry_ids IS NOT NULL`). Verified by AC-08.
- `phase = NULL` rows: filtered by SQL `WHERE phase IS NOT NULL`. Verify separately
  in the store-level test (all-null-phase scenario, see `query_log_store_method.md`).
- Phase string case: `"Delivery"` and `"delivery"` are distinct HashMap keys — no
  normalization. Document in module-level doc comment as known behavior.

---

## Covered Risks

| Risk | Test |
|------|------|
| R-04 (wrong cold-start return) | `test_phase_affinity_score_use_fallback_returns_one` |
| R-07 (off-by-one normalization) | `test_phase_affinity_score_single_entry_bucket_returns_one`, `test_rebuild_normalization_three_entry_bucket_exact_scores`, `test_rebuild_normalization_last_entry_in_five_bucket` |
| R-10 (phase rename staleness) | `test_phase_affinity_score_unknown_phase_returns_one` |
