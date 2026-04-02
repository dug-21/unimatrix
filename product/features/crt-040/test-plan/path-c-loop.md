# Test Plan: path-c-loop (Wave 3)

**File modified:** `crates/unimatrix-server/src/services/nli_detection_tick.rs`

**Changes:**
- Add `const MAX_COSINE_SUPPORTS_PER_TICK: usize = 50` module constant
- Build `category_map: HashMap<u64, String>` from `all_active` in Phase 5 (before Path C)
- Add Path C write loop after Path A log, before Path B gate
- Add unconditional `debug!` observability log after the Path C loop

**Critical context from source reading:**
- `candidate_pairs` is already truncated to `config.max_graph_inference_per_tick` in Phase 5
  before Path C runs. Path C iterates the truncated set, not the raw Phase 4 output.
- A `category_map: HashMap<u64, &str>` already exists in Phase 5 (built from `all_active`
  for the Phase 5 sort). Path C may reuse this or build its own — the implementation must
  decide. If reused, the HashMap key is `u64` and value is `&str`. The test plan is
  agnostic on this implementation detail, but the unit tests must verify behavior, not
  mechanism.
- The joint early-return `if candidate_pairs.is_empty() && informs_metadata.is_empty()` that
  previously existed in Phase 5 is **REMOVED** by this feature (AC-19 resolution). Path C
  runs unconditionally on every tick. When both lists are empty, Path A and Path C iterate
  zero times but the Path C observability log still fires. The Path B entry gate
  (`if candidate_pairs.is_empty()`) is retained and guards NLI batch only.

**Risk coverage:** R-01 (Critical), R-05, R-06, R-07, R-08, R-09, R-10 (Medium/Low), AC-12,
AC-15, AC-19

---

## Unit Test Expectations

Tests live in `#[cfg(test)] mod tests` inside `nli_detection_tick.rs`. Tests that exercise
Path C directly use helpers that call the Path C sub-function (`run_cosine_supports_path`
if extracted, or a test-accessible variant of the loop). Where the full tick function is
exercised, use an in-memory store populated with the required state.

Tests follow the Arrange/Act/Assert pattern with `#[tokio::test]`.

---

### Group 1: Core qualification logic (R-01, AC-01–AC-04)

#### TC-01: Qualifying pair writes Supports edge with correct source (AC-01, R-01)

```
async fn test_path_c_qualifying_pair_writes_supports_edge()
```

- Arrange:
  - In-memory store with two entries: id=1 (category="lesson-learned"), id=2 (category="decision")
  - `candidate_pairs = vec![(1, 2, 0.70_f32)]`
  - `category_map` pre-built: `{1 => "lesson-learned", 2 => "decision"}`
  - `config.supports_cosine_threshold = 0.65`
  - `config.informs_category_pairs` includes `["lesson-learned", "decision"]`
  - `existing_supports_pairs = HashSet::new()` (empty)
  - Budget: 0 edges written so far
- Act: run Path C logic
- Assert:
  - `graph_edges` contains one row: `(source_id=1, target_id=2, relation_type="Supports", source="cosine_supports")`
  - Row count = 1

#### TC-02: Pair below threshold produces no edge (AC-02)

```
async fn test_path_c_below_threshold_no_edge()
```

- Arrange: same as TC-01 but `candidate_pairs = vec![(1, 2, 0.64_f32)]`
- Assert: `graph_edges` is empty

#### TC-03: Pair at exactly threshold boundary qualifies (AC-02 boundary — >= not >)

```
async fn test_path_c_exact_threshold_boundary_qualifies()
```

- Arrange: `candidate_pairs = vec![(1, 2, 0.65_f32)]`, threshold = 0.65
- Assert: edge written (`graph_edges` row count = 1)
- Covers: the `>=` vs `>` boundary — pair at exactly 0.65 MUST qualify

#### TC-04: Disallowed category pair produces no edge even above threshold (AC-03, R-01)

```
async fn test_path_c_disallowed_category_no_edge()
```

- Arrange:
  - id=5 (category="decision"), id=6 (category="decision")
  - `candidate_pairs = vec![(5, 6, 0.80_f32)]`
  - `category_map = {5 => "decision", 6 => "decision"}`
  - `informs_category_pairs` does NOT include `["decision", "decision"]`
- Assert: `graph_edges` is empty (category filter blocks write even at cosine 0.80)
- Covers: R-01 third scenario (disallowed category above threshold)

#### TC-05: Entry absent from category_map produces no edge (R-01, deprecated mid-tick)

```
async fn test_path_c_missing_entry_id_no_panic_no_edge()
```

- Arrange:
  - `candidate_pairs = vec![(99, 2, 0.70_f32)]`
  - `category_map` does NOT contain id=99 (simulates entry deprecated between Phase 2 and Path C)
  - id=2 present in map
- Assert:
  - No panic
  - `graph_edges` is empty
  - (If tracing subscriber available) `warn!` fired for entry 99
- Covers: R-01 second scenario (None branch continues without panic)

#### TC-06: Pair already in existing_supports_pairs is skipped (AC-04)

```
async fn test_path_c_existing_pair_skipped()
```

- Arrange:
  - `candidate_pairs = vec![(1, 2, 0.70_f32)]`
  - `existing_supports_pairs = HashSet::from([(1_u64, 2_u64)])`
  - `graph_edges` already contains row `(1, 2, "Supports")`
- Act: run Path C
- Assert: `graph_edges` still has exactly ONE row (no duplicate)
- Covers: AC-04

---

### Group 2: Budget cap (AC-12)

#### TC-07: Exactly 50 edges written from 60 qualifying pairs (AC-12)

```
async fn test_path_c_budget_cap_50_from_60_qualifying()
```

- Arrange:
  - In-memory store
  - 60 candidate pairs, all with cosine=0.70, all qualifying category combination,
    none in `existing_supports_pairs`
  - Pair IDs: `(i, i+1000)` for i in 1..=60 (ensures unique pairs)
  - `config.supports_cosine_threshold = 0.65`
  - `config.max_graph_inference_per_tick = 60` (REQUIRED: must be >= 60 so that Phase 5
    truncation does not reduce the candidate list below 60 before Path C runs; the default
    of 10 would truncate to 10 candidates, making the 50-edge budget cap unreachable)
  - `MAX_COSINE_SUPPORTS_PER_TICK = 50`
- Act: run Path C
- Assert: `graph_edges` row count == 50 (budget exhausted; last 10 pairs not written)
- Covers: AC-12 (budget cap)

#### TC-08: Budget counter increments only on true return from write_graph_edge (R-07, failure mode)

```
async fn test_path_c_budget_counter_not_incremented_on_false_return()
```

- Arrange:
  - In-memory store
  - `config.max_graph_inference_per_tick = 65` (must be >= total candidate count so Phase 5
    truncation does not reduce the list before Path C)
  - 50 qualifying pairs (all new — will be written, `write_graph_edge` returns `true`)
  - 10 UNIQUE-conflict pairs: pairs that pass the `existing_supports_pairs` pre-filter (NOT
    in the pre-filter set) but are already present in `graph_edges` from a prior write in
    the same test — these hit INSERT OR IGNORE and cause `write_graph_edge` to return `false`
    (i.e., `rows_affected = 0`). Interleave these BEFORE some of the new 50 pairs.
  - Total input: 60 candidates; 10 will produce UNIQUE conflict false returns, 50 will
    produce genuine inserts (true returns)
- Act: run Path C
- Assert:
  - `graph_edges` row count == 50 (only the genuine inserts)
  - Budget counter reaches exactly 50 (false returns from UNIQUE conflict do NOT increment it)
  - Path C loop exits after 50 true returns (budget exhausted), not after 60 iterations
- Covers: R-07 (UNIQUE conflict → `write_graph_edge` returns `false` → counter NOT
  incremented; the budget cap is reached only by counting actual written edges)
- Note: this test distinguishes two distinct false-return paths: (1) `existing_supports_pairs`
  pre-filter skips the call entirely, and (2) UNIQUE conflict causes `write_graph_edge` to
  return `false` after the call. Both must leave the budget counter unchanged. This TC
  exercises path (2); TC-06 covers path (1).

---

### Group 3: NaN/Inf guard (R-09)

#### TC-09: NaN cosine pair produces no edge and emits warn

```
async fn test_path_c_nan_cosine_no_edge_warn_emitted()
```

- Arrange: `candidate_pairs = vec![(1, 2, f32::NAN)]`
- Assert:
  - No panic
  - `graph_edges` is empty
  - Loop continues (subsequent pairs after NaN are still processed)
- Covers: R-09

#### TC-10: Infinity cosine pair produces no edge

```
async fn test_path_c_infinity_cosine_no_edge()
```

- Arrange: `candidate_pairs = vec![(1, 2, f32::INFINITY)]`
- Assert: no panic, `graph_edges` empty
- Covers: R-09 second variant

#### TC-11: NaN guard fires before threshold comparison

```
async fn test_path_c_nan_guard_order_threshold_not_evaluated()
```

- Arrange: pair with `f32::NAN`; set `supports_cosine_threshold = 0.0` (every finite value
  would qualify)
- Assert: edge NOT written despite NaN being "above" a 0.0 threshold in raw comparison
  (`NAN >= 0.0` is false in Rust, but explicit `!is_finite()` guard must fire first)
- Covers: R-09 guard placement

---

### Group 4: Observability (R-06, AC-19)

#### TC-12: Observability log fires when candidate_pairs is empty (AC-19)

```
async fn test_path_c_observability_log_fires_with_zero_counts()
```

- Arrange:
  - `candidate_pairs = vec![]` (empty)
  - `informs_metadata = vec![]` (empty)
  - The joint early-return has been removed (AC-19 resolution). Path C runs unconditionally,
    so both lists may be empty without bypassing the observability log.
- Assert: the debug log fires with `cosine_supports_candidates = 0` and
  `cosine_supports_edges_written = 0`
- Covers: R-06, AC-19

#### TC-13: Observability log fires with correct counts for qualifying run

```
async fn test_path_c_observability_log_counts_correct()
```

- Arrange: 5 qualifying pairs at cosine 0.70; 3 pairs below threshold at cosine 0.50
- Assert: observability log fires with:
  - `cosine_supports_candidates = 5` (pairs that passed cosine threshold; category check counts here or before — match implementation)
  - `cosine_supports_edges_written = 5` (all 5 written; no budget exhaustion)
- Note: `cosine_supports_candidates` counts pairs that passed the cosine threshold gate
  (per IMPLEMENTATION-BRIEF.md pseudocode). Verify the exact counter increment site in the
  delivery code.
- Covers: AC-19 (correct counts)

#### TC-14: Log field names do not collide with Path A fields

```
fn test_path_c_log_field_names_distinct_from_path_a()
```

- This is a code review / static test. Verify by inspection:
  - Path A uses fields `informs_edges_written`, `informs_candidates_after_cap`
  - Path C must use `cosine_supports_candidates`, `cosine_supports_edges_written`
  - No overlap
- Document result in RISK-COVERAGE-REPORT.md under R-06

---

### Group 5: Backward compatibility (R-05, AC-15)

#### TC-15: inferred_edge_count unchanged after cosine_supports write (AC-15)

```
async fn test_inferred_edge_count_unchanged_after_path_c_write()
```

- Arrange:
  - In-memory store with `source='nli'` edge already present (contributes to `inferred_edge_count`)
  - Call Path C to write a `source='cosine_supports'` edge
- Act: compute `GraphCohesionMetrics`
- Assert: `inferred_edge_count` equals baseline (only `source='nli'` rows counted)
- Covers: AC-15, R-05, NFR-06

---

### Group 6: Path C unconditional execution (AC-05)

#### TC-16: Path C runs when nli_enabled=false

```
async fn test_path_c_runs_unconditionally_without_nli()
```

- Arrange:
  - Config: `nli_enabled = false` (no provider)
  - In-memory store with qualifying candidate pair
- Act: run `run_graph_inference_tick`
- Assert: `graph_edges` contains the Supports edge from Path C
- Covers: AC-05, FR-13 (Path C unconditional)
- Note: Path B (NLI gate) will early-return due to `get_provider()` failing. Path C must
  have already run before that gate.

---

### Group 7: Phase 4 canonical form (R-08)

#### TC-17: Reversed pair in candidate_pairs produces at most one edge

```
async fn test_path_c_reversed_pair_no_duplicate_edge()
```

- Arrange:
  - `candidate_pairs = vec![(1, 2, 0.70), (2, 1, 0.70)]` — both directions present
  - `existing_supports_pairs` is empty
- Act: run Path C
- Assert: `graph_edges` row count == 1 (canonical `(lo,hi)` form dedup, or INSERT OR IGNORE)
- Covers: R-08 (Phase 4 normalization consistency)

---

### Group 8: MAX_COSINE_SUPPORTS_PER_TICK constant (ADR-004)

#### TC-18: Constant value is 50 and is independent of MAX_INFORMS_PER_TICK

```
fn test_max_cosine_supports_per_tick_value()
```

- Assert: `MAX_COSINE_SUPPORTS_PER_TICK == 50_usize`
- Assert: `MAX_COSINE_SUPPORTS_PER_TICK != MAX_INFORMS_PER_TICK` (25 != 50)
- Covers: AC-12, ADR-004 independence of budget constants

---

## Integration Test Expectations

See OVERVIEW.md integration harness plan for the two new infra-001 tests:
- `test_context_status_supports_edge_count_increases_after_tick` (lifecycle suite)
- `test_inferred_edge_count_unchanged_by_cosine_supports` (lifecycle suite)

Integration gate for path-c-loop: the smoke suite must pass after delivery. The lifecycle
suite `test_context_status_supports_edge_count_increases_after_tick` verifies AC-05 through
the MCP interface.

---

## Edge Cases

| Edge Case | Expectation |
|-----------|-------------|
| `candidate_pairs` empty, `informs_metadata` non-empty | Path C runs; observability log fires with 0/0 (TC-12) |
| `candidate_pairs` empty, `informs_metadata` also empty | Path C runs (joint early-return removed); observability log fires with `cosine_supports_candidates=0` and `cosine_supports_edges_written=0` (TC-12) |
| Budget exhausted exactly at 50 | Loop breaks at 51st candidate; next tick handles remaining |
| `supports_cosine_threshold` raised above all candidates | 0 edges written; log fires with `cosine_supports_edges_written=0` |
| All pairs above threshold but all in disallowed categories | 0 edges written; budget counter not incremented |
| `category_map` pre-built from `all_active` (R-10) | O(1) lookup per pair — code review gate; no linear scan |

---

## Assertions Summary

| AC-ID / Risk | Test | Assertion |
|-------------|------|-----------|
| AC-01 | TC-01 | Qualifying pair writes edge with `source="cosine_supports"` |
| AC-02 | TC-02, TC-03 | Below threshold: no edge; at exactly 0.65: edge written |
| AC-03 | TC-04 | Disallowed category: no edge even at 0.80 |
| AC-04 | TC-06 | Pre-existing pair skipped; row count unchanged |
| AC-05 | TC-16 | Path C runs with `nli_enabled=false` |
| AC-12 | TC-07 | 60 qualifying pairs → exactly 50 edges written |
| AC-15 | TC-15 | `inferred_edge_count` unchanged after cosine_supports write |
| AC-19 | TC-12, TC-13 | Observability log fires with correct zero/non-zero counts |
| R-01 | TC-01, TC-04, TC-05 | HashMap lookup used; None branch continues; disallowed category blocked |
| R-06 | TC-12 | Log fires when candidates = 0 |
| R-07 | TC-08 | Budget counter not incremented on false return |
| R-08 | TC-17 | Reversed pair → at most one edge |
| R-09 | TC-09, TC-10, TC-11 | NaN/Inf: no edge, warn emitted, loop continues |
| R-10 | (code review) | HashMap pre-build, not linear scan per pair |
