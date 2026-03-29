# crt-030 Implementation Brief — Personalized PageRank for Multi-Hop Relevance Propagation

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-030/SCOPE.md |
| Architecture | product/features/crt-030/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-030/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-030/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-030/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|------------|-----------|
| `graph_ppr.rs` | product/features/crt-030/pseudocode/graph_ppr.md | product/features/crt-030/test-plan/graph_ppr.md |
| `search.rs` Step 6d | product/features/crt-030/pseudocode/search_step_6d.md | product/features/crt-030/test-plan/search_step_6d.md |
| `config.rs` InferenceConfig extension | product/features/crt-030/pseudocode/config_ppr_fields.md | product/features/crt-030/test-plan/config_ppr_fields.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | product/features/crt-030/pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | product/features/crt-030/test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Add Personalized PageRank (PPR) as Step 6d in the `context_search` pipeline to propagate
relevance mass from the HNSW seed set through positive-edge chains (`Supports`, `CoAccess`,
`Prerequisite`), surfacing multi-hop neighbors that HNSW cannot reach. The feature breaks the
self-reinforcing access imbalance where `lesson-learned` and `outcome` entries supporting
popular `decision` entries are never retrieved and never gain confidence. PPR also activates
`GRAPH_EDGES.CoAccess` as a live relevance channel and supersedes the never-landed #396
depth-1 Supports expansion.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|------------|--------|----------|
| Module structure for PPR function | `graph_ppr.rs` as `#[path]` submodule of `graph.rs`, re-exported via `pub use graph_ppr::personalized_pagerank` — mirrors `graph_suppression.rs` pattern | SCOPE.md / ARCHITECTURE.md | architecture/ADR-001-graph-ppr-submodule-structure.md |
| PPR function signature | `fn personalized_pagerank(graph: &TypedRelationGraph, seed_scores: &HashMap<u64, f64>, alpha: f64, iterations: usize) -> HashMap<u64, f64>` — caller normalizes seed_scores; function runs exact iteration count | SPECIFICATION.md FR-01/FR-02 | architecture/ADR-002-ppr-function-signature.md |
| Edge direction semantics | All three positive edge types traverse `Direction::Incoming`. Supports A→B: seed B finds A. Prerequisite A→B: seed B finds A. CoAccess stored bidirectionally so Incoming is symmetric | SCOPE.md / SPECIFICATION.md FR-04 | architecture/ADR-003-edge-direction-semantics.md |
| Determinism mechanism | Pre-sort all node IDs ascending once before the iteration loop; reuse `Vec<u64>` across all iterations. Score maps remain `HashMap<u64, f64>` for O(1) access | SCOPE.md AC-05 / SPECIFICATION.md FR-02 | architecture/ADR-004-deterministic-accumulation.md |
| Pipeline step position | Step 6d inserts between Step 6b (supersession injection) and Step 6c (co-access prefetch). Final order: `6b → 6d (PPR) → 6c (co-access prefetch) → 7 (NLI)`. Background Research stale text corrected | SCOPE.md Goals item 2 / SPECIFICATION.md FR-07 | architecture/ADR-005-pipeline-step-position.md |
| Personalization vector construction | Read from the already-cloned `phase_snapshot` extracted by the col-031 pre-loop block. Compute `hnsw_score × phase_affinity` per HNSW candidate. No direct `phase_affinity_score()` call; no re-acquisition of `PhaseFreqTableHandle` lock. Cold-start: `× 1.0` (absent from snapshot) | SPECIFICATION.md FR-06 / ADR-003 col-031 | architecture/ADR-006-personalization-vector-construction.md |
| `ppr_blend_weight` dual role | Single parameter intentionally serves two roles: (1) blend coefficient for existing HNSW candidates; (2) floor similarity coefficient for PPR-only injected entries. Semantic unity: "how much to trust PPR signal." `ppr_inject_weight` deferred | SCOPE.md SR-04 resolved / SPECIFICATION.md FR-08 | architecture/ADR-007-ppr-blend-weight-dual-role.md |
| RayonPool offload | DEFERRED — crt-030 ships inline synchronous path only. No `PPR_RAYON_OFFLOAD_THRESHOLD` constant or conditional branch. 100K+ scale offload is a follow-up issue | SCOPE.md Constraints / ALIGNMENT-REPORT.md WARN-01 resolved | architecture/ADR-008-latency-rayon-offload.md |
| PPR score map memory profile | No traversal depth cap. Score map bounded by total node count (O(N)). `ppr_inclusion_threshold` and `ppr_max_expand` are the control surfaces for pool expansion, not the score map size | SCOPE.md SR-05 resolved | architecture/ADR-009-ppr-score-map-memory-profile.md |

---

## Files to Create / Modify

| Operation | Path | Summary |
|-----------|------|---------|
| Create | `crates/unimatrix-engine/src/graph_ppr.rs` | Pure PPR function with power iteration, `edges_of_type` exclusivity, node-ID-sorted accumulation, inline unit tests |
| Modify | `crates/unimatrix-engine/src/graph.rs` | Add `mod graph_ppr;` declaration and `pub use graph_ppr::personalized_pagerank;` re-export |
| Modify | `crates/unimatrix-server/src/services/search.rs` | Insert Step 6d block between Step 6b and Step 6c; update pipeline step comments |
| Modify | `crates/unimatrix-server/src/infra/config.rs` | Add five PPR fields to `InferenceConfig` with serde defaults, doc-comments, validation |

---

## Data Structures

### TypedRelationGraph (existing — `crates/unimatrix-engine/src/graph.rs`)
```
TypedRelationGraph {
    inner: StableGraph<u64, RelationEdge>,   // node weight = entry ID
    node_index: HashMap<u64, NodeIndex>,      // entry ID → petgraph NodeIndex
}
```
PPR reads `node_index` keys for the sorted node ID list and traverses `inner` via `edges_of_type`.

### RelationEdge (existing — `crates/unimatrix-engine/src/graph.rs`)
```
RelationEdge {
    relation_type: RelationType,
    weight: f32,   // used as `weight as f64` in PPR
}
```
`CoAccess` edges: `weight = count/MAX(count)` from bootstrap. `Supports` and `Prerequisite`: `weight = 1.0`.

### Personalization Vector (new, local to Step 6d)
```
HashMap<u64, f64>   // entry_id → hnsw_score × phase_affinity, normalized to sum 1.0
```
Built from `results_with_scores` using the `phase_snapshot` already extracted by the col-031 pre-loop block.

### PPR Score Map (return type of `personalized_pagerank`)
```
HashMap<u64, f64>   // entry_id → steady-state PPR score after `iterations` steps
```
All reachable nodes; values non-negative; filtered by `ppr_inclusion_threshold` before expansion.

### Five new `InferenceConfig` fields
```
ppr_alpha:               f64    // default 0.85, range (0.0, 1.0) exclusive
ppr_iterations:          usize  // default 20,   range [1, 100] inclusive
ppr_inclusion_threshold: f64    // default 0.05, range (0.0, 1.0) exclusive
ppr_blend_weight:        f64    // default 0.15, range [0.0, 1.0] inclusive
ppr_max_expand:          usize  // default 50,   range [1, 500] inclusive
```

---

## Function Signatures

### `personalized_pagerank` (new — `graph_ppr.rs`)
```rust
/// Compute Personalized PageRank over positive edges (Supports, CoAccess, Prerequisite).
///
/// SR-01 constrains `graph_penalty` and `find_terminal_active` to Supersedes-only traversal;
/// it does not restrict new retrieval functions from using other edge types.
/// PPR uses Supports, CoAccess, and Prerequisite only.
///
/// `seed_scores` must be pre-normalized to sum 1.0 (caller responsibility).
/// Returns an empty HashMap if `seed_scores` is empty.
/// Runs exactly `iterations` steps (no early exit — determinism requirement).
pub fn personalized_pagerank(
    graph: &TypedRelationGraph,
    seed_scores: &HashMap<u64, f64>,
    alpha: f64,
    iterations: usize,
) -> HashMap<u64, f64>
```

**Power iteration formula** (per node v, step t+1):
```
score[v][t+1] = (1 - alpha) * personalization[v]
              + alpha * Σ_{u: u→v ∈ positive_edges} (weight[u→v] / out_degree_weight[u]) * score[u][t]
```
Where `personalization[v] = seed_scores.get(v).copied().unwrap_or(0.0)`.

**Determinism**: Pre-sort `graph.node_index.keys()` into `Vec<u64>` once before the loop (ADR-004). Inner loop iterates over the sorted Vec, not HashMap keys.

**Traversal**: `graph.edges_of_type(node_idx, RelationType::Supports, Direction::Incoming)`, same for `CoAccess` and `Prerequisite`. No `.edges_directed()` calls.

**Out-degree normalization**: `positive_out_degree(u)` = sum of `Supports`, `CoAccess`, `Prerequisite` outgoing edge weights. Nodes with zero positive out-degree do not propagate (receive teleportation mass only).

### Step 6d Block in `search.rs`
```rust
// Step 6d: PPR expansion (crt-030)
if !use_fallback {
    // 1. Build seed scores from phase_snapshot
    let mut seed_scores: HashMap<u64, f64> = ...;
    // 2. Normalize; zero-sum guard
    // 3. Call personalized_pagerank
    // 4. Blend scores for existing pool entries
    // 5. Expand pool with PPR-only entries above ppr_inclusion_threshold
    //    (sorted by score desc, capped at ppr_max_expand, quarantine-checked)
}
// Step 6c: co-access boost prefetch (over full expanded pool)
```

### Five `InferenceConfig` default functions (new — `config.rs`)
```rust
fn default_ppr_alpha() -> f64 { 0.85 }
fn default_ppr_iterations() -> usize { 20 }
fn default_ppr_inclusion_threshold() -> f64 { 0.05 }
fn default_ppr_blend_weight() -> f64 { 0.15 }
fn default_ppr_max_expand() -> usize { 50 }
```

---

## Constraints

| Constraint | Source |
|------------|--------|
| `petgraph = "0.8"` with `features = ["stable_graph"]` only — no `rayon`, `serde-1`, `graphmap`, `matrix_graph` features | SPECIFICATION.md C-01 / NFR-09 |
| No schema changes, no new SQL queries — PPR operates on the pre-built in-memory `TypedRelationGraph` | SCOPE.md Non-Goals / SPECIFICATION.md C-02 |
| No new `FusionWeights` term — PPR influence enters via pool expansion and similarity pre-blend only | SPECIFICATION.md C-03 / NFR-06 |
| `personalized_pagerank` must be pure synchronous (no async, no Rayon) | SPECIFICATION.md C-04 / NFR-07 |
| No tick changes — PPR uses the pre-built `TypedGraphState` from the existing tick | SPECIFICATION.md C-05 |
| `use_fallback = true` guard skips Step 6d entirely — zero allocation, zero cost. Bit-for-bit identical to pre-crt-030 | SCOPE.md AC-12 |
| All graph traversal in `graph_ppr.rs` uses `edges_of_type()` exclusively. Direct `.edges_directed()` calls are prohibited | SCOPE.md AC-02 |
| Node-ID-sorted accumulation per iteration is a correctness requirement (not optional for determinism) | SCOPE.md AC-05 / ADR-004 |
| No lock held during PPR computation — use cloned `typed_graph` already extracted before Step 6a | SPECIFICATION.md NFR-04 / C-08 |
| `graph_ppr.rs` max 500 lines — overflow to `graph_ppr_tests.rs` | SPECIFICATION.md NFR-08 / C-09 |
| Sequential store fetches for PPR expansion (max 50 entries) — batch fetch deferred | ADR-008 / SPECIFICATION.md C-10 |
| Step 6d uses the `phase_snapshot` already extracted by the col-031 pre-loop block (no new lock acquisition) | ADR-006 |
| Quarantine check must be applied to every entry fetched by Step 6d expansion — R-08 critical risk | RISK-TEST-STRATEGY.md R-08 |
| Inclusion threshold comparison is strictly greater-than (`>`) — not `>=` | SCOPE.md AC-13 / RISK-TEST-STRATEGY.md R-06 |
| Node-ID sort `Vec` constructed once before the iteration loop — not inside the loop | ADR-004 / RISK-TEST-STRATEGY.md R-04 |

---

## Dependencies

| Dependency | Version / Source | Notes |
|------------|-----------------|-------|
| `petgraph` | `0.8`, `stable_graph` feature only | Already in `unimatrix-engine/Cargo.toml` — no change |
| `#414` (phase affinity frequency table / `PhaseFreqTable`) | col-031 deliverable | Graceful cold-start (`× 1.0`) when unavailable. No code change when #414 merges — snapshot read pattern handles it transparently |
| `TypedRelationGraph` / `edges_of_type` | `unimatrix-engine/src/graph.rs` | Existing — PPR is a new consumer only |
| `TypedGraphState.use_fallback` | `unimatrix-server/src/services/typed_graph.rs` | PPR guards on this field (cold-start / Supersedes cycle) |
| `graph_suppression.rs` (col-030) | `unimatrix-engine/src/graph_suppression.rs` | Structural model for `graph_ppr.rs` (pure function, `edges_of_type` exclusive, inline tests) |
| `InferenceConfig` | `unimatrix-server/src/infra/config.rs` | Established home for all PPR config fields |

---

## NOT in Scope

- RayonPool offload (`PPR_RAYON_OFFLOAD_THRESHOLD`) — deferred to a follow-up issue for 100K+ scale (ADR-008)
- `ppr_inject_weight` separate from `ppr_blend_weight` — deferred; dual-role is intentional (ADR-007)
- Batched store fetch for PPR-only expansion entries — deferred; sequential is v1 policy (ADR-008, C-10)
- Full #414 integration test verifying phase data used in production — deferred post-merge
- `graph_penalty`, `find_terminal_active`, or any supersession penalty logic changes
- `Supersedes` and `Contradicts` edge traversal in PPR
- New `FusedScoreInputs` term or `FusionWeights` changes
- NLI pipeline, contradiction suppression (col-030), supersession injection changes
- Background tick changes
- Feature flag or runtime toggle for PPR beyond `use_fallback`
- Issue #396 (depth-1 Supports expansion) — PPR supersedes it; #396 closes after crt-030 merges

---

## Alignment Status

**All alignment checks PASS.** No variances requiring approval.

Source: product/features/crt-030/ALIGNMENT-REPORT.md (reviewed 2026-03-29, re-checked post Option B correction).

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | PPR directly advances Wave 1A intelligence pipeline goal; breaks access imbalance; activates CoAccess as a relevance channel |
| Milestone Fit | PASS | Squarely in Wave 1A; all prerequisite wave items (crt-024, crt-025, crt-027, crt-021, W1-2 RayonPool) are complete |
| Scope Gaps | PASS | All 10 SCOPE.md Goals and all 18 ACs addressed in SPECIFICATION.md FR/NFR entries |
| Scope Additions | PASS | RayonPool offload branch (WARN-01) consistently deferred across ARCHITECTURE.md (ADR-008), SPECIFICATION.md (NFR-07), and RISK-TEST-STRATEGY.md (R-01) |
| Architecture Consistency | PASS | Component breakdown, lock ordering, latency budget (scale table), and SR resolutions are internally consistent |
| Risk Completeness | PASS | R-01 (offload) correctly Deferred with zero test scenarios; R-08 (quarantine bypass) classified Critical with three explicit test scenarios |

**Critical risk carrying into implementation** (R-08): PPR-only entries injected into `results_with_scores` bypass the quarantine filter applied at Step 6 to HNSW entries. The Step 6d fetch must independently apply the quarantine check. This must have a dedicated test — not just implicit coverage. Failure to enforce this allows quarantined entries (withdrawn knowledge, poisoned entries) to appear in search results.

---

## Pipeline Step Order Reference

```
Step 5:   HNSW search
Step 6:   Fetch entries, quarantine filter
Step 6a:  Status penalty marking
Step 6b:  Supersession candidate injection
Step 6d:  PPR expansion  ← NEW (crt-030)
Step 6c:  Co-access boost prefetch (over full expanded pool)
Step 7:   NLI scoring + fused score computation + sort + truncate
Step 9:   Truncate to k (safety)
Step 10:  Floors
Step 10b: Contradicts collision suppression (col-030)
```

---

## Latency Budget (informational)

| Scale | Entries | Est. Positive Edges | PPR (20 iters) | Step 6d Total (incl. 50 fetches) | Offload? |
|-------|---------|---------------------|----------------|-----------------------------------|----------|
| Small | 1K | 1K–5K | < 0.1 ms | < 5 ms | No |
| Medium | 10K | 10K–50K | < 1 ms | < 10 ms | No |
| Large | 100K | 100K–500K | ~10–50 ms | ~15–60 ms | Follow-up issue |

Current production scale is < 10K entries. The inline synchronous path adds < 1 ms at current scale.
