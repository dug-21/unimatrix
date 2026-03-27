# col-030: Contradicts Collision Suppression

## Problem Statement

When an agent receives search or briefing results, the ranking pipeline may surface two
entries that carry a `Contradicts` edge between them — meaning the knowledge base itself
has already determined they express conflicting claims. Serving both to the same agent in
the same response creates a "knowledge collision": the agent receives a high-ranked entry
and its direct semantic contradiction in the same batch, with no signal that a conflict
exists. This undermines the usefulness of the ranked list.

The existing pipeline has the infrastructure to detect this:
- `Contradicts` edges are written to `GRAPH_EDGES` by the NLI post-store detection path
  (`nli_detection.rs`, `contradiction_cache.rs`).
- `TypedRelationGraph` already loads those edges from `GRAPH_EDGES` on every background
  tick rebuild (via `build_typed_relation_graph` in `graph.rs`).
- `edges_of_type(Contradicts)` is already implementable via the `TypedRelationGraph.inner`
  petgraph and the existing `edges_of_type` boundary method.
- The search hot path already clones `typed_graph` out from under a short read lock
  (Step 6, `search.rs:607–622`) before the scoring loop.

What is missing: a post-scoring filter step that, given the ranked-and-truncated result
list, identifies pairs connected by `Contradicts` edges and removes the lower-ranked
member of each conflicting pair.

This feature is scoped as an **independent stepping stone** before PPR (#398): it validates
the end-to-end `TypedRelationGraph` retrieval path (edge loading, `edges_of_type` query,
hot-path read lock) without touching the scoring formula. The eval gate is a zero-regression
check — ranking and distribution must be preserved across existing scenarios.

## Goals

1. Implement a post-scoring `Contradicts` collision suppression filter in `SearchService::search`,
   applied after the scored list is sorted and truncated to `k`, using edges already present
   in the pre-built `TypedRelationGraph`.
2. Implement `edges_of_type(Contradicts)` queries within the graph module, confirming the
   existing `edges_of_type` method handles `RelationType::Contradicts` correctly and is
   exercised by the suppression logic.
3. Validate the `TypedRelationGraph` retrieval path end-to-end: edge loading from store,
   graph build, hot-path read, and `edges_of_type` — all exercised by this feature.
4. Pass the zero-regression eval gate: no change in MRR, P@K ranking, or score distribution
   across all existing eval scenarios (which have no Contradicts edges).
5. Implement collision suppression as a pure function in `unimatrix-engine/src/graph.rs`
   so it is testable without server infrastructure.

## Non-Goals

- This feature does NOT modify the fused scoring formula or any scoring weights.
- This feature does NOT write new `Contradicts` edges (that is the NLI post-store detection
  path, already implemented). It only reads existing edges.
- This feature does NOT implement Personalized PageRank (PPR) — that is #398, which
  depends on this feature as a predecessor.
- This feature does NOT apply collision suppression to `context_lookup` or `context_get`
  — only to `SearchService::search`, which serves both `context_search` and
  `context_briefing`.
- This feature does NOT penalize (score-adjust) contradicting entries — it removes the
  lower-ranked one from the result set.
- This feature does NOT implement `#413` (graph cohesion metrics in `context_status`).
- This feature does NOT introduce a new config toggle for enabling/disabling suppression;
  suppression is applied whenever the `TypedRelationGraph` is available
  (`use_fallback = false`) and `Contradicts` edges exist in the current result set.

## Background Research

### Where Contradicts Edges Live

`GRAPH_EDGES` table (schema, `graph_tests.rs`). Written by two paths:
1. `run_post_store_nli` in `nli_detection.rs` — fires after `context_store`, writes
   `Contradicts` edges when NLI contradiction score exceeds `nli_contradiction_threshold`
   (default 0.6) and similarity exceeds `nli_entailment_threshold`.
2. `maybe_run_bootstrap_promotion` — one-shot bootstrap pass on first tick.

The `bootstrap_only` flag on edges written by the bootstrap path is `true`; these are
excluded from `TypedRelationGraph.inner` structurally (Pass 2b in `build_typed_relation_graph`).
Only NLI-inferred edges with `bootstrap_only=false` are included in the live graph.

The constant `EDGE_SOURCE_NLI = "nli"` (col-029 ADR-001, entry #3591) is written to
`graph_edges.source` for NLI-inferred edges.

### TypedRelationGraph: Current State

- Defined in `crates/unimatrix-engine/src/graph.rs`.
- `edges_of_type` is the sole filter boundary (SR-01 mitigation) — all traversal MUST use
  it. Calling `.edges_directed()` directly at traversal sites is prohibited.
- Already holds `Contradicts` edges in `inner` alongside `CoAccess`, `Supports`, and
  `Supersedes` — they are just not queried anywhere yet.
- `TypedRelationGraph` is `Clone` (derived). The search hot path clones it once under a
  short read lock before any traversal.
- Cold-start: `use_fallback = true`, `typed_graph` is empty. Suppression must be skipped
  when `use_fallback = true`.

### Search Pipeline Integration Point

Full 12-step pipeline in `SearchService::search` (`search.rs`):

```
Step 5:  HNSW candidate retrieval (expanded to nli_top_k when NLI enabled)
Step 6:  Quarantine filter + entry fetch
Step 6a: Status filter / penalty marking (Flexible vs Strict mode)
Step 6b: Supersession candidate injection (find_terminal_active)
Step 6c: Co-access boost map prefetch
Step 7:  NLI scoring → fused score → sort DESC → truncate to k
Step 8:  [renumbered in code as post-Step 7 rebuild]
Step 9:  No-op truncation
Step 10: Similarity and confidence floors
**NEW**  [Step 10b]: Contradicts collision suppression
Step 11: Build ScoredEntry output
Step 12: S5 audit
```

The suppression filter belongs after Step 10 (floors already applied, final set is
determined) and before Step 11 (ScoredEntry construction). It operates on the
`results_with_scores` slice in sorted order. The `final_scores` Vec is parallel and
must also be kept in sync after any removal.

### Suppression Logic

For each pair (i, j) where i < j (i.e., entry_i ranks higher than entry_j):
- If there exists a `Contradicts` edge between entry_i.id and entry_j.id in **either
  direction** (required, not optional — edges are unidirectional at write time), remove
  entry_j (the lower-ranked member).
- The higher-ranked entry is retained; its score is unchanged.
- At most one pass through the list is needed: process in rank order, skip already-removed
  entries.

This is an O(n) sweep over results (small k, typically ≤ 20) with O(E_c) edge lookups per
entry, where E_c is the number of Contradicts edges per entry (typically very small).

A pure function `suppress_contradicts(results: &[(u64, ...)], graph: &TypedRelationGraph) -> Vec<bool>`
returns a keep/drop bitmask, making the logic unit-testable without server infrastructure.

**Edge direction (confirmed)**: `nli_detection.rs` writes `Contradicts` edges unidirectionally —
`(source_id, neighbor_id, 'Contradicts')` only, always from the new entry toward its neighbor
(lines 509–523). There is no reverse write. A contradiction between A and B will have an edge
in whichever direction happened to be the "new entry" at detection time. Checking both
`Outgoing` and `Incoming` directions is therefore **required for correctness**, not merely
a safety measure.

### Existing Co-Access Boost Insertion (Reference Pattern)

The co-access boost map was inserted at Step 6c, following the same pattern of "compute
something from `typed_graph` or store, store it in a local variable, use it in Step 7
scoring loop." The Contradicts suppression follows the same topology but operates
post-scoring rather than pre-scoring.

### Eval Harness for Zero-Regression Gate

The eval harness (`eval/runner/`) runs scenarios from JSONL files through a
`EvalServiceLayer` with config overrides, computing MRR, P@K, CC@k, ICD per profile.

Zero-regression gate (`render_zero_regression.rs`, `eval/report/`): compares candidate
profile MRR and P@K against baseline per-scenario. No regressions = gate passes. This is
the mandated gate for #395 per the roadmap.

The gate is already implemented for other features. col-030 does not need to add new eval
infrastructure — it uses the existing `--distribution_change false` profile path.

Existing eval scenarios have no `Contradicts` edges in the test DB (the test DB is populated
from `test_scenarios.rs` fixtures). Therefore, suppression is a no-op for all existing
scenarios, which is exactly the zero-regression invariant.

### Behavior-Based Test Pattern (entry #724)

For retrieval pipeline features that apply penalties or filters: assert on relative ranking
order (`result[0].id == expected_winner`), not on absolute scores. Use the integration
test infrastructure in `test_scenarios.rs` for deterministic scenario construction.

## Proposed Approach

### 1. Pure suppression function in `unimatrix-engine/src/graph.rs`

Add a new public function:
```rust
pub fn suppress_contradicts(
    result_ids: &[u64],
    graph: &TypedRelationGraph,
) -> Vec<bool>  // true = keep, false = suppress
```

Iterates `result_ids` in order (highest rank first). For each entry not already suppressed,
queries `edges_of_type(entry_idx, RelationType::Contradicts, Direction::Outgoing)` and
`edges_of_type(entry_idx, RelationType::Contradicts, Direction::Incoming)` to collect all
contradiction neighbors (both directions required — edges are unidirectional at write time).
Any lower-ranked neighbor present in `result_ids` gets its keep flag set to false.

Returns a `Vec<bool>` of length `result_ids.len()`. Caller applies the mask.

This is pure (no I/O), deterministic, O(n * degree_c) where degree_c is the number of
Contradicts edges per node (typically < 3), and directly testable.

### 2. Insertion in `SearchService::search` (Step 10b)

After Step 10 (floors) and before Step 11 (ScoredEntry construction):

```
// Step 10b: Contradicts collision suppression.
// Skip when use_fallback=true (graph not yet built) or no Contradicts edges present.
// Operates on results_with_scores (parallel with final_scores).
```

Extract `result_ids` from `results_with_scores`. Call `suppress_contradicts`. Apply mask
to both `results_with_scores` and `final_scores` (they are parallel Vecs). This preserves
the sorted order invariant.

### 3. Test coverage in `graph_tests.rs`

Unit tests for `suppress_contradicts`:
- No edges in graph: all entries kept (no-op).
- One Contradicts edge between rank-0 and rank-1: rank-1 dropped.
- One Contradicts edge between rank-0 and rank-3: rank-3 dropped.
- Contradicts edge between rank-2 and rank-3, but rank-0 also contradicts rank-2: both
  rank-2 and rank-3 suppressed (rank-0 kept, rank-1 kept).
- Non-Contradicts edges (CoAccess, Supports, Supersedes) do not trigger suppression.
- `use_fallback = true` path: cold-start graph is empty → suppression is effectively
  a no-op (empty graph has no edges).

### 4. Integration test confirming ranking preservation (search.rs tests)

Add a test following the behavior-based pattern (entry #724): construct entries with a
known `Contradicts` edge, verify the higher-ranked entry appears in results and the
lower-ranked entry is absent. Assert relative ordering is preserved for entries without
Contradicts edges.

### Rationale for Insertion Point

Post-scoring (after sort + truncate) is correct because:
- The suppression decision is based on which entry ranks higher, not on score magnitudes.
- Applying it pre-scoring would require a more complex "which of the two to keep" decision
  without having final scores.
- After floors (Step 10) ensures we only suppress from the set actually returned to the
  caller.
- The `typed_graph` clone is already present in the search scope from Step 6 — no new lock
  acquisition needed.

## Acceptance Criteria

- AC-01: `suppress_contradicts(result_ids, graph)` returns a `Vec<bool>` of the same
  length as `result_ids`. For an empty graph or result set with no Contradicts edges, all
  values are `true`.
- AC-02: For two entries in the result set connected by a `Contradicts` edge (either
  direction), the lower-ranked entry (higher index in sorted order) is suppressed
  (`false`); the higher-ranked entry is retained (`true`).
- AC-03: Non-Contradicts edges (CoAccess, Supports, Supersedes, Prerequisite) do not
  cause suppression.
- AC-04: When `use_fallback = true` (cold-start graph), the suppression step is skipped
  and all results pass through unchanged.
- AC-05: Suppression is applied to both `context_search` (Flexible mode) and
  `context_briefing` (Strict mode), as both use `SearchService::search`.
- AC-06: Zero-regression eval gate passes: MRR, P@K, and score distribution are unchanged
  across all existing eval scenarios (which have no Contradicts edges in the test DB).
- AC-07: When Contradicts edges exist between entries in the result set, exactly the
  lower-ranked entry per conflicting pair is removed. The result set length is reduced
  accordingly (no padding back to `k`).
- AC-08: The `suppress_contradicts` function is defined in `unimatrix-engine/src/graph.rs`
  and unit-tested in `graph_tests.rs`.
- AC-09: All existing tests in `graph_tests.rs` and `search.rs` continue to pass with no
  modifications.
- AC-10: `edges_of_type` is the sole call site for querying Contradicts neighbors
  (SR-01 mitigation — no direct `.edges_directed()` calls in suppression logic).

## Constraints

- **No new dependencies**: suppression uses `petgraph` (already a dependency of
  `unimatrix-engine`) via the existing `edges_of_type` method. No new crates.
- **No schema changes**: `Contradicts` edges already exist in `GRAPH_EDGES`. No
  migration required.
- **SR-01 boundary**: all graph traversal MUST go through `TypedRelationGraph::edges_of_type`.
  Direct `.edges_directed()` or `.neighbors_directed()` calls are prohibited in the
  suppression function.
- **Cold-start safety**: `use_fallback = true` must bypass suppression. The empty cold-start
  graph (`TypedRelationGraph::empty()`) contains no edges and no nodes, so calling
  `suppress_contradicts` on it is a natural no-op regardless, but the `use_fallback` guard
  must be explicit in `search.rs`.
- **Max 500 lines per file**: `graph.rs` is currently 588 lines including the test module
  reference. The suppression function adds ~30–50 lines. If the file reaches 600+ lines,
  the suppression function should be placed in a new `graph_suppression.rs` sibling module
  and re-exported from `graph.rs`.
- **Eval gate is mandatory**: the zero-regression eval report must be produced and reviewed
  before the PR is considered merge-ready.
- **`final_scores` parallel invariant**: `results_with_scores` and `final_scores` are
  parallel Vecs in `search.rs`. Both must be filtered by the suppression mask in the
  same pass to preserve alignment.

## Open Questions

~~1. **Contradicts edge direction semantics**~~ **RESOLVED**: `nli_detection.rs` writes
   `Contradicts` edges unidirectionally (`source_id → neighbor_id`) with no reverse write.
   Both `Outgoing` and `Incoming` directions must be checked — this is required for
   correctness, not just safety.

~~2. **Suppression scope (`context_lookup`)**~~ **RESOLVED**: Out of scope. `context_lookup`
   is a deterministic fetch by ID — no ranking, no collision. If contradiction warnings on
   single-entry fetch are ever wanted, that is a separate proactive conflict notification
   feature.

~~3. **Suppressed-entry count in audit log**~~ **RESOLVED**: Deferred. The zero-regression
   eval gate uses scenarios with no Contradicts edges — audit data would show nothing useful
   until #412 ships real edges. Add as a follow-up comment on #395 for post-#412 enrichment.

4. **Graph file size**: `graph.rs` is already at ~588 lines. If `suppress_contradicts`
   plus its helpers pushes it past 600, should the suppression logic live in
   `graph_suppression.rs` or inline in `search.rs`? The pure-function design means either
   is valid. Prefer `graph.rs` for discoverability if the line count allows it; otherwise
   `graph_suppression.rs` as a sibling module re-exported from `graph.rs`.

5. **Eval scenario coverage**: Existing scenarios have no `Contradicts` edges. The
   zero-regression gate validates no regressions. A positive test (entries with Contradicts
   edges actually removed from results) requires either synthetic JSONL scenarios with
   hand-authored contradicting entries, or integration tests in `search.rs`. The latter
   is sufficient for this feature; eval scenario coverage of suppression behavior is
   deferred.

## Tracking

https://github.com/dug-21/unimatrix/issues/418
