# SPECIFICATION: col-030 — Contradicts Collision Suppression

## Objective

When `SearchService::search` returns a ranked result list, any pair of entries connected by a
`Contradicts` edge in `TypedRelationGraph` constitutes a knowledge collision: the agent receives
conflicting claims with no signal that a conflict exists. This feature inserts a post-scoring
filter at Step 10b (after similarity/confidence floors, before `ScoredEntry` construction) that
removes the lower-ranked member of each conflicting pair. The higher-ranked entry is retained
unchanged; the result set shrinks without backfill to `k`.

---

## Functional Requirements

**FR-01** — A pure function `suppress_contradicts(result_ids: &[u64], graph: &TypedRelationGraph) -> Vec<bool>` must be defined in `unimatrix-engine/src/graph.rs` (or `graph_suppression.rs` per FR-12). It accepts entry IDs in descending rank order and returns a keep/drop bitmask of the same length (`true` = keep, `false` = suppress).

**FR-02** — For each pair `(i, j)` where `i < j` (entry `i` ranks higher), if a `Contradicts` edge exists between `result_ids[i]` and `result_ids[j]` in either direction (Outgoing or Incoming from each node), entry `j` must be marked `false` (suppressed).

**FR-03** — The suppression sweep must check both `Direction::Outgoing` and `Direction::Incoming` on each surviving (not-yet-suppressed) entry, because `nli_detection.rs` writes `Contradicts` edges unidirectionally and no reverse edge is guaranteed.

**FR-04** — Only `RelationType::Contradicts` edges trigger suppression. Edges of type `CoAccess`, `Supports`, `Supersedes`, and `Prerequisite` must not cause any entry to be suppressed.

**FR-05** — All graph traversal inside `suppress_contradicts` must go through `TypedRelationGraph::edges_of_type`. Direct calls to `.edges_directed()` or `.neighbors_directed()` on `inner` are prohibited (SR-01 boundary).

**FR-06** — `SearchService::search` must apply the suppression mask at Step 10b: after Step 10 (similarity and confidence floors) and before Step 11 (ScoredEntry construction).

**FR-07** — The suppression mask must be applied to both `results_with_scores` and `final_scores` in a single indexed pass. The two Vecs must remain strictly parallel after the operation; separate iterator chains over each Vec are prohibited (SR-02 invariant).

**FR-08** — When `use_fallback = true` (cold-start: `TypedRelationGraph` not yet built), the Step 10b suppression step must be skipped entirely. All entries pass through unchanged.

**FR-09** — When suppression fires, a `tracing::debug!` log line must be emitted for each suppressed entry, recording: the suppressed entry's ID and the ID of the surviving entry that contradicts it (SR-04 minimum observability).

**FR-10** — The scores of surviving entries must not be modified by the suppression step. Suppression is a pure removal; no score adjustments, penalties, or re-normalization are applied.

**FR-11** — After suppression, the result set length is reduced by the number of suppressed entries. No backfill or padding to the original `k` is performed.

**FR-12** — If adding `suppress_contradicts` plus its associated unit tests to `graph.rs` causes the file to exceed 500 lines (the workspace hard limit per `rust-workspace.md`), the function must be placed in a new sibling module `unimatrix-engine/src/graph_suppression.rs` and re-exported from `graph.rs`. This placement decision must be resolved before implementation begins (SR-06 gate risk).

**FR-13** — `suppress_contradicts` must be unit-tested in `graph_tests.rs` (if in `graph.rs`) or the sibling test module. The following cases are required:
  - Empty graph: all entries kept.
  - No Contradicts edges in graph: all entries kept.
  - Contradicts edge between rank-0 and rank-1 (Outgoing from rank-0): rank-1 suppressed, rank-0 kept.
  - Contradicts edge between rank-0 and rank-1 (Incoming to rank-0, i.e., edge written from rank-1): rank-1 suppressed, rank-0 kept (verifying bidirectional check).
  - Contradicts edge between rank-0 and rank-3: rank-3 suppressed, rank-1 and rank-2 unaffected.
  - Contradicts edge between rank-2 and rank-3 only: rank-3 suppressed, rank-0 and rank-1 unaffected.
  - Rank-0 contradicts rank-2; rank-2 also contradicts rank-3: rank-2 and rank-3 both suppressed.
  - Non-Contradicts edges (CoAccess, Supports, Supersedes) between entries: no suppression.

**FR-14** — A positive integration test must exist in `search.rs` that:
  - Constructs entries with a known `Contradicts` edge between them (manually seeded into `TypedRelationGraph` via `build_typed_relation_graph`).
  - Runs the full search pipeline.
  - Asserts the higher-ranked entry appears in results.
  - Asserts the lower-ranked contradicting entry is absent from results.
  - This test is a mandatory gate and is not substitutable by the zero-regression eval gate (SR-05).

**FR-15** — Integration tests in `search.rs` that set up `Contradicts` edges must not use the `create_graph_edges_table` helper from `unimatrix-store` (which reflects a pre-v13 schema). Graph setup must use `build_typed_relation_graph` with in-memory `EntryRecord` fixtures and a hand-constructed edge slice (SR-07 safe path).

---

## Non-Functional Requirements

**NFR-01** — **Performance**: `suppress_contradicts` is O(n * degree_c) where n is the result set size (≤ `k`, typically ≤ 20) and degree_c is the number of `Contradicts` edges per node (expected < 3 in production). The function must not perform any I/O, allocation beyond the output `Vec<bool>`, or store reads.

**NFR-02** — **Zero-regression**: MRR, P@K, and score distribution across all existing eval scenarios must be unchanged. Existing scenarios have no `Contradicts` edges in the test DB; suppression is a provable no-op for all of them.

**NFR-03** — **No new dependencies**: The implementation uses only `petgraph` (already a dependency of `unimatrix-engine`) via the existing `edges_of_type` API. No new crates may be added.

**NFR-04** — **No schema changes**: `Contradicts` edges already exist in the `GRAPH_EDGES` table (schema v13+). No migration is required.

**NFR-05** — **Observability floor**: Before #412 ships audit-log visibility for suppression events, the DEBUG log line (FR-09) is the only operator-visible trace of suppression. The log line must include both the suppressed entry ID and the contradicting entry ID so operators can correlate missing results with specific edge pairs.

**NFR-06** — **No config toggle**: Suppression is unconditionally active whenever `use_fallback = false` and the result set contains at least one conflicting pair. No feature flag, env var, or config parameter controls this behavior.

**NFR-07** — **File-size budget**: Every modified file must remain within 500 lines after changes. `graph.rs` is currently ~588 lines; the placement decision (FR-12) must account for this before implementation.

---

## Acceptance Criteria

**AC-01** — `suppress_contradicts(result_ids, graph)` returns a `Vec<bool>` of the same length as `result_ids`. For an empty graph or a result set with no `Contradicts` edges, all values are `true`.
*Verification*: unit test in `graph_tests.rs` — empty graph case and no-Contradicts case.

**AC-02** — For two entries in the result set connected by a `Contradicts` edge (Outgoing direction), the lower-ranked entry (higher index in sorted order) is suppressed (`false`); the higher-ranked entry is retained (`true`).
*Verification*: unit test — rank-0 → rank-1 Outgoing Contradicts edge; rank-1 must be `false`.

**AC-03** — For two entries connected by a `Contradicts` edge in the Incoming direction (edge written from the lower-ranked toward the higher-ranked), the lower-ranked entry is still suppressed.
*Verification*: unit test — rank-1 → rank-0 Contradicts edge (Incoming from rank-0's perspective); rank-1 must be `false`.

**AC-04** — Non-`Contradicts` edges (`CoAccess`, `Supports`, `Supersedes`, `Prerequisite`) between result entries do not cause any suppression.
*Verification*: unit test — entries with Supports or CoAccess edges; all values `true`.

**AC-05** — When `use_fallback = true`, the suppression step in `SearchService::search` is skipped; all results pass through unchanged regardless of graph content.
*Verification*: existing cold-start test in `search.rs` continues to pass with no modifications (cold-start graph is empty, so suppression is vacuously a no-op; the explicit `use_fallback` guard must be confirmed present in code review).

**AC-06** — Zero-regression eval gate passes: MRR, P@K, and score distribution are unchanged across all existing eval scenarios.
*Verification*: eval harness run with `--distribution_change false` profile produces no regressions in `render_zero_regression` report.

**AC-07** — When a conflicting pair is present, the lower-ranked entry per pair is absent from the returned results. The result set length is reduced; no padding to `k` is applied.
*Verification*: positive integration test in `search.rs` (FR-14) — result count is `(expected_k - suppressed_count)`.

**AC-08** — `suppress_contradicts` is defined in `unimatrix-engine/src/graph.rs` or `graph_suppression.rs` (per FR-12 placement decision) and unit-tested in the corresponding test module.
*Verification*: code review confirms function location and test coverage for all cases in FR-13.

**AC-09** — All existing tests in `graph_tests.rs` and `search.rs` pass without modification.
*Verification*: `cargo test --workspace` green with no test removals or `#[ignore]` additions.

**AC-10** — `edges_of_type` is the sole call site for querying `Contradicts` neighbors inside `suppress_contradicts`. No direct `.edges_directed()` or `.neighbors_directed()` calls exist in the suppression function.
*Verification*: code review; `grep` for `.edges_directed\|.neighbors_directed` inside the suppression function returns no matches.

**AC-11** — A `tracing::debug!` log line is emitted for each suppressed entry, recording both the suppressed entry ID and the contradicting (retaining) entry ID.
*Verification*: code review confirms `debug!` call with both IDs; log output visible during manual testing with `RUST_LOG=debug`.

**AC-12** — `results_with_scores` and `final_scores` are filtered by the suppression mask in a single indexed pass. The two Vecs remain strictly parallel after suppression.
*Verification*: code review confirms a single-pass index-based filter (e.g., `retain_indexed` pattern or `zip` + `filter` over indices); no separate `.retain()` calls on each Vec independently.

---

## Domain Models

### Entities

**TypedRelationGraph** — A directed typed graph (`StableGraph`) built from `GRAPH_EDGES` on each background tick. Nodes are entry IDs (`u64`). Edges carry a `RelationEdge { relation_type: RelationType, weight: f64 }`. Used read-only on the search hot path; cloned once under a short read lock (Step 6, `search.rs`). Field `use_fallback: bool` on `TypedGraphState` signals whether the graph has been built at least once.

**RelationType** — Discriminant for edge semantics. Five variants: `Supersedes`, `Contradicts`, `Supports`, `CoAccess`, `Prerequisite`. Stored as strings in `GRAPH_EDGES.relation_type`. `Contradicts` edges are written by `run_post_store_nli` in `nli_detection.rs` when NLI contradiction score exceeds threshold.

**edges_of_type** — The sole filter boundary on `TypedRelationGraph` (SR-01). Accepts a `NodeIndex`, a `RelationType`, and a `petgraph::Direction`. Returns an iterator over matching `EdgeReference` values. All traversal in the engine must call this method; direct `inner.edges_directed()` is prohibited at traversal sites.

**suppress_contradicts** — Pure function (no I/O, no side effects) that takes a slice of entry IDs in descending rank order and a `&TypedRelationGraph`, and returns a `Vec<bool>` keep/drop mask. Entry `i` is marked `false` if any surviving higher-ranked entry `j < i` has a `Contradicts` edge to/from entry `i` in the graph.

**SearchService::search — Step 10b** — The new insertion point after Step 10 (similarity/confidence floors) and before Step 11 (`ScoredEntry` construction). Reads the `use_fallback` flag; skips entirely when `true`. Calls `suppress_contradicts`; applies the returned mask to both `results_with_scores` and `final_scores` in one indexed pass.

**Contradicts edge direction invariant** — `nli_detection.rs` writes `(source_id, neighbor_id, 'Contradicts')` with `source_id` being the new entry and `neighbor_id` being its detected contradiction target. The reverse edge is never written. A contradiction between entries A and B has exactly one edge, in whichever direction happened to be written at detection time. Bidirectional checking (both `Direction::Outgoing` and `Direction::Incoming`) is therefore required for correctness, not merely safety.

### Ubiquitous Language

| Term | Meaning |
|------|---------|
| Collision | Two entries in the same search result set connected by a `Contradicts` edge |
| Suppression | Removal of the lower-ranked member of a colliding pair from the result set |
| Cold-start | `use_fallback = true`; `TypedRelationGraph` has not yet been built by the background tick |
| Surviving entry | A result set entry not suppressed; its score and rank are unchanged |
| Parallel Vec invariant | The requirement that `results_with_scores` and `final_scores` remain co-indexed after any mutation |

---

## User Workflows

### Workflow 1: Search with no Contradicts edges (normal path)

1. Agent calls `context_search` or `context_briefing`.
2. `SearchService::search` executes Steps 5–10.
3. Step 10b: `use_fallback = false`; `suppress_contradicts` called; no edges between result entries; returns all-`true` mask.
4. Both Vecs unchanged.
5. Step 11: `ScoredEntry` list constructed and returned.
6. Agent receives full `k`-length ranked result. Behavior identical to pre-col-030.

### Workflow 2: Search with a Contradicts collision in results

1. Agent calls `context_search` or `context_briefing`.
2. `SearchService::search` executes Steps 5–10. Entries A (rank 0, higher score) and B (rank 2, lower score) are both in the result set. A `Contradicts` edge exists between them (either direction).
3. Step 10b: `suppress_contradicts` called. Mask: `[true, true, false, ...]` (B at index 2 suppressed).
4. `DEBUG: suppressed entry_id=<B> contradicted_by=<A>` emitted via `tracing::debug!`.
5. Both `results_with_scores` and `final_scores` filtered in one indexed pass. B removed from both.
6. Step 11: `ScoredEntry` list constructed without B. Result set has `k-1` entries.
7. Agent receives collision-free ranked result.

### Workflow 3: Cold-start (use_fallback = true)

1. Agent calls `context_search` immediately after server start, before the first background tick completes.
2. `SearchService::search` executes Steps 5–10.
3. Step 10b: `use_fallback = true`; suppression step skipped entirely.
4. Step 11: `ScoredEntry` list constructed from full (unsuppressed) result set.
5. Agent receives full result set. No collision suppression until the graph is built.

---

## Constraints

- **SR-01 boundary**: `edges_of_type` is the only permitted call for querying graph neighbors in suppression logic. No direct `petgraph` traversal calls at suppression sites.
- **SR-02 parallel Vec invariant**: `results_with_scores` and `final_scores` must be co-filtered in a single indexed pass. Any indexing bug produces silent score-to-entry misalignment.
- **SR-07 test helper**: `create_graph_edges_table` (in `unimatrix-store`) reflects a pre-v13 schema subset. Integration tests in `search.rs` that need `Contradicts` edges must not use it; use `build_typed_relation_graph` with in-memory fixtures instead.
- **SR-08 atomicity**: The `use_fallback` flag transition (from `true` to `false`) happens when the background tick writes a new `TypedGraphState` under the write lock. The search hot path acquires the read lock and clones the flag atomically with the graph. No partial-graph race is possible given the existing lock discipline.
- **500-line file limit**: per `rust-workspace.md`. `graph.rs` is at ~588 lines before this feature; placement of `suppress_contradicts` must be decided before implementation (FR-12, SR-03, SR-06).
- **Scope of suppression**: `context_search` and `context_briefing` only (both route through `SearchService::search`). `context_lookup` (deterministic ID fetch) and `context_get` (single-entry fetch) are explicitly excluded.
- **No scoring changes**: suppressed entries are removed; surviving entries' scores are not adjusted, re-ranked, or re-normalized.
- **No new Contradicts writes**: this feature only reads existing `Contradicts` edges from `TypedRelationGraph`. The NLI write path is unchanged.
- **No audit log enrichment**: suppressed-entry count in `AUDIT_LOG` is deferred (no `Contradicts` edges in eval DB; audit data would be vacuously empty until #412 ships). Deferred as a follow-up on #395.

---

## Dependencies

| Dependency | Type | Notes |
|-----------|------|-------|
| `petgraph` | Crate (existing) | `Direction::Outgoing`, `Direction::Incoming`, `NodeIndex`, `EdgeReference`. Already a dep of `unimatrix-engine`. |
| `TypedRelationGraph` | Internal | `unimatrix-engine/src/graph.rs`. Provides `edges_of_type`. Already cloned at Step 6 in `search.rs`. |
| `RelationType::Contradicts` | Internal | Already defined in `graph.rs`. Already loaded from `GRAPH_EDGES` by `build_typed_relation_graph`. |
| `TypedGraphStateHandle` | Internal | `unimatrix-server/src/services/typed_graph.rs`. Already read at Step 6 in `search.rs`. |
| `use_fallback` flag | Internal | Field on `TypedGraphState`. Already cloned at Step 6 in `search.rs`. |
| `tracing` | Crate (existing) | For `debug!` observability log (FR-09, NFR-05). |
| Eval harness | Internal | `eval/runner/`, `eval/report/render_zero_regression.rs`. Existing; no changes needed. |
| `build_typed_relation_graph` | Internal | Used in integration tests for seeding `Contradicts` edges via in-memory fixtures (SR-07 safe path). |

---

## NOT in Scope

- **`context_lookup` and `context_get`**: deterministic single-entry fetch; no ranking, no collision. Contradiction warnings on single-entry fetch are a separate feature.
- **Scoring changes**: no penalty applied to contradicting entries; no re-ranking of survivors.
- **New `Contradicts` edge writes**: NLI detection path is unchanged.
- **PPR (Personalized PageRank)**: feature #398. col-030 is its predecessor stepping stone.
- **Graph cohesion metrics in `context_status`**: feature #413. Separate.
- **Config toggle**: no feature flag; suppression is always-on when `use_fallback = false`.
- **Audit log suppression count**: deferred; no `Contradicts` edges in eval DB until #412.
- **Eval scenario coverage for suppression behavior**: JSONL scenarios with `Contradicts` edges are deferred. The integration test in `search.rs` (AC-07, FR-14) is sufficient for this feature.
- **Bootstrap-only edges**: `GRAPH_EDGES` rows with `bootstrap_only = true` are already excluded from `TypedRelationGraph.inner` by `build_typed_relation_graph` (Pass 2b). No suppression-specific handling required.

---

## Open Questions

**OQ-01** — **File placement (SR-06, FR-12)**: `graph.rs` is at ~588 lines before this feature. Should `suppress_contradicts` live in `graph.rs` inline (if line count allows) or in a new `graph_suppression.rs` sibling module?

*Status*: **Resolved — see ARCHITECTURE.md ADR-001.** Function goes in `crates/unimatrix-engine/src/graph_suppression.rs`, re-exported from `graph.rs` via `pub use graph_suppression::suppress_contradicts`. Unit tests go in `graph_suppression.rs` under `#[cfg(test)]` (NOT `graph_tests.rs` — 1,068 lines; see R-01 in RISK-TEST-STRATEGY.md).

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — Returned entries #3616 and #3624 directly relevant: entry #3616 confirms Step 10b insertion point and `use_fallback` guard pattern; entry #3624 confirms mandatory positive integration test as a non-optional gate for suppression features. Entries #605, #925 (delivery patterns) noted as context. Entries #3298, #2907, #2935 (observation/detection) not directly applicable.
