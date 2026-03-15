# Agent Report: 286-agent-1-fix

**Feature**: bugfix-286
**Agent ID**: 286-agent-1-fix
**Task**: Fix `get_embedding` to search all HNSW layers (GH#286)

---

## Files Modified

- `crates/unimatrix-vector/src/index.rs` — `get_embedding` method and two new tests

---

## Fix Summary

**Root cause**: `get_embedding` called `point_indexation.get_layer_iterator(0)`, which only iterates `points_by_layer[0]`. In hnsw_rs, each point is placed at insertion time into `points_by_layer[L]` where L is its randomly assigned level — with probability ~1/16 per level above 0 (M=16). A point at level ≥ 1 never appears in layer 0.

**Fix**: Changed `point_indexation.get_layer_iterator(0)` to `for point in point_indexation` — the `IntoIterator for &PointIndexation` impl returns `IterPoint`, which traverses all layers from 0 through `entry_point_level`, visiting every inserted point exactly once.

**Change is minimal**: 1 line changed in `get_embedding` body. Surrounding logic unchanged. Updated doc comment to explain the correct invariant and cite GH#286.

---

## New Tests

| Test function | What it verifies |
|---|---|
| `test_get_embedding_returns_some_for_all_points_regardless_of_layer` | Inserts 200 points so the RNG almost certainly assigns at least one to level ≥ 1 (P(all at layer 0) < 10⁻⁶), then asserts `get_embedding` returns `Some(_)` for every entry ID. Fails deterministically with the old code. |
| `test_get_embedding_value_matches_inserted_vector` | Inserts 50 entries, retrieves each embedding, verifies dimension and dot-product ≥ 0.99 (round-trip fidelity for unit vectors). |

---

## Test Results

```
running 106 tests
... (all pass)
test result: ok. 106 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.49s
```

- Before fix: 104 tests existed (new 2 added)
- After fix: 106/106 pass
- `cargo clippy -p unimatrix-vector -- -D warnings`: clean (no warnings in our crate)
- `cargo build --workspace`: clean (no errors)

---

## Issues / Blockers

None.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-vector hnsw layer iteration get_embedding` — found existing lesson-learned entry #1712 confirming the diagnosis, and no conflicting patterns.
- Stored: entry #1724 "get_embedding: use IntoIterator for &PointIndexation (all layers), not get_layer_iterator(0)" via `/uni-store-pattern` — captures the correct vs. wrong iteration pattern as a reusable guard for any future hnsw_rs point-scanning code in this crate.
