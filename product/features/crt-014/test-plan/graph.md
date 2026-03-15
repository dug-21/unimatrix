# Test Plan: graph.rs — crt-014

## Component

`crates/unimatrix-engine/src/graph.rs` — NEW file.

Public API under test:
- `build_supersession_graph(entries: &[EntryRecord]) -> Result<SupersessionGraph, GraphError>`
- `graph_penalty(node_id: u64, graph: &SupersessionGraph, entries: &[EntryRecord]) -> f64`
- `find_terminal_active(node_id: u64, graph: &SupersessionGraph, entries: &[EntryRecord]) -> Option<u64>`
- All `pub const` penalty constants

All tests are synchronous unit tests in the inline `#[cfg(test)]` block at the bottom of `graph.rs`. No store required — tests build `EntryRecord` slices directly.

---

## Test Helper

```rust
fn make_entry(id: u64, status: Status, supersedes: Option<u64>, superseded_by: Option<u64>) -> EntryRecord {
    // Construct a minimal EntryRecord with the given topology fields.
    // All other fields use defaults (confidence 0.5, category "decision", etc.).
}
```

This helper is used by all graph.rs tests. Extend the existing pattern from other engine test modules — do not create a separate test utilities file.

---

## AC-03: Cycle Detection

**Risk**: R-02

### `fn cycle_two_node_detected`

Scenario: A.supersedes = Some(B.id), B.supersedes = Some(A.id).

```
entries = [
    make_entry(id=1, status=Active, supersedes=Some(2), superseded_by=None),
    make_entry(id=2, status=Active, supersedes=Some(1), superseded_by=None),
]
result = build_supersession_graph(&entries)
assert!(matches!(result, Err(GraphError::CycleDetected)))
```

### `fn cycle_three_node_detected`

Scenario: A→B→C→A (triangle).

```
entries = [A(supersedes=C), B(supersedes=A), C(supersedes=B)]
assert!(matches!(build_supersession_graph(&entries), Err(GraphError::CycleDetected)))
```

### `fn cycle_self_referential_detected`

Scenario: A.supersedes = Some(A.id).

```
entries = [make_entry(id=1, supersedes=Some(1))]
assert!(matches!(build_supersession_graph(&entries), Err(GraphError::CycleDetected)))
```

### `fn valid_dag_depth_1`

Scenario: B supersedes A. No cycle.

```
entries = [A(supersedes=None), B(supersedes=Some(A.id))]
assert!(build_supersession_graph(&entries).is_ok())
```

### `fn valid_dag_depth_2`

Scenario: A → B → C (3-entry linear chain).

```
entries = [A, B(supersedes=A.id), C(supersedes=B.id)]
assert!(build_supersession_graph(&entries).is_ok())
```

### `fn valid_dag_depth_3_plus`

Scenario: 4-entry linear chain. Verifies no false positives at depth 3+.

```
entries = [A, B(supersedes=A), C(supersedes=B), D(supersedes=C)]
assert!(build_supersession_graph(&entries).is_ok())
```

### `fn empty_entry_slice_is_valid_dag`

Scenario: Edge case — empty input.

```
assert!(build_supersession_graph(&[]).is_ok())
```

### `fn single_entry_no_supersedes`

Scenario: One entry with no supersession relationship.

```
entries = [make_entry(id=1, supersedes=None)]
assert!(build_supersession_graph(&entries).is_ok())
```

---

## AC-04: Edge Direction Verification (R-04)

### `fn edge_direction_pred_to_successor`

This test explicitly inspects the graph structure. It addresses R-04 (edge reversed) by checking outgoing edges from the predecessor node.

```
// B.supersedes = Some(A.id) → edge must be A → B
entries = [A, B(supersedes=Some(A.id))]
graph = build_supersession_graph(&entries).unwrap()
a_index = graph.node_index[&A.id]
outgoing: Vec<_> = graph.inner.edges_directed(a_index, Outgoing).collect()
assert_eq!(outgoing.len(), 1)
// The target of the edge is B's node index
b_index = graph.node_index[&B.id]
assert!(outgoing.iter().any(|e| e.target() == b_index))
```

---

## AC-05: graph_penalty Range

**Risk**: R-01, R-12

### `fn penalty_range_all_scenarios`

For each topology scenario below, assert `0.0 < result < 1.0`:

| Scenario | Expected Constant |
|----------|------------------|
| Orphan (Deprecated, 0 outgoing edges) | ORPHAN_PENALTY (0.75) |
| Dead-end (no active reachable) | DEAD_END_PENALTY (0.65) |
| Partial supersession (>1 successor) | PARTIAL_SUPERSESSION_PENALTY (0.60) |
| Depth-1 clean replacement | CLEAN_REPLACEMENT_PENALTY (0.40) |
| Depth-2 decay | ~0.24 (within [0.10, 0.40]) |
| Depth-5 decay | clamped to 0.10 |

Each scenario must produce a value strictly in `(0.0, 1.0)`.

### `fn penalty_absent_node_returns_one`

Nodes not in the graph receive no penalty.

```
graph = build_supersession_graph(&[]).unwrap()  // empty graph
result = graph_penalty(9999, &graph, &[])
assert_eq!(result, 1.0)
```

---

## AC-06: Orphan Softer Than Clean Replacement (R-01, R-05)

### `fn orphan_softer_than_clean_replacement`

Behavioral ordering assertion replacing `deprecated_penalty_value` from confidence.rs.

```
assert!(
    ORPHAN_PENALTY > CLEAN_REPLACEMENT_PENALTY,
    "orphan ({ORPHAN_PENALTY}) must be softer (higher multiplier) than clean replacement ({CLEAN_REPLACEMENT_PENALTY})"
)
```

Also verify via graph_penalty on constructed entries:

```
// Orphan: Deprecated, no outgoing edges
orphan = make_entry(id=1, status=Deprecated, supersedes=None)
// Clean replacement: superseded with Active successor at depth 1
pred = make_entry(id=2, status=Active, supersedes=None)
succ = make_entry(id=3, status=Active, supersedes=Some(2), superseded_by=None)
// ...build graph with pred and succ...
orphan_penalty = graph_penalty(1, &orphan_graph, &[orphan])
clean_penalty  = graph_penalty(2, &chain_graph, &[pred, succ])
assert!(orphan_penalty > clean_penalty,
    "orphan ({orphan_penalty}) must be softer than clean replacement ({clean_penalty})")
```

---

## AC-07: 2-Hop Harsher Than 1-Hop (R-01, R-05)

### `fn two_hop_harsher_than_one_hop`

Behavioral ordering assertion replacing `superseded_penalty_value` from confidence.rs.

```
// Chain: A → B → C (A is at depth 2 from C; B is at depth 1 from C)
entries = [A(supersedes=None), B(supersedes=A.id), C(supersedes=B.id, status=Active)]
graph = build_supersession_graph(&entries).unwrap()
penalty_a = graph_penalty(A.id, &graph, &entries)  // depth-2 → harsher
penalty_b = graph_penalty(B.id, &graph, &entries)  // depth-1 → softer
assert!(penalty_a < penalty_b,
    "2-hop entry ({penalty_a}) must receive harsher (lower) penalty than 1-hop entry ({penalty_b})")
```

Also assert numerical values:

```
// depth-1: CLEAN_REPLACEMENT_PENALTY = 0.40
// depth-2: 0.40 * 0.60^1 = 0.24
assert!((penalty_b - CLEAN_REPLACEMENT_PENALTY).abs() < 1e-10)
assert!((penalty_a - CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR).abs() < 1e-10)
```

---

## AC-08: Partial Supersession Softer Than Clean (R-01, R-05)

### `fn partial_supersession_softer_than_clean`

Behavioral ordering assertion replacing `superseded_penalty_harsher_than_deprecated` from confidence.rs.

```
assert!(
    PARTIAL_SUPERSESSION_PENALTY > CLEAN_REPLACEMENT_PENALTY,
    "partial ({PARTIAL_SUPERSESSION_PENALTY}) must be softer than clean replacement ({CLEAN_REPLACEMENT_PENALTY})"
)
```

Also verify via graph_penalty:

```
// Partial: entry A with two active successors B and C
entries_partial = [A, B(supersedes=A.id, active), C(supersedes=A.id, active)]
// Clean: entry X with one active successor Y
entries_clean = [X, Y(supersedes=X.id, active)]
partial_penalty = graph_penalty(A.id, &partial_graph, &entries_partial)
clean_penalty   = graph_penalty(X.id, &clean_graph, &entries_clean)
assert!(partial_penalty > clean_penalty,
    "partial ({partial_penalty}) must be softer than clean replacement ({clean_penalty})")
```

---

## AC-09: find_terminal_active Three-Hop Chain (R-03)

### `fn terminal_active_three_hop_chain`

Validates correct traversal skipping superseded intermediate nodes.

```
// A(superseded by B), B(superseded by C), C(Active, superseded_by=None)
entries = [
    make_entry(id=1, status=Active,     supersedes=None,   superseded_by=Some(2)),
    make_entry(id=2, status=Active,     supersedes=Some(1), superseded_by=Some(3)),
    make_entry(id=3, status=Active,     supersedes=Some(2), superseded_by=None),
]
graph = build_supersession_graph(&entries).unwrap()
result = find_terminal_active(1, &graph, &entries)
assert_eq!(result, Some(3), "terminal must be C (id=3)")
```

### `fn terminal_active_depth_one_chain`

```
entries = [A(superseded_by=Some(B.id)), B(status=Active, superseded_by=None)]
result = find_terminal_active(A.id, &graph, &entries)
assert_eq!(result, Some(B.id))
```

### `fn terminal_active_superseded_intermediate_skipped`

Addresses R-03 scenario 4: C is Active but `superseded_by.is_some()` (chain continues); D is terminal.

```
// A→B→C→D: C has superseded_by=Some(D.id); D is Active and superseded_by=None
entries = [A, B(supersedes=A), C(supersedes=B, superseded_by=Some(D.id)), D(supersedes=C, status=Active, superseded_by=None)]
result = find_terminal_active(A.id, &graph, &entries)
assert_eq!(result, Some(D.id), "must skip C (superseded) and reach D")
```

---

## AC-10: find_terminal_active Returns None (No Active Terminal) (R-03)

### `fn terminal_active_no_reachable`

```
// Chain terminates at Deprecated entry with no active node reachable
entries = [A, B(supersedes=A, status=Deprecated, superseded_by=None)]
result = find_terminal_active(A.id, &graph, &entries)
assert_eq!(result, None)
```

### `fn terminal_active_absent_node`

```
result = find_terminal_active(9999, &empty_graph, &[])
assert_eq!(result, None)
```

---

## AC-11: find_terminal_active Depth Cap (R-07)

### `fn terminal_active_depth_cap_eleven`

Chain of 11 entries where node 11 is Active — depth cap prevents reaching it.

```
// entries: [0, 1(supersedes=0), 2(supersedes=1), ..., 10(supersedes=9, Active)]
// Total chain length = 11 hops from 0 to 10
result = find_terminal_active(0, &graph, &entries)
assert_eq!(result, None, "chain of 11 exceeds MAX_TRAVERSAL_DEPTH (10), must return None")
```

### `fn terminal_active_depth_cap_boundary_ten`

Chain of exactly 10 entries — node 10 is Active at depth 10. Must be found (boundary is inclusive).

```
// entries: [0..9], 9(supersedes=8, Active, superseded_by=None)
// From 0, reaching 9 requires 9 hops — within MAX_TRAVERSAL_DEPTH=10
result = find_terminal_active(0, &graph, &entries)
assert_eq!(result, Some(9))
```

### `fn terminal_active_depth_nine`

Chain of 9 — within bound, must find terminal.

```
result = find_terminal_active(0, &9_entry_graph, &entries)
assert!(result.is_some())
```

---

## AC-17: Dangling supersedes Reference (R-09)

### `fn dangling_supersedes_ref_is_skipped`

```
// Entry A with supersedes=Some(9999) where 9999 is not in the slice
entries = [make_entry(id=1, supersedes=Some(9999))]
result = build_supersession_graph(&entries)
assert!(result.is_ok(), "dangling ref must not cause Err or panic")
let graph = result.unwrap()
assert_eq!(graph.node_index.len(), 1, "graph must have only entry 1, no dangling node")
```

Note: the implementation emits `tracing::warn!` for the dangling reference. This is not directly assertable in unit tests without a tracing subscriber — document as a code-review check. If the test harness includes a `tracing_subscriber::fmt::try_init()` call, the log can be captured via `tracing_test` crate.

---

## AC-15: Behavioral Ordering — Migration Coverage

### `fn weight_sum_invariant_unchanged`

Replaces `penalties_independent_of_confidence_formula` from confidence.rs. Verifies that the weight sum invariant in `confidence.rs` is not broken by the constant removal. This test can remain in `confidence.rs` (it does not reference the removed constants) — or be moved to `graph.rs` as a cross-module invariant check.

```
// This test stays in confidence.rs:
let stored_sum = W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST;
assert_eq!(stored_sum, 0.92_f64, "confidence weight sum must remain 0.92")
```

Note: `penalties_independent_of_confidence_formula` only verifies the weight sum, not any penalty value. The weight sum check itself is not removed — only the penalty-constant assertions in the other 3 tests. Confirm during implementation whether this test can be retained as-is after the constant lines are removed. If it no longer references removed symbols, keep it.

---

## R-12: Decay Formula Bounds

### `fn decay_formula_depth_1`

```
assert!((graph_penalty_depth_1 - CLEAN_REPLACEMENT_PENALTY).abs() < 1e-10)
```

### `fn decay_formula_depth_2`

```
expected = CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR  // 0.40 * 0.60 = 0.24
assert!((graph_penalty_depth_2 - expected).abs() < 1e-10)
assert!(graph_penalty_depth_2 >= 0.10)  // above floor
```

### `fn decay_formula_depth_5_clamped`

```
// 0.40 * 0.60^4 = 0.40 * 0.1296 ≈ 0.0518 → clamped to 0.10
assert!((graph_penalty_depth_5 - 0.10).abs() < 1e-10)
```

### `fn decay_formula_depth_10_clamped`

```
// Very deep chain → clamped at floor
assert!((graph_penalty_depth_10 - 0.10).abs() < 1e-10)
```

### `fn decay_never_exceeds_clean_replacement`

For all depth values 1..=10, assert `result <= CLEAN_REPLACEMENT_PENALTY`.

---

## Edge Cases

| Edge Case | Test Name | Assertion |
|-----------|-----------|-----------|
| Empty entries slice | `fn empty_entry_slice_is_valid_dag` | `Ok(graph)` with zero nodes |
| All Active, none superseded | `fn all_active_no_penalty` | `graph_penalty` returns 1.0 for all entries |
| Starting node already Active for `find_terminal_active` | `fn terminal_active_starting_node_is_active` | Returns `Some(node_id)` |
| Two successors, one Active one Deprecated | `fn two_successors_one_active` | `successor_count > 1` → `PARTIAL_SUPERSESSION_PENALTY` |
| `node_id = 0` (u64 boundary) | `fn node_id_zero_not_in_graph` | Returns 1.0 without panic |
| Entry in graph but not in entries slice lookup | `fn graph_penalty_entry_not_in_slice` | Returns 1.0 without panic |

---

## NFR Checks

| NFR | Verification |
|-----|-------------|
| NFR-01: ≤5ms at 1,000 entries | Benchmark in tests: build 1,000 `EntryRecord` values, `Instant::now()` before/after `build_supersession_graph`, assert elapsed < 5ms |
| NFR-02: pure function | No I/O assertions needed; code review confirms no side effects |
| NFR-03: depth cap | `fn terminal_active_depth_cap_eleven` (above) |
| NFR-04: no unsafe | Enforced by workspace `#![forbid(unsafe_code)]`; CI build verifies |
| NFR-05: no async | All `graph.rs` functions are sync; compiler enforces |
