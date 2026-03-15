# Bug Investigation Report: 286-investigator

## Bug Summary

`test_search_multihop_injects_terminal_active` fails intermittently when run as part of the full lifecycle suite. The search returns only entry IDs [1, 2] (A and B, both deprecated), omitting C (id=3, the active terminal). The test passes in isolation because the same failure code path is not exercised when C is found directly by HNSW.

## Root Cause Analysis

Two independent mechanisms must hold simultaneously to produce the failure.

### Mechanism 1: `use_fallback = true` disables multi-hop traversal (always true in tests)

`SupersessionState` initializes with `use_fallback: true` on cold-start (`supersession.rs:61`). The background tick (900s interval) is the sole writer that sets `use_fallback: false`. The first tick is always skipped at startup (`background.rs:293-294`):

```rust
// Skip the immediate first tick (fires at t=0).
interval.tick().await;
loop {
    interval.tick().await;  // first real tick fires after 900 seconds
```

Since tests complete in seconds, `use_fallback` remains `true` for the full test duration. In the search pipeline (`search.rs:368-370`), single-hop injection is used:

```rust
let terminal_id: Option<u64> = if use_fallback {
    entry.superseded_by  // single-hop only — does NOT follow A→B→C to C
}
```

When HNSW returns A and B (both superseded), the injection loop processes:
- A.superseded_by = B → terminal_id = B → B already in result set → skip
- B.superseded_by = C → terminal_id = C → C not in result set → proceed to inject

So far C would still be injected. Mechanism 2 prevents it.

### Mechanism 2: `get_embedding` cannot find C when C is at HNSW layer > 0 (probabilistic, ~6.25%)

After resolving the terminal, the injection path calls `get_embedding(C_id)` (`search.rs:400`):

```rust
if let Some(emb) = self.vector_store.get_embedding(terminal_id).await {
    let sim = cosine_similarity(&embedding, &emb);
    results_with_scores.push((terminal, sim));
}
// If no embedding: skip injection (existing R-01 fallback pattern)
```

`get_embedding` is implemented in `index.rs:304-321`:

```rust
pub fn get_embedding(&self, entry_id: u64) -> Option<Vec<f32>> {
    let data_id = { ... };
    let hnsw = self.hnsw.read()...;
    let point_indexation = hnsw.get_point_indexation();
    // Iterate layer 0 (base layer — contains all points) <-- WRONG COMMENT
    for point in point_indexation.get_layer_iterator(0) {
        if point.get_origin_id() == data_id as usize {
            return Some(point.get_v().to_vec());
        }
    }
    None
}
```

The comment "base layer — contains all points" is **incorrect** for hnsw_rs. In hnsw_rs, each point is physically stored in `points_by_layer[L]` where `L` is its **randomly assigned** insertion level — NOT necessarily layer 0. Source: hnsw_rs 0.3.3 `generate_new_point` (`hnsw.rs:498-526`):

```rust
let level = self.layer_g.generate();  // random level via StdRng::from_os_rng()
let mut p_id = PointId(level as u8, -1);
points_by_layer_ref[p_id.0 as usize].push(Arc::clone(&new_point));
// stored ONLY in points_by_layer[level], never in points_by_layer[0..level-1]
```

Level assignment probability for `max_nb_connection=16`:
- `P(level = 0) ≈ 93.75%`
- `P(level >= 1) ≈ 6.25%`

When C is assigned level >= 1, `get_layer_iterator(0)` iterates only `points_by_layer[0]` and does not find C. `get_embedding` returns `None`, injection is silently skipped, and C never appears in results.

Note: HNSW SEARCH can still find C (neighbor pointers are bidirectional across all layers, so search traversal from entry point can reach C even if C is not in `points_by_layer[0]`). Only `get_layer_iterator(0)` is blind to C.

### Code Path Trace

```
context_search (MCP call)
  → UnimatrixServer::context_search (tools.rs)
    → SearchService::search (search.rs)
      → Step 5: vector_store.search(query, k=5, ef=32)
          → hnsw_rs::Hnsw::search()  [may return [A, B] without C]
      → Step 6: quarantine filter (none filtered — A, B both Deprecated not Quarantined)
      → GH#264 fix: read supersession_state (use_fallback=true, all_entries=[])
      → Step 6b: supersession injection loop
          → use_fallback=true → single-hop: terminal_id = entry.superseded_by
          → For A (superseded_by=B): B in existing_ids → skip
          → For B (superseded_by=C): C not in existing_ids → proceed
          → entry_store.get(C_id) → C record (Active, no superseded_by → valid)
          → vector_store.get_embedding(C_id)
              → VectorIndex::get_embedding (index.rs:304)
                → id_map.entry_to_data.get(C_id) → Some(data_id=2)
                → get_layer_iterator(0)
                    → iterates points_by_layer[0] only
                    → C is in points_by_layer[1] (if level=1 was assigned at insert)
                    → C NOT FOUND
                → returns None                  ← ROOT CAUSE
          → None → injection silently skipped
      → Step 9: truncate to k=5; results=[A, B]
      → C absent from results → test asserts id_c in result_ids → FAIL
```

### Why Isolation Usually Passes

When HNSW returns all 3 entries (A, B, C) — the most common case with only 3 points and ef_search=32 — C is already in `existing_ids` and injection is skipped entirely. The `get_embedding` bug is never triggered. The test passes.

The failure only manifests when BOTH:
1. HNSW greedy walk happens not to return C (possible with sparse 3-node graph and embedding geometry)
2. C was assigned layer > 0 at insert time (~6.25% probability)

Combined failure probability: low enough (~1-3%) that isolated runs almost always pass but it surfaces across many runs or CI sessions.

## Affected Files and Functions

| File | Function | Role in Bug |
|------|----------|-------------|
| `crates/unimatrix-vector/src/index.rs` | `VectorIndex::get_embedding` | Iterates `points_by_layer[0]` only — incorrect comment claims this is "all points"; root cause |
| `crates/unimatrix-server/src/services/search.rs` | `SearchService::search` (Step 6b, line ~400) | Silent None skip means C is never injected when `get_embedding` fails |
| `crates/unimatrix-server/src/services/supersession.rs` | `SupersessionState::new` | Cold-start `use_fallback: true` forces single-hop injection (enabling code path that exercises the bug) |
| `crates/unimatrix-server/src/background.rs` | `spawn_background_tick` loop (line 293) | Skips first tick; 900s before first real tick — `use_fallback` never becomes false in tests |

## Proposed Fix Approach

**Fix `VectorIndex::get_embedding` to iterate all layers, not just layer 0.**

In `/workspaces/unimatrix/crates/unimatrix-vector/src/index.rs`, change `get_embedding` to use `IterPoint` (full iteration via `IntoIterator for &PointIndexation`) instead of `get_layer_iterator(0)`:

```rust
pub fn get_embedding(&self, entry_id: u64) -> Option<Vec<f32>> {
    let data_id = {
        let id_map = self.id_map.read().unwrap_or_else(|e| e.into_inner());
        id_map.entry_to_data.get(&entry_id).copied()?
    };
    let hnsw = self.hnsw.read().unwrap_or_else(|e| e.into_inner());
    let point_indexation = hnsw.get_point_indexation();
    // Iterate ALL layers: hnsw_rs stores each point in points_by_layer[L] where L
    // is its assigned insertion level (probabilistic, NOT always layer 0).
    // IterPoint via IntoIterator covers all layers from 0 upward.
    for point in point_indexation {
        if point.get_origin_id() == data_id as usize {
            return Some(point.get_v().to_vec());
        }
    }
    None
}
```

Also update the comment on the preceding line to remove the false claim about "base layer contains all points."

### Why This Fix

The bug is a wrong assumption about hnsw_rs internal storage. The fix is local to one function (4 lines changed), corrects the assumption, and uses existing hnsw_rs API (`IntoIterator for &PointIndexation` / `IterPoint`). No changes to the injection logic, supersession state, or background tick are needed.

## Risk Assessment

- **Blast radius**: `VectorIndex::get_embedding` is called only from `SearchService::search` (search.rs:400) in the supersession injection path. No other callers. Confirmed via grep.
- **Regression risk**: Low. The fix makes `get_embedding` return `Some(embedding)` for entries at layer > 0, which was the intended behavior. The only behavioral change is correct injection for those entries.
- **Performance**: `IterPoint` is O(n) across all layers, same asymptotic as the current layer-0-only scan. With typical DB sizes (< 10,000 entries), this is negligible.
- **Confidence**: High. The bug is deterministic once both conditions are met. The hnsw_rs source code confirms the storage model.

## Missing Test

A unit test for `VectorIndex::get_embedding` that verifies entries at layer > 0 are found. The simplest approach: insert enough entries that statistically some land at layer > 0, then assert `get_embedding` returns `Some` for all of them.

Test scenario (to add to `index.rs` test module):
```rust
#[test]
fn test_get_embedding_returns_some_for_all_entries_regardless_of_layer() {
    // Insert 50 entries. With P(layer>0) ≈ 6.25%, ~3 will be at layer > 0.
    // All must return Some from get_embedding.
    let tvi = TestVectorIndex::new();
    let mut inserted_ids = vec![];
    for i in 0..50u64 {
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(i + 1, &emb).unwrap();
        inserted_ids.push(i + 1);
    }
    for entry_id in inserted_ids {
        let result = tvi.vi().get_embedding(entry_id);
        assert!(
            result.is_some(),
            "get_embedding must return Some for entry_id={entry_id} regardless of HNSW layer assignment"
        );
    }
}
```

This test would have failed pre-fix and caught the bug.

## Reproduction Scenario

**Deterministic**: Patch `LayerGenerator::generate()` (hnsw_rs) to return 1 for the third insertion (entry C). Then run the test — `get_embedding(C_id)` returns `None`, injection fails, test fails every time.

**Probabilistic** (as observed): Run the full lifecycle suite many times. The test fails approximately when C's HNSW layer assignment is > 0 AND the greedy HNSW search misses C. Combined probability ~1-3% per run.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-vector` — found #1603 (ADR-003 multi-hop traversal, crt-014), #748 (TestHarness pattern), #483 (superseded single-hop ADR). None covered this specific hnsw_rs storage layout bug.
- Queried: `/uni-knowledge-search` for "integration test fixture isolation server fixture flaky" — found test infrastructure conventions, no prior lesson about hnsw_rs layer assignment.
- Stored: entry #1712 "hnsw_rs: points stored only at assigned layer, not at layer 0 — get_layer_iterator(0) misses ~6% of points" via `/uni-store-lesson`. Tagged `caused_by_feature:crt-014` (crt-014 introduced `get_embedding` for injection; the bug was in that implementation).
