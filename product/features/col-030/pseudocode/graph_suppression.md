# Component: `graph_suppression.rs`

## Purpose

Pure function `suppress_contradicts` that, given an ordered slice of entry IDs (highest rank
first) and a `TypedRelationGraph`, returns a keep/drop bitmask and a parallel Vec of
contradicting-entry IDs. No I/O, no async, no mutable state. Directly unit-testable without
server infrastructure.

This module is a sibling to `graph.rs` (ADR-001). It is declared as a submodule of `graph.rs`
and re-exported from there; it does NOT appear in `lib.rs`.

---

## File Location

`crates/unimatrix-engine/src/graph_suppression.rs`

---

## Module Wiring (graph.rs — two lines only)

The implementation agent adds exactly these two lines to `graph.rs` at the top of the module
declarations section (after the existing `use` imports, before the penalty constants):

```
mod graph_suppression;
pub use graph_suppression::suppress_contradicts;
```

No other changes to `graph.rs`. No entry in `lib.rs`.

---

## Imports Required

```
use std::collections::HashSet;

use petgraph::Direction;

use crate::graph::{RelationType, TypedRelationGraph};
```

`petgraph::Direction` is available because `petgraph` is already a dependency of
`unimatrix-engine`. `crate::graph` is the parent module — this is the standard Rust
pattern for a submodule referencing its parent.

---

## Function Signature

```rust
pub fn suppress_contradicts(
    result_ids: &[u64],
    graph: &TypedRelationGraph,
) -> (Vec<bool>, Vec<Option<u64>>)
```

- `result_ids`: entry IDs in descending rank order (index 0 = highest ranked). Derived
  from `results_with_scores` by the caller. Never modified.
- `graph`: read-only reference to the pre-built typed graph. Never modified.
- Returns `(keep_mask, contradicting_ids)` where both Vecs have length `result_ids.len()`.
  - `keep_mask[i] = true` means entry at index `i` is retained.
  - `keep_mask[i] = false` means entry at index `i` is suppressed.
  - `contradicting_ids[i] = Some(id)` when `keep_mask[i] = false`; `id` is the ID of
    the lowest-index (highest-ranked) surviving entry that contradicts entry `i`.
  - `contradicting_ids[i] = None` when `keep_mask[i] = true`.

Visibility MUST be `pub fn` (not `pub(super)` or private). `pub(super)` compiles within
the module but causes E0364/E0365 at the `pub use` re-export in `graph.rs` (R-02).

---

## Algorithm

```
FUNCTION suppress_contradicts(result_ids, graph):

  n = result_ids.len()
  keep_mask         = Vec::with_capacity(n)  -- initialized to all true
  contradicting_ids = Vec::with_capacity(n)  -- initialized to all None

  FOR i in 0..n:
    keep_mask.push(true)
    contradicting_ids.push(None)

  -- Outer loop: for each surviving higher-ranked entry, collect its Contradicts neighbors
  FOR i in 0..n:
    IF keep_mask[i] == false:
      CONTINUE  -- already suppressed; do not use as a suppressor

    entry_id = result_ids[i]

    -- Resolve node index; skip if entry not in graph (e.g. stored after last tick rebuild)
    node_idx = match graph.node_index.get(&entry_id):
      None  => CONTINUE  -- entry not in graph; cannot have Contradicts edges
      Some(idx) => idx

    -- Query Outgoing Contradicts edges from this node (ADR-002, ADR-003)
    -- edges_of_type is the sole traversal boundary (SR-01); no direct .edges_directed() calls
    outgoing_neighbors: HashSet<u64> = graph
      .edges_of_type(node_idx, RelationType::Contradicts, Direction::Outgoing)
      .map(|edge_ref| graph.inner[edge_ref.target()])   -- dereference node weight (u64)
      .collect()

    -- Query Incoming Contradicts edges to this node (ADR-003 — unidirectional NLI writes)
    incoming_neighbors: HashSet<u64> = graph
      .edges_of_type(node_idx, RelationType::Contradicts, Direction::Incoming)
      .map(|edge_ref| graph.inner[edge_ref.source()])   -- dereference node weight (u64)
      .collect()

    -- Union of both directions (HashSet union is idempotent if NLI ever adds reverse edges)
    contradicts_neighbors: HashSet<u64> = outgoing_neighbors
      .union(&incoming_neighbors)
      .copied()
      .collect()

    IF contradicts_neighbors.is_empty():
      CONTINUE

    -- Inner loop: suppress any lower-ranked entry whose ID is in contradicts_neighbors
    FOR j in (i+1)..n:
      IF keep_mask[j] == false:
        CONTINUE  -- already suppressed by an earlier entry; skip

      IF contradicts_neighbors.contains(&result_ids[j]):
        keep_mask[j]         = false
        contradicting_ids[j] = Some(entry_id)
        -- Do NOT break; one higher-ranked entry may contradict multiple lower-ranked entries

  RETURN (keep_mask, contradicting_ids)
```

### Key correctness properties

1. The outer loop only processes entries where `keep_mask[i] == true`. An already-suppressed
   entry is not used to trigger further suppressions. This is correct: if rank-2 is suppressed
   by rank-0, rank-2 should not be able to suppress rank-3 (chain suppression via a suppressed
   node is incorrect). See the chain edge-case in RISK-TEST-STRATEGY.md.

   Clarification on "chain suppression" test case from ARCHITECTURE.md (test case 4):
   "rank-0 contradicts rank-2, rank-2 contradicts rank-3 → both suppressed."
   - rank-2 is suppressed by rank-0 (outer loop i=0).
   - rank-3 is suppressed by rank-2's direct Contradicts edge to rank-3. BUT per this
     algorithm, when i=2 we check `keep_mask[2] == false` and CONTINUE — rank-3 is NOT
     suppressed via rank-2.
   - Therefore the expected result for that test case is `[true, true, false, true]` NOT
     `[true, true, false, false]`.

   CONFLICT DETECTED: ARCHITECTURE.md test case 4 says `[true, true, false, false]` (both
   suppressed) but this algorithm produces `[true, true, false, true]` (only rank-2 suppressed
   via rank-0; rank-3 NOT suppressed because suppressed nodes do not propagate suppression).

   FLAG FOR ARCHITECT REVIEW: The algorithm must choose one of:
   (A) Suppressed nodes do NOT propagate — `keep_mask[i]==false` → skip in outer loop.
       Result for chain: `[true, true, false, true]`. Simpler, no transitive suppression.
   (B) Suppressed nodes DO propagate — outer loop processes all i regardless of keep_mask[i].
       Result for chain: `[true, true, false, false]`. Matches ARCHITECTURE.md test case 4.

   The pseudocode below implements option (B) to match the documented test expectation, but
   the gap is flagged here explicitly. The implementation agent must not silently choose
   option (A).

   REVISED ALGORITHM (option B — matches ARCHITECTURE.md test case 4):

```
FUNCTION suppress_contradicts(result_ids, graph):

  n = result_ids.len()
  keep_mask         = vec![true; n]
  contradicting_ids = vec![None; n]

  FOR i in 0..n:
    -- NOTE: outer loop processes ALL entries, including already-suppressed ones.
    -- A suppressed entry at index i still propagates its Contradicts edges to lower-ranked
    -- entries. This matches ARCHITECTURE.md test case 4 (chain suppression).

    entry_id = result_ids[i]

    node_idx = match graph.node_index.get(&entry_id):
      None  => CONTINUE
      Some(idx) => idx

    outgoing_neighbors: HashSet<u64> = graph
      .edges_of_type(node_idx, RelationType::Contradicts, Direction::Outgoing)
      .map(|e| graph.inner[e.target()])
      .collect()

    incoming_neighbors: HashSet<u64> = graph
      .edges_of_type(node_idx, RelationType::Contradicts, Direction::Incoming)
      .map(|e| graph.inner[e.source()])
      .collect()

    contradicts_neighbors: HashSet<u64> = outgoing_neighbors
      .union(&incoming_neighbors)
      .copied()
      .collect()

    IF contradicts_neighbors.is_empty():
      CONTINUE

    FOR j in (i+1)..n:
      IF keep_mask[j] == true AND contradicts_neighbors.contains(&result_ids[j]):
        keep_mask[j]         = false
        contradicting_ids[j] = Some(entry_id)
        -- contradicting_id set to i's entry_id (the entry triggering suppression)
        -- even if i itself was suppressed by an earlier entry

  RETURN (keep_mask, contradicting_ids)
```

2. The `None` / `Some(id)` in `contradicting_ids` is set on first match only — once
   `keep_mask[j]` is set `false`, the inner loop skips further checks on `j` (the
   `keep_mask[j] == true` guard in the inner loop). The stored `contradicting_id` is
   therefore the lowest-index (highest-ranked) suppressor for that entry.

3. `node_index.get()` returning `None` is a valid production scenario: the entry was stored
   after the last background tick completed. The function skips traversal for that entry
   (cannot have graph edges in this snapshot) and moves on. No panic.

4. The function allocates exactly one `HashSet` per outer iteration (or two, unioned). For
   k ≤ 20 with Contradicts degree < 3, this is 120 edge lookups and ~20 HashSet allocations
   per call — negligible on the hot path.

---

## Error Handling

This function is pure with no fallible operations. It cannot return an error.

The only potential panic site is `graph.inner[edge_ref.target()]` and `graph.inner[edge_ref.source()]`
— these index into the petgraph node weight by NodeIndex. Since `edges_of_type` only yields
edges that exist in `inner`, and nodes are never removed from `TypedRelationGraph` once added
(no removal path exists), these accesses are always valid. No explicit error handling needed.

---

## Key Test Scenarios

All unit tests live in `graph_suppression.rs` under `#[cfg(test)]`. NOT in `graph_tests.rs`
(that file is 1,068 lines — R-01).

Tests use `build_typed_relation_graph` with hand-constructed `GraphEdgeRow` slices and
`EntryRecord` stubs. All seeded edges must use `bootstrap_only: false`.

### Test 1: Empty result set
- `result_ids = []`, any graph
- Expected: `(vec![], vec![])`
- Validates: no panic on empty input (R-06)

### Test 2: No Contradicts edges in graph (graph has entries but no edges)
- `result_ids = [1, 2, 3]`, graph has nodes but no Contradicts edges
- Expected: `(vec![true, true, true], vec![None, None, None])`
- Validates: no false suppression from non-Contradicts edges

### Test 3: Outgoing Contradicts rank-0 → rank-1
- Graph edge: `source_id=1, target_id=2, relation_type="Contradicts"`
- `result_ids = [1, 2]`
- Expected: `(vec![true, false], vec![None, Some(1)])`
- Validates: basic suppression, AC-02

### Test 4: Incoming Contradicts (edge written rank-1 → rank-0, i.e. Incoming to rank-0)
- Graph edge: `source_id=2, target_id=1, relation_type="Contradicts"`
- `result_ids = [1, 2]`
- Expected: `(vec![true, false], vec![None, Some(1)])`
- Validates: bidirectional check (ADR-003, R-05, AC-03)
- This is the critical test — an Outgoing-only implementation passes test 3 but fails here

### Test 5: Outgoing Contradicts rank-0 → rank-3 (non-adjacent)
- Graph edge: `source_id=1, target_id=4, relation_type="Contradicts"`
- `result_ids = [1, 2, 3, 4]`
- Expected: `(vec![true, true, true, false], vec![None, None, None, Some(1)])`
- Validates: suppression skips intervening entries, AC-02 variant

### Test 6: Chain suppression (rank-0 contradicts rank-2; rank-2 contradicts rank-3)
- Graph edges: `(1→3, "Contradicts")` and `(3→4, "Contradicts")`
- `result_ids = [1, 2, 3, 4]`
- Expected: `(vec![true, true, false, false], vec![None, None, Some(1), Some(3)])`
- Validates: option B propagation — suppressed node at index 2 still propagates to index 3

### Test 7: Non-Contradicts edges only
- Graph edges: `(1→2, "CoAccess")`, `(2→3, "Supports")`, `(3→4, "Supersedes")`
- `result_ids = [1, 2, 3, 4]`
- Expected: `(vec![true, true, true, true], vec![None, None, None, None])`
- Validates: FR-04 (only Contradicts edges trigger suppression), AC-04

### Test 8: mask length invariant (AC-01)
- For every test case: assert `keep_mask.len() == result_ids.len()`
- For every test case: assert `contradicting_ids.len() == result_ids.len()`
- Validates: R-06 (no out-of-bounds in caller's indexed loop)
