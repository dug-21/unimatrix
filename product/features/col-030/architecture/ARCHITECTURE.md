# col-030: Contradicts Collision Suppression — Architecture

## System Overview

col-030 adds a post-scoring suppression filter to `SearchService::search` that removes the
lower-ranked member of any pair of result entries connected by a `Contradicts` edge in the
`TypedRelationGraph`. This prevents the search pipeline from surfacing contradictory knowledge
to the same agent in a single response.

The feature is an independent stepping stone before PPR (#398). It validates the full
`TypedRelationGraph` retrieval path — edge loading, `build_typed_relation_graph`, hot-path
read lock, and `edges_of_type(Contradicts)` — without modifying any scoring formula.

All infrastructure required (NLI-written `Contradicts` edges, `TypedRelationGraph` with
bidirectional `edges_of_type`, pre-built graph in `TypedGraphState`, search hot-path read
lock clone) already exists. col-030 adds one pure function and one call site in search.rs.

## Component Breakdown

### Component 1: `suppress_contradicts` (new, `graph_suppression.rs`)

**Location**: `crates/unimatrix-engine/src/graph_suppression.rs`
**Exported via**: `crates/unimatrix-engine/src/graph.rs` re-export

Responsibility: Given an ordered slice of entry IDs (highest-rank first) and a
`TypedRelationGraph`, return a `Vec<bool>` keep-mask of the same length. For each pair
(i, j) where i < j (entry at index i ranks higher), if there exists a `Contradicts` edge
in either direction between `result_ids[i]` and `result_ids[j]`, set `keep_mask[j] = false`.

This function is pure (no I/O, no side effects), deterministic, and directly unit-testable
without server infrastructure.

**Why a separate file (ADR-001)**: `graph.rs` is 587 lines. Adding `suppress_contradicts`
(~30-50 lines) plus helpers would push it to 617-637 lines, violating the 500-line per-file
convention (entry #161). `graph_suppression.rs` as a sibling module re-exported from
`graph.rs` is the mandated pattern.

### Component 2: Step 10b insertion in `SearchService::search` (modified, `search.rs`)

**Location**: `crates/unimatrix-server/src/services/search.rs`
**Insertion point**: After Step 10 (similarity/confidence floors), before Step 11
(ScoredEntry construction)

Responsibility: Extract result IDs from the floor-filtered `results_with_scores`, call
`suppress_contradicts`, apply the keep-mask to both `results_with_scores` and the
corresponding prefix of `final_scores` in a single indexed pass (SR-02), emit a DEBUG-level
log line when at least one entry is suppressed (SR-04).

The `typed_graph` clone is already in scope from Step 6 (`search.rs:611-619`). No new lock
acquisition is needed.

**Cold-start guard**: The call is guarded by `if !use_fallback`. `use_fallback` is already
in scope (cloned at Step 6). When `use_fallback = true`, the suppression block is skipped
entirely and all results pass through unchanged (AC-04).

### Component 3: Unit tests in `graph_suppression.rs` (inline `#[cfg(test)]`)

**Location**: `crates/unimatrix-engine/src/graph_suppression.rs` under `#[cfg(test)]`
(NOT `graph_tests.rs` — that file is already 1,068 lines; see R-01 in RISK-TEST-STRATEGY.md)

Six unit tests for `suppress_contradicts` covering:
- Empty graph: no-op
- Contradicts edge rank-0 → rank-1: rank-1 suppressed
- Contradicts edge rank-0 → rank-3: rank-3 suppressed
- Chain suppression: rank-0 contradicts rank-2, rank-2 contradicts rank-3 → both suppressed
- Non-Contradicts edge types (CoAccess, Supports, Supersedes): no suppression
- Bidirectional edge coverage: Incoming edge from higher-rank also triggers suppression

### Component 4: Integration test in `search.rs` (new, mandatory per SR-05)

**Location**: `crates/unimatrix-server/src/services/search.rs` (test module)

One integration test using `test_scenarios.rs` infrastructure: populate entries with a known
`Contradicts` edge, run `SearchService::search`, assert the higher-ranked entry appears in
results and the lower-ranked entry is absent (behavior-based pattern, entry #724).

## Component Interactions

```
Background tick
  └─ TypedGraphState::rebuild()
       └─ store.query_graph_edges() → GraphEdgeRow[]
       └─ build_typed_relation_graph() → TypedRelationGraph (held in TypedGraphStateHandle)

SearchService::search() [search.rs]
  Step 6:  read lock → clone typed_graph, all_entries, use_fallback → release lock
  ...
  Step 10: floors (retain on results_with_scores)
  Step 10b [NEW]:
    if !use_fallback:
      result_ids = extract IDs from results_with_scores
      keep_mask = suppress_contradicts(result_ids, &typed_graph)  [graph_suppression.rs]
      apply keep_mask → single indexed rebuild of results_with_scores + final_scores prefix
      if any suppressed: debug!("suppressed entry {id} contradicts {contradicting_id}")
  Step 11: zip(results_with_scores, final_scores) → ScoredEntry[]
```

## Technology Decisions

- **petgraph via `edges_of_type`**: All graph traversal in `suppress_contradicts` uses
  `TypedRelationGraph::edges_of_type(node_idx, RelationType::Contradicts, Direction::Outgoing)`
  and `...::Incoming`. No direct calls to `.edges_directed()` or `.neighbors_directed()`.
  (ADR-002, SR-01 boundary)

- **Pure function design**: `suppress_contradicts` takes `&[u64]` and `&TypedRelationGraph`,
  returns `Vec<bool>`. No async, no I/O, no mutable state. Enables unit testing without
  server setup.

- **`graph_suppression.rs` module split**: Mandated by 500-line limit. Re-exported from
  `graph.rs` so all callers use `unimatrix_engine::graph::suppress_contradicts`. (ADR-001)

- **Bidirectional Contradicts query**: NLI writes edges unidirectionally — a contradiction
  between A and B produces one edge in the direction the detection happened to see them. Both
  `Outgoing` and `Incoming` must be queried for correctness, not merely safety. (ADR-003)

- **Single indexed pass for mask application**: `results_with_scores` and the corresponding
  prefix of `final_scores` are filtered by the same boolean mask in a single `.enumerate()`
  pass — never separate `retain` calls. (ADR-004, SR-02)

- **No config toggle**: Suppression is active whenever `use_fallback = false`. The
  cold-start guard (`if !use_fallback`) is the only gating condition. No feature flag. (ADR-005)

## Integration Points

### Existing components consumed

| Component | What is used | Location |
|-----------|-------------|----------|
| `TypedRelationGraph` | Holds Contradicts edges; provides `edges_of_type` | `unimatrix-engine/src/graph.rs` |
| `edges_of_type` | Sole filter boundary for graph traversal (SR-01) | `TypedRelationGraph` impl |
| `RelationType::Contradicts` | Edge type discriminant for filtering | `graph.rs` |
| `TypedGraphState.use_fallback` | Cold-start guard flag | `services/typed_graph.rs` |
| `typed_graph` clone | Already in scope at search hot path (Step 6) | `search.rs:611-619` |
| `results_with_scores: Vec<(EntryRecord, f64)>` | Floor-filtered result list | `search.rs:892` |
| `final_scores: Vec<f64>` | Parallel fused-score Vec (same pre-floor length) | `search.rs:893` |
| `test_scenarios.rs` | Integration test entry/graph construction | `unimatrix-engine/src/test_scenarios.rs` |

### New component introduced

| Component | Location | Exported as |
|-----------|----------|-------------|
| `suppress_contradicts` | `unimatrix-engine/src/graph_suppression.rs` | `unimatrix_engine::graph::suppress_contradicts` |

### NOT touched

- `context_lookup`, `context_get` — single-entry fetch, no ranking, out of scope
- `GRAPH_EDGES` schema — no migration; Contradicts edges already written by NLI path
- Scoring formula — no changes to `compute_fused_score` or any weights
- `edges_of_type` implementation — already correct for any `RelationType`

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `suppress_contradicts` | `pub fn suppress_contradicts(result_ids: &[u64], graph: &TypedRelationGraph) -> Vec<bool>` | `graph_suppression.rs`, re-exported from `graph.rs` |
| `edges_of_type` | `fn edges_of_type(&self, node_idx: NodeIndex, relation_type: RelationType, direction: Direction) -> impl Iterator<Item = EdgeReference<'_, RelationEdge>>` | `graph.rs:188` |
| `TypedRelationGraph::node_index` | `HashMap<u64, NodeIndex>` | `graph.rs:167` |
| `RelationType::Contradicts` | Enum variant | `graph.rs:69` |
| `use_fallback` | `bool` — cloned from `TypedGraphState` at Step 6 | `search.rs:619` |
| `results_with_scores` | `Vec<(EntryRecord, f64)>` — floor-filtered, sorted DESC | `search.rs:892` |
| `final_scores` | `Vec<f64>` — parallel to `results_with_scores` before floors | `search.rs:893` |

### Mask application contract (SR-02)

After floors, `results_with_scores.len()` ≤ `final_scores.len()`. The aligned prefix is
`final_scores[..results_with_scores.len()]`. Step 10b rebuilds both Vecs from this aligned
prefix in one pass:

```
let result_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();
let keep_mask = suppress_contradicts(&result_ids, &typed_graph);
// Single indexed pass — never two separate retain calls
let aligned_len = results_with_scores.len();
let mut new_rws = Vec::with_capacity(aligned_len);
let mut new_fs = Vec::with_capacity(aligned_len);
for (i, (entry_sim, &fs)) in results_with_scores
    .iter()
    .zip(final_scores[..aligned_len].iter())
    .enumerate()
{
    if keep_mask[i] {
        new_rws.push(entry_sim.clone());
        new_fs.push(fs);
    }
}
results_with_scores = new_rws;
final_scores = new_fs;
```

This preserves the zip-at-Step-11 invariant without needing to make `final_scores` mutable
until Step 10b.

## Parallel Vec Invariant Analysis (SR-02)

At Step 10b:
- `results_with_scores`: floor-filtered, sorted DESC by final_score. Length ≤ k.
- `final_scores`: built from `scored` at line 893, NOT filtered by floors. Length = k (or
  however many passed the score sort).
- The Step 11 zip uses "shorter wins" — `results_with_scores` is the shorter iterator
  after floors, so pairing is correct for indices 0..results_with_scores.len().

Step 10b operates on the aligned prefix `results_with_scores[0..n]` ↔
`final_scores[0..n]` where n = `results_with_scores.len()`. The single-pass indexed
rebuild (above) maintains this alignment after suppression.

Note: `final_scores` is a `let` binding at line 893, so Step 10b must shadow it with a
`let final_scores = new_fs;` or the implementation agent must use `mut`. The
implementation brief must call this out explicitly.

## `use_fallback` Atomicity (SR-08)

`use_fallback` is cloned from the `TypedGraphState` under the read lock at Step 6 (line
611-619 in search.rs). The background tick acquires the write lock to swap the entire
`TypedGraphState` struct atomically (including `typed_graph` and `use_fallback` together).
The search hot path reads both under the same read lock acquisition, so there is no
torn-read window between `use_fallback = false` and a populated `typed_graph`. SR-08
risk is mitigated by the existing read-lock clone pattern.

## File Placement Decision (SR-03, SR-06 — Resolved)

`graph.rs` is 587 lines. `suppress_contradicts` adds ~30-50 lines, which would reach
617-637 lines — violating the 500-line limit.

**Decision: `graph_suppression.rs` as a sibling module.** The function is declared in
`crates/unimatrix-engine/src/graph_suppression.rs` and re-exported from `graph.rs` via
`pub use graph_suppression::suppress_contradicts;`. All callers import from
`unimatrix_engine::graph::suppress_contradicts` — the module split is invisible to callers.

Unit tests for `suppress_contradicts` go in `graph_suppression.rs` under `#[cfg(test)]`,
NOT in `graph_tests.rs` — `graph_tests.rs` is already 1,068 lines; appending to it would
cause a gate-3b rejection (R-01, RISK-TEST-STRATEGY.md).

## Test Coverage Strategy

### Unit tests (graph_suppression.rs `#[cfg(test)]`)

Tests for `suppress_contradicts` using hand-constructed `TypedRelationGraph` instances
(via `build_typed_relation_graph` with synthetic `GraphEdgeRow` slices):

1. **No edges**: all `true`
2. **Outgoing Contradicts rank-0 → rank-1**: `[true, false]`
3. **Outgoing Contradicts rank-0 → rank-3**: `[true, true, true, false]`
4. **Chain**: rank-0 contradicts rank-2, rank-2 contradicts rank-3 →
   `[true, true, false, false]`
5. **Non-Contradicts edges only** (CoAccess, Supports, Supersedes): all `true`
6. **Incoming direction**: edge written as rank-1 → rank-0 in the graph; rank-0 retains,
   rank-1 suppressed → `[true, false]`

### Integration test (search.rs)

One test: entries A, B, C where A and B have a Contradicts edge. A ranks higher than B
by fused score. Assert: A in results, B not in results, C in results (behavior-based,
entry #724).

Safe test helpers to use for GRAPH_EDGES setup: the integration test must insert edges
via `Store::insert_graph_edge()` (or the equivalent production write path), not via
`create_graph_edges_table` (SR-07: that helper is pre-v13 schema only, entry #3600).

### Eval gate

Zero-regression eval gate (existing infrastructure, `--distribution_change false` profile).
Gate passes because existing eval scenarios have no Contradicts edges — suppression is a
no-op. Gate alone does not validate suppression correctness (SR-05); the integration test
above is the mandatory positive gate.

## Open Questions

None. All open questions from SCOPE.md are resolved:

- **File placement (Open Question 4)**: Resolved — `graph_suppression.rs` (see ADR-001).
- **Eval scenario coverage (Open Question 5)**: Integration test in `search.rs` is sufficient.
  Eval JSONL scenario coverage deferred to post-#412.
- **Edge direction**: Confirmed unidirectional write; both directions must be queried (ADR-003).
- **`use_fallback` atomicity**: Confirmed safe by existing read-lock clone pattern (SR-08).
