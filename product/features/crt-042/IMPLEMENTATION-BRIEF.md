# crt-042: PPR Expander — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-042/SCOPE.md |
| Architecture | product/features/crt-042/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-042/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-042/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-042/ALIGNMENT-REPORT.md |

---

## Goal

Widen the PPR candidate pool by inserting a BFS graph expansion phase (Phase 0) before PPR
personalization vector construction. HNSW k=20 seeds are treated as traversal starting points;
`graph_expand` collects reachable entry IDs via positive edges in the in-memory `TypedRelationGraph`
and merges them into `results_with_scores` so all candidates — seeds and expanded alike — receive
non-zero PPR personalization mass. The expander ships behind `ppr_expander_enabled = false` and
is gated by an A/B eval (MRR >= 0.2856, P@5 > 0.1115) before default enablement.

---

## BLOCKING GATES (must be resolved before writing Phase 0 code)

### SR-03 HARD GATE: S1/S2 Edge Directionality

crt-041 writes S1 (tag co-occurrence Informs) and S2 (structural vocabulary Informs) edges
**single-direction only** (source_id < target_id convention — confirmed in
`graph_enrichment_tick.rs` line 92: `t2.entry_id > t1.entry_id`). S8 CoAccess edges also use
`a = min(ids), b = max(ids)` (line 330). With Outgoing-only traversal, seeds in the higher-ID
position cannot reach their lower-ID partners via S1/S2 edges.

**Required action before Phase 0 implementation:**
1. Query `GRAPH_EDGES` for `relation_type = 'Informs'` rows. Confirm whether any symmetric
   reverse partners exist (bidirectional pairs).
2. If single-direction only: file a separate issue to back-fill bidirectional S1/S2 Informs
   edges at the crt-041 write site (same pattern as CoAccess back-fill, Unimatrix entry #3889).
   crt-042 must not ship until the back-fill issue is filed and a resolution path is confirmed.
3. Separately confirm S8 CoAccess directionality: crt-035 promotion tick writes both directions;
   confirm S8-only pairs (not yet promoted by tick) are handled.

### SR-01 INVESTIGATION: O(1) Embedding Lookup Path

`vector_store.get_embedding(id)` is O(N) per call (entry #3658). With up to 200 expanded
entries: 200 × O(7000) = ~1.4M f32 comparisons per search when fully expanded. This is the
primary latency risk.

**Required before implementing Phase 0 embedding lookup:**
Investigate whether `VectorIndex.id_map.entry_to_data` (O(1) HashMap: entry_id → data_id)
combined with direct HNSW layer-0 point vector access can provide O(1) embedding retrieval
by data_id, bypassing the full `IntoIterator` layer scan. Document the result in the PR:
- If O(1) path is feasible: implement it; the ≤50ms-delta latency gate is substantially relaxed.
- If O(1) requires significant rework: file a follow-up issue, proceed with O(N) path, and
  ensure the latency gate (P95 latency addition ≤ 50ms over pre-crt-042 baseline, measured
  from `debug!` traces) is evaluated before default enablement.

### Eval Gate Failure Owner

If the eval gate fails (MRR < 0.2856 or P@5 shows no improvement after S1/S2 bidirectionality
is confirmed), the investigation owner is the delivery lead for this feature. Decision path:
(a) confirm S1/S2 back-fill was applied before eval snapshot, (b) confirm Phase 0 insertion
point precedes Phase 1, (c) confirm BFS actually traverses edges from seeds. Timeline: the
flag remains `false` by default until the gate passes; no escalation path beyond re-running
the eval with corrected configuration.

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| graph_expand | pseudocode/graph_expand.md | test-plan/graph_expand.md |
| Phase 0 (search.rs) | pseudocode/phase0_search.md | test-plan/phase0_search.md |
| InferenceConfig additions | pseudocode/inference_config.md | test-plan/inference_config.md |
| Eval profile | pseudocode/eval_profile.md | test-plan/eval_profile.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions Table

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| `graph_expand` placement | New `graph_expand.rs` as `#[path]` submodule of `graph.rs`; re-exported via `pub use`. Not inline in search.rs or graph.rs. | Unimatrix #4049 | architecture/ADR-001-graph-expand-submodule-placement.md |
| Phase 0 insertion point | First block inside `if !use_fallback` in Step 6d, before Phase 1. After Steps 6a–6b. Before Step 6c (co-access prefetch). | Unimatrix #4050 | architecture/ADR-002-phase-0-insertion-point.md |
| Initial score for expanded entries | True cosine similarity via `vector_store.get_embedding()`. Constant floor rejected (removes semantic signal). PPR-derived score structurally impossible (circular). O(1) path investigation mandatory before O(N) fallback is accepted. | Unimatrix #4051 | architecture/ADR-003-cosine-similarity-source-for-expanded-entries.md |
| Config validation conditionality | Unconditional: `expansion_depth` and `max_expansion_candidates` validated at server start regardless of `ppr_expander_enabled`. NLI conditional-validation trap is not repeated. | Unimatrix #4052 | architecture/ADR-004-config-validation-unconditional.md |
| Latency instrumentation approach | `debug!` trace with wall-clock `elapsed_ms`, seed count, raw expanded count, final added count. `Instant::now()` only inside `if ppr_expander_enabled` branch — zero overhead on default path. Gate: P95 latency addition ≤ 50ms over pre-crt-042 baseline (measure baseline in same eval run). | Unimatrix #4053 | architecture/ADR-005-timing-instrumentation-approach.md |
| Traversal direction | Outgoing-only. Specified behaviorally: entry T surfaces when seed S exists and edge S→T exists. Bidirectionality solved at write side. `Direction::Both` rejected (inconsistent with graph_ppr.rs convention, write-side is correct fix). | Unimatrix #4054 | architecture/ADR-006-traversal-direction-outgoing-only.md |

---

## Files to Create/Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-engine/src/graph_expand.rs` | Create | `graph_expand` pure BFS function; module-level doc with behavioral contract and `edges_of_type()` invariant; inline unit tests (or split to `graph_expand_tests.rs` if >500 lines) |
| `crates/unimatrix-engine/src/graph.rs` | Modify | Add `#[path = "graph_expand.rs"] mod graph_expand;` and `pub use graph_expand::graph_expand;` |
| `crates/unimatrix-server/src/infra/config.rs` | Modify | Add three `InferenceConfig` fields: `ppr_expander_enabled`, `expansion_depth`, `max_expansion_candidates` at all four coordinated sites (struct, Default, serde fn, validate()) |
| `crates/unimatrix-server/src/services/search.rs` | Modify | Add three `SearchService` fields; wire in `new()`; insert Phase 0 block (BFS call, quarantine filter, cosine score, `debug!` trace) inside `if !use_fallback` before Phase 1 |
| `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` | Create | Eval profile A: `ppr_expander_enabled = true`, `expansion_depth = 2`, `max_expansion_candidates = 200` |

Optional (if `graph_expand.rs` inline tests exceed 500 lines):

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-engine/src/graph_expand_tests.rs` | Create | Unit tests split from `graph_expand.rs` following `graph_ppr_tests.rs` pattern |

---

## Data Structures

### New `InferenceConfig` fields (infra/config.rs)

```rust
ppr_expander_enabled: bool,      // default false — feature flag
expansion_depth: usize,          // default 2 — BFS hop depth [1, 10]
max_expansion_candidates: usize, // default 200 — BFS candidate cap [1, 1000]
```

All three use `#[serde(default = "fn_name")]`. Default value functions and `Default::default()`
values must match atomically (entry #3817). All `InferenceConfig {}` struct literals in tests
must include all three fields or use `..Default::default()` (entries #2730, #4044).

### New `SearchService` fields (search.rs)

```rust
ppr_expander_enabled: bool,
expansion_depth: usize,
max_expansion_candidates: usize,
```

Wired from `InferenceConfig` in `SearchService::new()`, following the existing five-field PPR
wiring pattern (`ppr_alpha`, `ppr_iterations`, etc.).

### `graph_expand` return type

`HashSet<u64>` — entry IDs reachable from seeds, excluding seed IDs, capped at
`max_candidates`. O(1) membership test for subsequent quarantine/deduplication filtering.

---

## Function Signatures

### `graph_expand` (new — `unimatrix-engine/src/graph_expand.rs`)

```rust
pub fn graph_expand(
    graph: &TypedRelationGraph,
    seed_ids: &[u64],
    depth: usize,
    max_candidates: usize,
) -> HashSet<u64>
```

**Behavioral contract:**
- Entry T surfaces when seed S exists and edge S → T of type CoAccess, Supports, Informs,
  or Prerequisite exists (S points to T via Outgoing traversal).
- Entry C does NOT surface when seed B exists and only edge C → B exists (no reverse edge).
- Returns empty when: `seed_ids` is empty, graph has no nodes, or `depth = 0`.
- BFS frontier processed in sorted node-ID order (determinism per ADR-004 crt-030).
- Visited-set prevents revisiting nodes (prevents oscillation on bidirectional CoAccess edges).
- All traversal via `edges_of_type()` exclusively — no direct `.edges_directed()` or
  `.neighbors_directed()` calls (SR-01, entry #3627).
- Pure, synchronous, no I/O, no locking, no side effects.
- Excluded edge types: `Supersedes`, `Contradicts`.

### Phase 0 block (inline in `search.rs` Step 6d)

```
// Phase 0 [crt-042]: graph_expand — widen seed pool if ppr_expander_enabled
// Combined ceiling: Phase 0 max_expansion_candidates (200) + Phase 5 ppr_max_expand (50)
//   + HNSW k=20 = 270 maximum candidates before PPR scoring.
if self.ppr_expander_enabled {
    let phase0_start = std::time::Instant::now();
    let seed_ids: Vec<u64> = results_with_scores.iter().map(|(e, _)| e.id).collect();
    let expanded_ids = graph_expand(&typed_graph, &seed_ids, self.expansion_depth,
                                    self.max_expansion_candidates);
    let in_pool: HashSet<u64> = seed_ids.iter().copied().collect();
    let mut results_added = 0usize;
    for expanded_id in expanded_ids.iter().copied().sorted() {
        if in_pool.contains(&expanded_id) { continue; }
        let Ok(entry) = entry_store.get(expanded_id).await else { continue; };
        if SecurityGateway::is_quarantined(&entry.status) { continue; }
        let Some(emb) = vector_store.get_embedding(expanded_id).await else { continue; };
        let cosine_sim = cosine_similarity(&query_embedding, &emb);
        results_with_scores.push((entry, cosine_sim));
        results_added += 1;
    }
    tracing::debug!(
        expanded_count = expanded_ids.len(),
        fetched_count = results_added,
        elapsed_ms = phase0_start.elapsed().as_millis(),
        expansion_depth = self.expansion_depth,
        max_expansion_candidates = self.max_expansion_candidates,
        "Phase 0 (graph_expand) complete"
    );
}
// Phase 1: Build personalization vector (existing — now over seeds + expanded)
```

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | **SR-03 blocking gate**: S1/S2 directionality confirmed bidirectional (or back-fill filed) before any Phase 0 code is written. |
| C-02 | `vector_store.get_embedding()` is O(N) per call. O(1) path investigation is mandatory before accepting the O(N) fallback. |
| C-03 | All `TypedRelationGraph` traversal in `graph_expand.rs` must use `edges_of_type()` only. No `.edges_directed()` or `.neighbors_directed()` at new traversal sites (entry #3627). |
| C-04 | The typed graph read lock must be released before Phase 0 executes. `graph_expand` operates on the pre-cloned `typed_graph`. |
| C-05 | `graph_expand` must be synchronous and pure. Async calls (`entry_store.get`, `vector_store.get_embedding`) are in the `search.rs` Phase 0 caller, not inside the function. |
| C-06 | No SQLite schema migration. `InferenceConfig` fields use `#[serde(default)]`. `GRAPH_EDGES` is unchanged. |
| C-07 | No new `RelationType` variants or edge writes. This feature reads `TypedRelationGraph` only. |
| C-08 | `graph_expand` is hot-path only. It must not be invoked from the background tick. |
| C-09 | BFS sorted node-ID order creates a deterministic bias toward older (lower-ID) entries at budget boundary. Accepted and documented; future optimization (edge-weight sort) is post-measurement. |
| C-10 | `graph_expand.rs` must not exceed 500 lines. Tests split to `graph_expand_tests.rs` if needed. |
| C-11 | `ppr_expander_enabled = false` is the default in this feature. Default enablement is a separate post-eval decision. |

---

## Dependencies

### Crates

| Crate | Usage |
|-------|-------|
| `unimatrix-engine` | `TypedRelationGraph`, `edges_of_type`, `RelationType`, `NodeIndex`, `RelationEdge` |
| `unimatrix-server` | `InferenceConfig`, `SearchService`, `SecurityGateway::is_quarantined` |
| `petgraph` | `StableGraph` (via `edges_of_type` boundary only) |
| `std::collections::HashSet` | Return type of `graph_expand` |

### Existing Components Consumed

| Component | Role | Notes |
|-----------|------|-------|
| `personalized_pagerank` | Receives expanded pool as input | Unchanged — only the input pool widens |
| `graph_ppr.rs` | Submodule split pattern for `graph_expand.rs` | Follow exactly |
| `graph_suppression.rs` | Submodule split pattern for `graph_expand.rs` | Follow exactly |
| `SecurityGateway::is_quarantined` | Quarantine check in Phase 0 | Same as Phase 5 pattern |
| `entry_store.get()` | Async fetch for expanded entry IDs | Same as Phase 5 pattern |
| `vector_store.get_embedding()` | O(N) embedding lookup for cosine similarity | Primary latency driver — investigate O(1) path first |
| `InferenceConfig::validate()` | Extended with unconditional range checks for two new fields | Follows existing PPR validation block |
| `run_eval.py` | Eval harness; must accept new profile without modification | Profile adds `[inference]` section |

### External Prerequisites

| Prerequisite | Status | Blocking? |
|-------------|--------|-----------|
| crt-041 merged (S1/S2/S8 edges) | Must be merged; improvement magnitude depends on edge density | No (expander works on any graph density) |
| S1/S2 edge directionality confirmed bidirectional (AC-00 / SR-03) | Must be confirmed before Phase 0 code | Yes — HARD GATE |
| S1/S2 back-fill migration (if single-direction confirmed) | Back-fill issue filed + resolution path confirmed | Yes — HARD GATE (back-fill must precede eval snapshot) |

---

## NOT in Scope

- Any change to `personalized_pagerank` internals (`graph_ppr.rs`). PPR algorithm is unchanged.
- Any change to the fused scoring formula (weights, normalization, co-access boost).
- Writing new graph edges or adding new `RelationType` variants (crt-040/crt-041 scope).
- SQL neighbor queries at query time. `graph_expand` is in-memory only.
- Schema migration. `GRAPH_EDGES` and all SQLite tables are unchanged.
- Expander invocation from the background tick.
- `TypedGraphState::rebuild()` changes.
- Enabling `ppr_expander_enabled = true` as the default (post-eval decision).
- Goal-conditioned or behavioral signal integration (Groups 5/6 scope).
- Batch embedding lookup or O(1) index-based embedding retrieval (future optimization if
  O(1) investigation shows it requires significant rework).
- Sorting BFS frontier by edge weight (future optimization, post SR-02 follow-up).

---

## Alignment Status

Source: ALIGNMENT-REPORT.md (reviewed 2026-04-02, agent: crt-042-vision-guardian)

**Overall: PASS with two WARNs requiring human attention.**

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — directly advances Wave 1A intelligence pipeline goals |
| Milestone Fit | PASS — correct Cortical-phase retrieval improvement, no Wave 3 scope pulled in |
| Scope Gaps | WARN-1 (accepted — see below) |
| Scope Additions | WARN-2 (human confirmation required — see below) |
| Architecture Consistency | PASS — all six ADRs present, combined ceiling documented |
| Risk Completeness | PASS — all SCOPE-RISK-ASSESSMENT items mapped |

### WARN-1: Traversal behavioral contract precision (ACCEPTED — no action needed)

SCOPE.md AC-03 states traversal is Outgoing-only but is silent on the behavioral consequence for
backward edges. The specification (AC-04) adds the precise behavioral statement: entry C does NOT
surface when seed B exists and only edge C→B exists. This is a necessary elaboration for correct
implementation and test coverage. No scope change required. Accepted.

### WARN-2: Latency ceiling — RESOLVED

Confirmed: gate is **P95 latency addition ≤ 50ms over pre-crt-042 baseline** — a delta, not an
absolute. Measure the baseline (expander disabled) in the same eval run; the gate is the addition.
ARCHITECTURE.md §Latency Profile updated accordingly. No further action required.

### WARN-3: Architecture behavioral contract — RESOLVED

ARCHITECTURE.md §Component 1 behavioral contract has been corrected. The erroneous example
("edge A→B surfaces A") has been replaced with the correct statement: entries pointing TO seeds
(A→B) are NOT surfaced by graph_expand — they remain available to PPR's reverse walk in Phase 2.
SPECIFICATION.md AC-04 is authoritative for traversal semantics. Both documents are now consistent.
