# col-030: Contradicts Collision Suppression — Pseudocode Overview

## Components Involved

| Component | File | Role |
|-----------|------|------|
| `suppress_contradicts` | `crates/unimatrix-engine/src/graph_suppression.rs` | Pure function: computes keep/drop bitmask from ranked result IDs and the typed graph |
| Step 10b insertion | `crates/unimatrix-server/src/services/search.rs` | Call site: applies the bitmask to both parallel Vecs in a single indexed pass |
| `graph.rs` wiring | `crates/unimatrix-engine/src/graph.rs` | Two-line addition only: `mod graph_suppression; pub use graph_suppression::suppress_contradicts;` |

## Data Flow

```
SearchService::search (search.rs)
  |
  | Step 6: read lock clone
  |   typed_graph: TypedRelationGraph   -- contains Contradicts edges from NLI
  |   use_fallback: bool                -- true until first background tick completes
  |
  | Step 10: floor retain calls
  |   results_with_scores: Vec<(EntryRecord, f64)>   -- floor-filtered, sorted DESC by final_score
  |   final_scores: Vec<f64>                          -- NOT floor-filtered; may be longer
  |
  | Step 10b [NEW — col-030]:
  |   result_ids: Vec<u64>              -- extracted from results_with_scores, rank order
  |   keep_mask: Vec<bool>             -- produced by suppress_contradicts()
  |   contradicting_ids: Vec<Option<u64>>  -- produced alongside keep_mask (one per slot)
  |     |
  |     v
  |   suppress_contradicts(result_ids, &typed_graph)
  |     --> for each pair (i < j): query Outgoing + Incoming Contradicts edges from i
  |     --> if j's ID is in i's Contradicts neighbors: keep_mask[j] = false
  |     --> returns (Vec<bool>, Vec<Option<u64>>) [see Shared Types below]
  |     |
  |   single indexed pass over zip(results_with_scores, final_scores[..aligned_len])
  |     --> new_rws: Vec<(EntryRecord, f64)>   -- kept entries only
  |     --> new_fs:  Vec<f64>                  -- kept scores only
  |     --> debug! log per suppressed entry (suppressed_entry_id, contradicting_entry_id)
  |   results_with_scores = new_rws         -- reassign (mut)
  |   let final_scores = new_fs             -- SHADOW (not let mut at line 893)
  |
  | Step 11: zip(results_with_scores, final_scores) --> Vec<ScoredEntry>
```

## Shared Types (no new types introduced)

All types are existing. No new structs or enums are added by col-030.

| Type | Defined In | Used By |
|------|-----------|---------|
| `TypedRelationGraph` | `unimatrix-engine/src/graph.rs` | Both components (read-only in suppression) |
| `RelationType::Contradicts` | `unimatrix-engine/src/graph.rs` | `suppress_contradicts` (filter argument) |
| `petgraph::Direction` | `petgraph` crate | `suppress_contradicts` (Outgoing + Incoming) |
| `NodeIndex` | `petgraph` crate | `suppress_contradicts` (graph lookup) |
| `EntryRecord` | `unimatrix-core` | Step 10b (cloned into new_rws) |

### Return type decision: `(Vec<bool>, Vec<Option<u64>>)`

`suppress_contradicts` returns a tuple:
- `Vec<bool>`: keep/drop mask, length == `result_ids.len()` (true = keep, false = suppress)
- `Vec<Option<u64>>`: for each suppressed slot (`false`), the ID of the highest-ranked surviving
  entry that contradicts it; `None` for kept entries

This satisfies FR-09 / NFR-05 (both IDs in the debug log) without requiring the caller to
re-derive the contradicting ID. The caller destructures in the indexed pass.

## Sequencing Constraints

1. `graph_suppression.rs` must be written first (it has no dependency on search.rs).
2. `graph.rs` wiring (`mod` + `pub use`) is a two-line edit that can be done alongside step 1.
3. Step 10b insertion in `search.rs` depends on `suppress_contradicts` being importable via
   `unimatrix_engine::graph::suppress_contradicts` (the re-export path).
4. Unit tests in `graph_suppression.rs` are self-contained (no server infrastructure).
5. Integration test in `search.rs` depends on both components being complete.

## File-Size Budget

| File | Current Lines | Change | Post-change |
|------|--------------|--------|-------------|
| `graph_suppression.rs` | 0 (new) | ~120 lines (function + 8 unit tests) | ~120 |
| `graph.rs` | 587 | +2 lines | 589 |
| `search.rs` | ~2700+ | +~25 lines (Step 10b block) | within limit |

All files remain under the 500-line limit for new/modified files. `graph_suppression.rs`
starts fresh and has no pre-existing content.
