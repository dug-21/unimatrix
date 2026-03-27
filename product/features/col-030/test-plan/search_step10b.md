# Component Test Plan: `search.rs` — Step 10b Insertion

## Component Summary

**File**: `crates/unimatrix-server/src/services/search.rs`
**Modification**: Insert Step 10b block between Step 10 (floors, line ~899-903) and Step 11
(ScoredEntry construction, line ~906).
**Test location**: `search.rs` existing `#[cfg(test)] mod tests` (already present at line 942).
**Critical trap R-12**: DO NOT use `create_graph_edges_table` in any new test. Use
`build_typed_relation_graph` with in-memory `GraphEdgeRow` fixtures only.

---

## Test Infrastructure Notes

The existing `search.rs` `#[cfg(test)]` block uses `make_test_entry` and direct score
computation helpers. The two new tests require a `TypedRelationGraph` with a `Contradicts`
edge. Construction pattern:

```rust
use unimatrix_engine::graph::{
    GraphEdgeRow, RelationType, TypedRelationGraph, build_typed_relation_graph,
};

fn make_graph_with_contradicts(higher_id: u64, lower_id: u64) -> TypedRelationGraph {
    let entries = vec![
        make_test_entry(higher_id, Status::Active, None, 0.9, "decision"),
        make_test_entry(lower_id, Status::Active, None, 0.9, "decision"),
    ];
    let edges = vec![GraphEdgeRow {
        source_id: higher_id,
        target_id: lower_id,
        relation_type: RelationType::Contradicts.as_str().to_string(),
        weight: 1.0,
        created_at: 0,
        created_by: "test".to_string(),
        source: "nli".to_string(),
        bootstrap_only: false,  // MUST be false — bootstrap_only=true is excluded
    }];
    build_typed_relation_graph(&entries, &edges).unwrap()
}
```

The search pipeline tests call `suppress_contradicts` indirectly. They do not instantiate
`SearchService` with a live store; they simulate the Step 10b logic directly against the
pre-suppression Vecs, OR — if the test module uses full `SearchService`, they mock/inject a
`TypedGraphState` with `use_fallback=false` and the pre-built graph.

**Preferred approach**: The `search.rs` tests at line 942 are unit-level tests of search
pipeline logic (not full MCP integration). The two new tests follow the same pattern — they
test the Step 10b logic with constructed inputs, asserting on the resulting `Vec<ScoredEntry>`
shape and values.

---

## Test Cases

### T-SC-08 — Mandatory positive integration test (AC-07, FR-14)

**Risk coverage**: R-13 (eval gate not sufficient), R-12 (correct test helper), AC-07
**Scenario**: Three entries A, B, C. A and B share a Contradicts edge; A ranks higher
than B by fused score. C has no edges. After Step 10b, B must be absent from results;
A and C must be present.

**Arrange**:
- Entry A: id=1, similarity=0.90 (higher score → rank-0)
- Entry B: id=2, similarity=0.75 (lower score → rank-1, contradicts A)
- Entry C: id=3, similarity=0.65 (rank-2, no edges)
- `Contradicts` edge: source=1 (A), target=2 (B), `bootstrap_only=false`
- `TypedRelationGraph` built via `build_typed_relation_graph` (not `create_graph_edges_table`)
- `use_fallback = false`

**Act**: Invoke the Step 10b block logic (directly or via `SearchService::search` with
injected `TypedGraphState`):
```
result_ids = [1, 2, 3]
keep_mask = suppress_contradicts(&result_ids, &graph)
// apply mask to results_with_scores and final_scores[..aligned_len]
```

**Assert**:
- Result set contains entry with id=1 (A retained)
- Result set does NOT contain entry with id=2 (B suppressed)
- Result set contains entry with id=3 (C retained, unaffected)
- `results.len() == 2` (k=3 reduced to 2 by suppression; no backfill per FR-11)
- No panic; `final_scores` shadow is applied before ScoredEntry construction

**SR-07 compliance check**: `create_graph_edges_table` MUST NOT appear in this test or any
helper it calls. Code review: `grep "create_graph_edges_table" crates/unimatrix-server/src/services/search.rs` returns 0 matches in new test code.

---

### T-SC-09 — Floor + suppression combo (R-07, R-03)

**Risk coverage**: R-07 (`aligned_len` from wrong Vec), R-03 (`final_scores` shadow
correctness), AC-07 (partial verification)
**Scenario**: Four entries. One entry is removed by the Step 10 similarity floor; one is
suppressed by Step 10b. The test asserts both the correct surviving entry count and the
correct `final_score` values for the surviving entries.

**Arrange**:
- Entry A: id=1, similarity=0.90, final_score=F_A (rank-0 post-sort)
- Entry B: id=2, similarity=0.82, final_score=F_B (rank-1 — contradicts A → will be suppressed)
- Entry C: id=3, similarity=0.78, final_score=F_C (rank-2 — no edges)
- Entry D: id=4, similarity=0.45, final_score=F_D (rank-3 — will be removed by similarity_floor)
- `similarity_floor = 0.60` (removes D at Step 10)
- `Contradicts` edge: source=1 (A), target=2 (B), `bootstrap_only=false`
- `use_fallback = false`

**Pipeline execution sequence**:
1. Step 10: `results_with_scores.retain(|_, sim| *sim >= 0.60)` → removes D
   After Step 10: `results_with_scores = [(A, 0.90), (B, 0.82), (C, 0.78)]` (len=3)
   `final_scores` still len=4 (unfiltered): `[F_A, F_B, F_C, F_D]`
   `aligned_len = results_with_scores.len() = 3` (NOT 4)

2. Step 10b: `suppress_contradicts(&[1,2,3], &graph)` → mask = `[true, false, true]`
   Single-pass rebuild:
   - i=0 (A, keep): push A to new_rws, push F_A to new_fs
   - i=1 (B, drop): debug log; skip
   - i=2 (C, keep): push C to new_rws, push F_C to new_fs
   Shadow: `let final_scores = new_fs;` (len=2)
   `results_with_scores = [A, C]` (len=2)

3. Step 11: zip `[(A, F_A), (C, F_C)]`

**Assert**:
- `results.len() == 2` (D removed by floor, B suppressed; 2 survivors)
- `results[0].entry.id == 1` (A, rank-0)
- `results[1].entry.id == 3` (C, rank-2 after suppression)
- `results[0].final_score == F_A` — must be A's score, not B's score
- `results[1].final_score == F_C` — must be C's score, not D's score
- Entry id=2 (B) absent from results
- Entry id=4 (D) absent from results

**What this catches**:
- If `aligned_len = final_scores.len()` (=4) instead of `results_with_scores.len()` (=3),
  the zip in the single-pass produces 4 iterations but `results_with_scores` only has 3
  elements — index out of bounds panic, or (if zipped) wrong score assignments.
- If `final_scores` shadow is omitted, Step 11 zips `[A, C]` with the pre-suppression
  `final_scores[0..2]` = `[F_A, F_B]` — result would be A→F_A (correct) but C→F_B (wrong).
  The assertion `results[1].final_score == F_C` catches this silently-wrong scenario.

---

## Code Review Gates

| Check | Method |
|-------|--------|
| `if !use_fallback` guard is present and non-inverted | Code review Step 10b opener |
| `aligned_len = results_with_scores.len()` (not `final_scores.len()`) | Code review |
| Single `enumerate()` + `zip` loop (no separate `retain` calls) | Code review |
| `let final_scores = new_fs` shadow present (not `let mut final_scores` at line 893) | Code review line 893 and Step 10b |
| `debug!` contains both `suppressed_entry_id` and `contradicting_entry_id` | Code review |
| `create_graph_edges_table` absent from new test code | `grep "create_graph_edges_table" search.rs` → 0 new matches |
| `bootstrap_only: false` in `GraphEdgeRow` fixture | Code review test helper |
| Step 10b inserted after Step 10 `retain` blocks | Code review ordering |
| Step 10b inserted before Step 11 `ScoredEntry` construction | Code review ordering |

---

## AC-05: Cold-start regression

Existing cold-start tests in `search.rs` must continue to pass unmodified. The `if !use_fallback`
guard ensures Step 10b is skipped entirely when `use_fallback=true`. No new test needed for
this case — the regression is implicit in `cargo test --workspace` passing.

The guard form must be `if !use_fallback { ... }` not `if use_fallback { } else { ... }`.
An inverted guard (`if use_fallback`) would skip suppression in production and run it only
on cold-start (a natural no-op because the cold-start graph is empty). Code review is the
verification gate.

---

## Eval Gate (AC-06)

```bash
cargo run -p eval-runner -- --distribution_change false
```

This gate confirms MRR, P@K, and score distribution are unchanged. It DOES NOT validate
suppression correctness (all existing eval scenarios have no Contradicts edges). It is a
necessary but not sufficient gate. T-SC-08 is the mandatory additional positive gate (R-13).

---

## Risk Coverage Summary

| Risk | Covered By |
|------|-----------|
| R-03 (`final_scores` binding) | T-SC-09 assertion on `results[1].final_score == F_C` |
| R-07 (`aligned_len` wrong) | T-SC-09 (floors + suppression in same call) |
| R-10 (DEBUG log missing ID) | Code review: `debug!` field check |
| R-11 (cold-start guard) | AC-05 regression pass + code review |
| R-12 (wrong test helper) | T-SC-08 uses `build_typed_relation_graph` only; grep gate |
| R-13 (eval gate not sufficient) | T-SC-08 is explicitly listed as a separate mandatory gate |
