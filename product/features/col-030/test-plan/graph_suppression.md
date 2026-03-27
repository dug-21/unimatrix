# Component Test Plan: `graph_suppression.rs`

## Component Summary

**File**: `crates/unimatrix-engine/src/graph_suppression.rs`
**Function under test**: `pub fn suppress_contradicts(result_ids: &[u64], graph: &TypedRelationGraph) -> (Vec<bool>, Vec<Option<u64>>)`
**Test location**: inline `#[cfg(test)]` in `graph_suppression.rs` (NOT `graph_tests.rs`)
**Critical Trap**: `graph_tests.rs` is 1,068 lines. Any addition causes gate-3b rejection (R-01).

---

## Test Infrastructure Notes

Each test builds a `TypedRelationGraph` using `build_typed_relation_graph(&entries, &edges)`.

**Local helper pattern** — `graph_tests.rs` helpers (`make_entry`, `make_edge_row`) are not
accessible from `graph_suppression.rs`. The `#[cfg(test)]` block must define its own local
helpers:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use petgraph::Direction;
    use unimatrix_core::{EntryRecord, Status};
    use crate::graph::{GraphEdgeRow, RelationType, TypedRelationGraph, build_typed_relation_graph};

    fn make_entry(id: u64) -> EntryRecord { /* minimal EntryRecord, status=Active */ }

    fn contradicts_edge(source_id: u64, target_id: u64) -> GraphEdgeRow {
        GraphEdgeRow {
            source_id,
            target_id,
            relation_type: RelationType::Contradicts.as_str().to_string(),
            weight: 1.0,
            created_at: 0,
            created_by: "test".to_string(),
            source: "nli".to_string(),
            bootstrap_only: false,  // MUST be false — bootstrap_only=true is excluded by build_typed_relation_graph
        }
    }

    fn edge_row(source_id: u64, target_id: u64, rel: RelationType) -> GraphEdgeRow {
        GraphEdgeRow {
            source_id,
            target_id,
            relation_type: rel.as_str().to_string(),
            weight: 1.0,
            created_at: 0,
            created_by: "test".to_string(),
            source: "test".to_string(),
            bootstrap_only: false,
        }
    }
}
```

**bootstrap_only trap**: edges with `bootstrap_only=true` are excluded by `build_typed_relation_graph`
Pass 2b. All test edges MUST use `bootstrap_only: false` (R-12 equivalent at unit level).

---

## Test Cases

### T-GS-01 — Empty graph, all entries kept
**Risk coverage**: R-06 (mask length invariant), AC-01
**Scenario**: Empty `TypedRelationGraph` (no entries, no edges); 3 result IDs passed in.
**Arrange**:
```rust
let graph = build_typed_relation_graph(&[], &[]).unwrap();
let result_ids = vec![10u64, 20u64, 30u64];
```
**Act**: `let (mask, cids) = suppress_contradicts(&result_ids, &graph);`
**Assert**:
- `mask.len() == 3` — length invariant holds even when entries absent from graph
- `mask == vec![true, true, true]`
- `cids.len() == 3`
- `cids == vec![None, None, None]`

**Also assert**: `suppress_contradicts(&[], &graph)` returns `(vec![], vec![])` (empty input no-panic).

---

### T-GS-02 — Outgoing Contradicts rank-0 → rank-1
**Risk coverage**: R-04 (string match correctness), AC-02
**Scenario**: Two entries; edge written from rank-0 to rank-1. Rank-1 must be suppressed.
**Arrange**:
```rust
let entries = vec![make_entry(1), make_entry(2)];
let edges = vec![contradicts_edge(1, 2)];  // source=1(rank-0), target=2(rank-1)
let graph = build_typed_relation_graph(&entries, &edges).unwrap();
let result_ids = vec![1u64, 2u64];  // rank-0 first
```
**Act**: `let (mask, cids) = suppress_contradicts(&result_ids, &graph);`
**Assert**:
- `mask.len() == 2`
- `mask[0] == true`  (rank-0 retained)
- `mask[1] == false` (rank-1 suppressed)
- `cids.len() == 2`
- `cids[0] == None`    (rank-0 not suppressed)
- `cids[1] == Some(1)` (rank-1 suppressed by entry id=1)

**Note**: `contradicts_edge` uses `RelationType::Contradicts.as_str()` which produces
`"Contradicts"`. This confirms `edges_of_type` string comparison works for this edge type
(first-ever test of `edges_of_type` with `RelationType::Contradicts`).

---

### T-GS-03 — Outgoing Contradicts rank-0 → rank-3 (non-adjacent)
**Risk coverage**: FR-02 (all pairs checked, not just adjacent), AC-01
**Scenario**: Four entries; only rank-0 and rank-3 connected. Rank-1 and rank-2 unaffected.
**Arrange**:
```rust
let entries = vec![make_entry(1), make_entry(2), make_entry(3), make_entry(4)];
let edges = vec![contradicts_edge(1, 4)];  // rank-0→rank-3
let graph = build_typed_relation_graph(&entries, &edges).unwrap();
let result_ids = vec![1u64, 2u64, 3u64, 4u64];
```
**Act**: `let (mask, cids) = suppress_contradicts(&result_ids, &graph);`
**Assert**:
- `mask.len() == 4`
- `mask == vec![true, true, true, false]`
- `cids.len() == 4`
- `cids[0] == None`    (rank-0 not suppressed)
- `cids[1] == None`    (rank-1 not suppressed)
- `cids[2] == None`    (rank-2 not suppressed)
- `cids[3] == Some(1)` (rank-3 suppressed by entry id=1)

---

### T-GS-04 — Chain: rank-0 contradicts rank-2; rank-2 contradicts rank-3
**Risk coverage**: FR-02 (transitive suppression via rank order sweep), RISK-TEST-STRATEGY edge case (chain suppression)
**Scenario**: Rank-0 contradicts rank-2. Rank-2 also contradicts rank-3. Rank-1 unaffected.
Expected: rank-2 suppressed by rank-0; rank-3 suppressed by rank-2 (already-suppressed entries
can still trigger suppression of lower entries they contradict — the sweep processes in rank
order over the original candidate list).
**Arrange**:
```rust
let entries = vec![make_entry(1), make_entry(2), make_entry(3), make_entry(4)];
let edges = vec![
    contradicts_edge(1, 3),  // rank-0→rank-2
    contradicts_edge(3, 4),  // rank-2→rank-3
];
let graph = build_typed_relation_graph(&entries, &edges).unwrap();
let result_ids = vec![1u64, 2u64, 3u64, 4u64];
```
**Act**: `let (mask, cids) = suppress_contradicts(&result_ids, &graph);`
**Assert**:
- `mask.len() == 4`
- `mask == vec![true, true, false, false]`
- `cids.len() == 4`
- `cids[0] == None`    (rank-0 not suppressed)
- `cids[1] == None`    (rank-1 not suppressed)
- `cids[2] == Some(1)` (rank-2 suppressed by entry id=1)
- `cids[3] == Some(3)` (rank-3 suppressed by entry id=3, which propagates even though rank-2 is itself suppressed — Option B)

**Note on chain semantics**: The expected output depends on whether the implementation sweeps
each pair (i, j) where i < j checking if any surviving rank-0..i-1 entry contradicts rank-j,
OR if it propagates through already-suppressed nodes. The correct output per the algorithm
spec is `[true, true, false, false]` — rank-3 is suppressed because rank-2 contradicts it,
even though rank-2 is itself suppressed. This test validates that interpretation.

---

### T-GS-05 — Non-Contradicts edges only: all entries kept
**Risk coverage**: FR-04 (edge type discrimination), AC-04
**Scenario**: Entries connected by `CoAccess`, `Supports`, and `Supersedes`. None suppressed.
**Arrange**:
```rust
let entries = vec![make_entry(1), make_entry(2), make_entry(3)];
let edges = vec![
    edge_row(1, 2, RelationType::CoAccess),
    edge_row(2, 1, RelationType::CoAccess),  // CoAccess is bidirectional in practice
    edge_row(1, 3, RelationType::Supports),
];
let graph = build_typed_relation_graph(&entries, &edges).unwrap();
let result_ids = vec![1u64, 2u64, 3u64];
```
**Act**: `let (mask, cids) = suppress_contradicts(&result_ids, &graph);`
**Assert**:
- `mask.len() == 3`
- `mask == vec![true, true, true]`
- `cids.len() == 3`
- `cids == vec![None, None, None]`

---

### T-GS-06 — Incoming direction: edge written rank-1 → rank-0 (MANDATORY, R-05, AC-03)
**Risk coverage**: R-05 (bidirectional query — the most critical test), ADR-003, AC-03
**Scenario**: Two entries; NLI happened to write the Contradicts edge from the lower-ranked
entry to the higher-ranked entry. Rank-1 must still be suppressed (Incoming from rank-0's
perspective).
**Arrange**:
```rust
let entries = vec![make_entry(1), make_entry(2)];
let edges = vec![contradicts_edge(2, 1)];  // source=2(rank-1), target=1(rank-0) — INCOMING from rank-0
let graph = build_typed_relation_graph(&entries, &edges).unwrap();
let result_ids = vec![1u64, 2u64];  // rank-0=id 1, rank-1=id 2
```
**Act**: `let (mask, cids) = suppress_contradicts(&result_ids, &graph);`
**Assert**:
- `mask.len() == 2`
- `mask[0] == true`  (rank-0 retained)
- `mask[1] == false` (rank-1 suppressed even though edge is Outgoing from rank-1)
- `cids.len() == 2`
- `cids[0] == None`    (rank-0 not suppressed)
- `cids[1] == Some(1)` (rank-1 suppressed by entry id=1, detected via Incoming direction query)

**This test catches an Outgoing-only implementation.** An implementation that only queries
`Direction::Outgoing` from each node will return `[true, true]` (seeing no outgoing Contradicts
from rank-0). Only bidirectional querying (both Outgoing and Incoming from rank-0's
node) will see the rank-1 → rank-0 edge and correctly suppress rank-1.

---

### T-GS-07 — Contradicts only between rank-2 and rank-3; rank-0 and rank-1 unaffected
**Risk coverage**: FR-02 (pair selectivity), AC-01
**Scenario**: Edge only between rank-2 and rank-3. Rank-0 and rank-1 have no edges at all.
**Arrange**:
```rust
let entries = vec![make_entry(1), make_entry(2), make_entry(3), make_entry(4)];
let edges = vec![contradicts_edge(3, 4)];  // rank-2→rank-3
let graph = build_typed_relation_graph(&entries, &edges).unwrap();
let result_ids = vec![1u64, 2u64, 3u64, 4u64];
```
**Act**: `let (mask, cids) = suppress_contradicts(&result_ids, &graph);`
**Assert**:
- `mask.len() == 4`
- `mask == vec![true, true, true, false]`  (only rank-3 suppressed; rank-2 is the suppressor and remains kept)
- `cids.len() == 4`
- `cids[0] == None`    (rank-0 not suppressed)
- `cids[1] == None`    (rank-1 not suppressed)
- `cids[2] == None`    (rank-2 not suppressed — it is the suppressor, not a victim)
- `cids[3] == Some(3)` (rank-3 suppressed by entry id=3)

**Correction note**: The outer loop reaches i=2 (id=3, keep_mask[2]=true) and finds its
outgoing Contradicts edge to id=4 (rank-3). It sets keep_mask[3]=false and
contradicting_ids[3]=Some(3). Rank-2 (id=3) is never marked false by any entry — it is the
suppressor, not a victim. The previously incorrect assertion `[true, true, false, false]`
has been corrected to `[true, true, true, false]`.

---

### T-GS-08 — use_fallback=true path: empty TypedRelationGraph → all true
**Risk coverage**: FR-08, R-11, AC-05 (unit-level confirmation)
**Scenario**: `TypedRelationGraph::empty()` (cold-start state) is passed directly. The
function must return all-true without panic regardless of graph state, because the cold-start
guard in `search.rs` (`if !use_fallback`) prevents this case in production. This test confirms
the function itself is also safe when called with an empty graph.
**Arrange**:
```rust
let graph = TypedRelationGraph::empty();
let result_ids = vec![1u64, 2u64, 3u64];
```
**Act**: `let (mask, cids) = suppress_contradicts(&result_ids, &graph);`
**Assert**:
- `mask.len() == 3`
- `mask == vec![true, true, true]`
- `cids.len() == 3`
- `cids == vec![None, None, None]`
- (No panic; all IDs absent from `node_index` return `true` per the "entry not in graph"
  edge case specified in RISK-TEST-STRATEGY.md)

---

## Code Review Gates (non-test assertions)

These must be checked at gate-3b:

| Check | Command / Method |
|-------|-----------------|
| `suppress_contradicts` declared `pub fn` | grep `pub fn suppress_contradicts` in `graph_suppression.rs` |
| `graph.rs` contains module declaration | grep `mod graph_suppression` in `graph.rs` |
| `graph.rs` contains re-export | grep `pub use graph_suppression::suppress_contradicts` in `graph.rs` |
| No new entry in `lib.rs` | grep `graph_suppression` in `lib.rs` returns 0 matches |
| No direct `edges_directed`/`neighbors_directed` calls | grep `edges_directed\|neighbors_directed` in `graph_suppression.rs` returns 0 |
| `graph_tests.rs` unchanged (0 new tests) | git diff `graph_tests.rs` shows no additions |
| `graph_suppression.rs` < 500 lines | `wc -l graph_suppression.rs` |

---

## Risk Coverage Summary

| Risk | Covered By |
|------|-----------|
| R-01 (test file placement) | Tests placed in `graph_suppression.rs` `#[cfg(test)]` by definition |
| R-02 (pub visibility) | Compile gate: T-SC-08 imports via re-export |
| R-04 (edges_of_type string match) | T-GS-02 (explicit "Contradicts" string via `as_str()`) |
| R-05 (bidirectional omission) | T-GS-06 (mandatory Incoming case) |
| R-06 (mask length) | All 8 tests assert `mask.len() == result_ids.len()` |
| R-08 (module not wired) | Compile gate |
| R-09 (lib.rs pollution) | Code review grep |
