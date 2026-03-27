# col-030: Contradicts Collision Suppression — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/col-030/SCOPE.md |
| Scope Risk Assessment | product/features/col-030/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/col-030/architecture/ARCHITECTURE.md |
| Specification | product/features/col-030/specification/SPECIFICATION.md |
| Risk Test Strategy | product/features/col-030/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-030/ALIGNMENT-REPORT.md |

---

## Goal

Insert a post-scoring `Contradicts` collision suppression filter into `SearchService::search`
(Step 10b) that removes the lower-ranked member of any result pair connected by a `Contradicts`
edge in `TypedRelationGraph`, preventing the search pipeline from surfacing contradictory
knowledge to the same agent in a single response. The suppression function is implemented as a
pure function in a new `graph_suppression.rs` module, re-exported from `graph.rs`, leaving all
scoring logic unchanged and passing the zero-regression eval gate across all existing scenarios.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| `suppress_contradicts` (`graph_suppression.rs`) | pseudocode/graph_suppression.md | test-plan/graph_suppression.md |
| Step 10b insertion (`search.rs`) | pseudocode/search_step10b.md | test-plan/search_step10b.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions Table

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| File placement for `suppress_contradicts` | New sibling module `graph_suppression.rs`; re-exported from `graph.rs` via `pub use graph_suppression::suppress_contradicts`. `lib.rs` gets no new entry. | SR-03, SR-06, OQ-01 resolved by architect | architecture/ADR-001-graph-suppression-module-split.md |
| Graph traversal boundary | `edges_of_type` is the sole call site for querying Contradicts neighbors. Direct `.edges_directed()` or `.neighbors_directed()` calls are prohibited inside `suppress_contradicts`. | SR-01 | architecture/ADR-002-edges-of-type-boundary.md |
| Contradicts edge direction | Both `Direction::Outgoing` and `Direction::Incoming` must be queried per candidate entry. NLI writes edges unidirectionally; the direction is non-deterministic from suppression's perspective. | ADR-003 | architecture/ADR-003-bidirectional-contradicts-query.md |
| Parallel Vec mask application | Single indexed `enumerate()` pass over zip of aligned prefix `results_with_scores` / `final_scores[..aligned_len]`. No separate `retain` calls on each Vec. `aligned_len = results_with_scores.len()`. | SR-02 | architecture/ADR-004-single-indexed-mask-application.md |
| Config toggle | No toggle. Suppression is unconditionally active when `use_fallback = false`. `if !use_fallback` is the only gating condition. | SR-04, NFR-06 | architecture/ADR-005-no-config-toggle.md |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-engine/src/graph_suppression.rs` | **Create** | Pure `suppress_contradicts` function + inline `#[cfg(test)]` unit tests (8 cases per FR-13) |
| `crates/unimatrix-engine/src/graph.rs` | **Modify** | Add `mod graph_suppression; pub use graph_suppression::suppress_contradicts;` — two lines only; no other changes |
| `crates/unimatrix-server/src/services/search.rs` | **Modify** | Insert Step 10b block (cold-start guard, mask call, single-pass rebuild of both Vecs, DEBUG log) between Step 10 and Step 11; add positive integration test |

### Files explicitly NOT touched

- `crates/unimatrix-engine/src/graph_tests.rs` — already 1,068 lines; no new tests here (see Critical Trap R-01 below)
- `crates/unimatrix-engine/src/lib.rs` — no new `pub mod` entry (see R-09)
- `crates/unimatrix-server/src/services/typed_graph.rs` — `use_fallback` and `TypedGraphState` are unchanged
- All NLI detection, scoring, and schema files — out of scope

---

## Data Structures

### `TypedRelationGraph` (existing, `graph.rs`)
```rust
pub struct TypedRelationGraph {
    pub inner: StableGraph<u64, RelationEdge>,
    pub node_index: HashMap<u64, NodeIndex>,
}
```
Used read-only. Already cloned at Step 6 of `search.rs` (lines 611–619).

### `RelationType::Contradicts` (existing, `graph.rs`)
```rust
pub enum RelationType {
    Supersedes,
    Contradicts,   // written by nli_detection.rs; read by suppress_contradicts
    Supports,
    CoAccess,
    Prerequisite,
}
```

### `suppress_contradicts` return type
`Vec<bool>` — keep/drop bitmask, same length as `result_ids` input. `true` = keep, `false` = suppress.

### `results_with_scores` / `final_scores` (existing, `search.rs`)
- `results_with_scores: Vec<(EntryRecord, f64)>` — floor-filtered, sorted DESC by fused score (line 892)
- `final_scores: Vec<f64>` — parallel Vec, built from `scored` at line 893, NOT filtered by Step 10 floors; may be longer than `results_with_scores` after floors

The aligned prefix is `final_scores[..results_with_scores.len()]`. Step 10b operates on this prefix exclusively.

---

## Function Signatures

### New function — `graph_suppression.rs`
```rust
pub fn suppress_contradicts(
    result_ids: &[u64],
    graph: &TypedRelationGraph,
) -> Vec<bool>
```
Pure function. No I/O, no async, no store reads. Returns a `Vec<bool>` of exactly `result_ids.len()`.

### Step 10b pseudocode outline — `search.rs`
```
// Step 10b: Contradicts collision suppression.
if !use_fallback {
    let result_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();
    let keep_mask = suppress_contradicts(&result_ids, &typed_graph);
    let aligned_len = results_with_scores.len();   // NOT final_scores.len()
    let mut new_rws = Vec::with_capacity(aligned_len);
    let mut new_fs  = Vec::with_capacity(aligned_len);
    for (i, (rw, &fs)) in results_with_scores
        .iter()
        .zip(final_scores[..aligned_len].iter())
        .enumerate()
    {
        if keep_mask[i] {
            new_rws.push(rw.clone());
            new_fs.push(fs);
        } else {
            debug!(
                suppressed_entry_id = rw.0.id,
                contradicting_entry_id = <id of highest-ranked surviving entry that contradicts rw.0.id>,
                "contradicts collision suppression: entry suppressed"
            );
        }
    }
    results_with_scores = new_rws;
    let final_scores = new_fs;   // SHADOW — see Critical Trap R-03
}
```
The contradicting entry ID must be captured by `suppress_contradicts` or derived in this loop. FR-09 requires both IDs in the log line.

### `edges_of_type` (existing, `graph.rs:188`)
```rust
pub fn edges_of_type(
    &self,
    node_idx: NodeIndex,
    relation_type: RelationType,
    direction: Direction,
) -> impl Iterator<Item = EdgeReference<'_, RelationEdge>>
```
Called twice per candidate entry: once with `Direction::Outgoing`, once with `Direction::Incoming`.

---

## CRITICAL IMPLEMENTATION TRAPS

### R-01 (Critical): Test file placement — `graph_tests.rs` is 1,068 lines
Unit tests for `suppress_contradicts` **MUST** go in `graph_suppression.rs` under `#[cfg(test)]`,
**NOT** in `graph_tests.rs`. `graph_tests.rs` is already at 1,068 lines — double the 500-line
limit. Appending to it will cause a gate-3b rejection (entry #3580). The architecture document
originally directed tests to `graph_tests.rs`; this was corrected by ALIGNMENT-REPORT WARN-02.
The authoritative placement is `graph_suppression.rs` `#[cfg(test)]`.

### R-02 (High): Visibility of `suppress_contradicts` in `graph_suppression.rs`
The function **MUST** be declared `pub fn suppress_contradicts`, not `pub(super) fn` or private.
`pub(super)` compiles within the module but produces E0364/E0365 at the `pub use` re-export
in `graph.rs`. This is a compile-time error that is easily missed during initial authoring.

### R-03 (High): `final_scores` is `let` (immutable) at line 893 — shadow required
`final_scores` at line 893 of `search.rs` is a `let` binding, not `let mut`. Step 10b **MUST**
shadow it with `let final_scores = new_fs;` after mask application. If the implementation
agent adds `let mut` at line 893 instead, the architecture intent is violated. If the shadow
is omitted entirely, Step 11's zip silently pairs surviving `results_with_scores` entries with
stale (pre-suppression) `final_scores` entries, producing silently wrong output with no compile
error or panic.

---

## Constraints

| Constraint | Source | Detail |
|-----------|--------|--------|
| SR-01 boundary | ADR-002, FR-05 | All graph traversal in `suppress_contradicts` goes through `edges_of_type`. No direct `.edges_directed()` or `.neighbors_directed()` calls. |
| SR-02 parallel Vec invariant | ADR-004, FR-07 | `results_with_scores` and `final_scores` filtered in a single indexed pass. No separate `retain` on each Vec. |
| SR-07 test helper | FR-15 | Integration tests in `search.rs` must NOT use `create_graph_edges_table` (pre-v13 schema). Use `build_typed_relation_graph` with in-memory `GraphEdgeRow` fixtures. Seeds must use `bootstrap_only=false`. |
| Cold-start guard | FR-08, ADR-005 | `if !use_fallback` guard must be present and non-inverted. `use_fallback` is already in scope from Step 6 clone. |
| 500-line file limit | entry #161, NFR-07 | Every modified or created file must stay under 500 lines. `graph.rs` (587 lines) gains 2 lines; stays under limit. `graph_suppression.rs` starts fresh. |
| No new crates | NFR-03 | `petgraph` is already a dependency of `unimatrix-engine`. No new entries in any `Cargo.toml`. |
| No schema changes | NFR-04 | `Contradicts` edges already exist in `GRAPH_EDGES` (schema v13+). No migration. |
| No scoring changes | SCOPE.md | `compute_fused_score` and all weights are unchanged. Suppressed entries are removed; surviving entries' scores are not adjusted. |
| `lib.rs` untouched | ADR-001, R-09 | No `pub mod graph_suppression` entry in `lib.rs`. Module is private to `graph`. |
| No backfill to `k` | FR-11 | Result set length is reduced; no padding. |
| No audit log enrichment | SCOPE.md | Deferred follow-up on #395 once #412 ships. |
| Bidirectional query | ADR-003, FR-03 | Both `Direction::Outgoing` and `Direction::Incoming` queried per candidate. |

---

## Dependencies

| Dependency | Type | Notes |
|-----------|------|-------|
| `petgraph` | Crate (existing) | `Direction::Outgoing`, `Direction::Incoming`, `NodeIndex`, `EdgeReference`. Dep of `unimatrix-engine`. |
| `TypedRelationGraph` | Internal | `unimatrix-engine/src/graph.rs`. Provides `edges_of_type` and `node_index`. |
| `RelationType::Contradicts` | Internal | Enum variant in `graph.rs:69`. Already loaded from `GRAPH_EDGES` by `build_typed_relation_graph`. |
| `TypedGraphState.use_fallback` | Internal | `services/typed_graph.rs`. Cloned at Step 6 (`search.rs:619`). |
| `typed_graph` clone | Internal | Already in scope from Step 6 (`search.rs:611–619`). No new lock acquisition needed. |
| `results_with_scores` | Internal | `Vec<(EntryRecord, f64)>` at `search.rs:892`. |
| `final_scores` | Internal | `Vec<f64>` at `search.rs:893`. `let` binding — shadow with `let final_scores = new_fs` at Step 10b. |
| `build_typed_relation_graph` | Internal | Used in unit and integration tests to seed `Contradicts` edges via in-memory `GraphEdgeRow` slices. |
| `tracing` | Crate (existing) | `debug!` log at Step 10b (FR-09, NFR-05). |
| Eval harness | Internal | `eval/runner/`, `eval/report/render_zero_regression.rs`. Existing; no changes needed. |

---

## NOT in Scope

- `context_lookup` and `context_get` — deterministic single-entry fetch; no ranking, no collision
- Scoring changes — no penalty to contradicting entries; no re-ranking of survivors
- New `Contradicts` edge writes — NLI detection path unchanged
- PPR / Personalized PageRank — feature #398, downstream of col-030
- Graph cohesion metrics in `context_status` — feature #413
- Config toggle for suppression — explicitly prohibited (ADR-005, NFR-06)
- Audit log suppression count enrichment — deferred follow-up on #395
- Eval JSONL scenario coverage for suppression behavior — integration test in `search.rs` is sufficient; deferred to post-#412
- Bootstrap-only edges (`bootstrap_only=true`) — already excluded by `build_typed_relation_graph` Pass 2b; no suppression-specific handling

---

## Mandatory Test Gates

The zero-regression eval gate (AC-06) does NOT validate suppression correctness — all existing
eval scenarios have no `Contradicts` edges. The following are non-negotiable additional gates:

1. **Positive integration test (FR-14, AC-07)**: constructs entries with a known `Contradicts`
   edge via `build_typed_relation_graph` (not `create_graph_edges_table`); runs
   `SearchService::search`; asserts higher-ranked entry present and lower-ranked entry absent.
   Uses `bootstrap_only=false` on the seeded edge.

2. **Bidirectional unit test (AC-03)**: edge written as rank-1 → rank-0 (Incoming to rank-0's
   perspective); rank-1 must be suppressed. This is the critical test that catches a
   Outgoing-only implementation (R-05).

3. **Floor + suppression combo test (R-07)**: exercises both Step 10 similarity floor and
   Step 10b suppression in the same search call; asserts both surviving entry identity and
   `ScoredEntry.final_score` values are correct.

---

## Alignment Status

Vision guardian review: 4 PASS, 2 WARN. Both WARNs are resolved before this brief was issued.

| Check | Status | Resolution |
|-------|--------|------------|
| Vision Alignment | PASS | Directly advances "knowledge integrity" and "trustworthy retrieval" core vision. Closes the downstream half of the "Contradicts edges unused at retrieval" gap. |
| Milestone Fit | PASS | Correctly positioned as Wave 1A stepping stone before PPR (#398). No future-wave capabilities pre-implemented. |
| Scope Gaps | PASS | All SCOPE.md goals, non-goals, constraints, and AC-01–AC-10 represented in source docs. |
| Scope Additions | WARN — resolved | Architecture adds chain-suppression unit test case (rank-0→rank-2→rank-3) not enumerated in SCOPE.md. Derivation is correct per SCOPE.md's algorithm. Accepted. |
| Architecture Consistency | WARN — resolved | SPECIFICATION.md OQ-01 status block read "Unresolved" while ARCHITECTURE.md ADR-001 had resolved it. This brief is the authoritative delivery contract: OQ-01 is resolved — `suppress_contradicts` goes in `graph_suppression.rs` (ADR-001). Implementation agents must not treat OQ-01 as open. |
| Risk Completeness | PASS | RISK-TEST-STRATEGY covers SR-01 through SR-08 and adds 13 delivery-specific risks with full scenario coverage. |

WARN-02 (architecture directed unit tests to `graph_tests.rs`) is resolved by this brief: tests
go in `graph_suppression.rs` `#[cfg(test)]` (see Critical Trap R-01 above).
