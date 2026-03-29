# SPECIFICATION: crt-030 — Personalized PageRank for Multi-Hop Relevance Propagation

**Feature ID:** crt-030
**GitHub Issue:** #398
**Supersedes:** #396 (depth-1 Supports expansion — never landed)
**Depends on:** #414 (phase affinity frequency table — graceful cold-start if unavailable)

---

## Objective

Graph edges in `GRAPH_EDGES` — `Supports`, `CoAccess`, and `Prerequisite` — are invisible to
`context_search` retrieval today. HNSW returns only entries that are directly similar to the query
embedding; entries connected to high-scoring candidates via one or more positive-edge hops cannot
surface through HNSW alone. This feature adds a Personalized PageRank (PPR) step to the search
pipeline that propagates relevance mass from the HNSW seed set through positive-edge chains,
surfaces new graph-reachable candidates, and blends PPR scores into the existing candidate pool
before co-access prefetch and NLI scoring.

---

## Functional Requirements

### FR-01: PPR Function

A pure function `personalized_pagerank` is implemented in `unimatrix-engine/src/graph_ppr.rs`
with the following signature:

```
pub fn personalized_pagerank(
    graph: &TypedRelationGraph,
    seed_scores: &HashMap<u64, f64>,
    alpha: f64,
    iterations: usize,
) -> HashMap<u64, f64>
```

- `seed_scores` is the pre-normalized personalization vector (caller's responsibility to
  normalize before passing). The function must not re-normalize internally.
- Returns a score map from entry ID to PPR score for all reachable nodes.
- If `seed_scores` is empty or all-zero, the function returns an empty `HashMap` immediately
  without iterating.

### FR-02: Power Iteration Algorithm

The function executes power iteration for exactly `iterations` steps. There is no early-exit
convergence check. The formula per iteration is:

```
r_new[v] = (1 - alpha) * personalization[v]
           + alpha * SUM over u where edge u->v exists:
               (r[u] * edge_weight(u, v) / positive_out_degree(u))
```

Where:
- `alpha` is the damping factor (teleportation complement is `1 - alpha`).
- `positive_out_degree(u)` counts only `Supports`, `CoAccess`, and `Prerequisite` out-edges
  (not `Supersedes` or `Contradicts`). Nodes with zero positive out-degree do not propagate
  forward — they only receive from teleportation.
- `edge_weight(u, v)` is `RelationEdge.weight as f64`.
- Contribution accumulation order within each iteration is sorted by node ID (ascending).
  This is a correctness constraint for determinism, not a performance optimization.

### FR-03: Edge Type Inclusion

PPR traverses only three edge types: `Supports`, `CoAccess`, and `Prerequisite`. `Supersedes`
and `Contradicts` edges are excluded by construction — they are never passed to the traversal
logic.

All graph traversal inside `graph_ppr.rs` uses `edges_of_type()` exclusively. Direct
`.edges_directed()` calls in `graph_ppr.rs` are prohibited (SR-01 single filter-boundary
invariant, ADR-002 col-030).

The function doc comment must state: "SR-01 constrains `graph_penalty` and `find_terminal_active`
to Supersedes-only traversal; it does not restrict new retrieval functions from using other
edge types. PPR uses Supports, CoAccess, and Prerequisite only."

### FR-04: Edge Direction Semantics

Edge direction traversal for each type:

| Edge Type | Direction | Rationale |
|-----------|-----------|-----------|
| `Supports` | `Direction::Incoming` | Edge `A→B` means "A supports B". PPR mass flows B→A: when B (a decision) is a seed, A (a lesson-learned) is surfaced as supporting evidence. |
| `Prerequisite` | `Direction::Incoming` | Same reasoning: "B requires A" — when B is a seed, A is surfaced as prerequisite. |
| `CoAccess` | `Direction::Incoming` | CoAccess edges are stored bidirectionally (A→B and B→A from bootstrap). Incoming traversal is symmetric in effect since both directions exist. Consistent with the "pull from sources" model. |

### FR-05: Out-Degree Normalization

Out-degree for propagation normalization counts only positive-edge out-degree: the sum of
outgoing `Supports`, `CoAccess`, and `Prerequisite` edge weights from a node. Nodes with
zero positive out-degree receive teleportation mass only and do not propagate to neighbors.

### FR-06: Personalization Vector Construction (Step 6d, Caller Responsibility)

The caller (Step 6d in `search.rs`) constructs and normalizes the personalization vector
before passing it to `personalized_pagerank`:

1. For each HNSW candidate with ID `entry_id` and HNSW score `hnsw_score`:
   ```
   personalization[entry_id] = hnsw_score * phase_affinity_score(entry_id)
   ```
   where `phase_affinity_score` is called directly on the `PhaseFreqTable` for the current
   agent's active phase (from the #414 frequency table).

   **Critical contract (ADR-003 col-031, Unimatrix #3687):** `phase_affinity_score` must be
   called **without** a `use_fallback` guard in PPR. The method returns `1.0` on cold-start
   (when #414 data is unavailable, the phase is absent, or the entry is absent from the
   bucket). This `1.0` return is the neutral multiplier: `hnsw_score × 1.0 = hnsw_score`.
   No conditional logic around the call is needed or permitted in PPR's context. This is the
   SR-06 mitigation.

2. For entries not in the HNSW pool: `personalization[id] = 0.0` (implicit — absent from the
   seed map).

3. Normalize the seed map so all values sum to `1.0` before passing to `personalized_pagerank`.

4. Zero-sum guard: if the sum of all personalization values is `0.0` (degenerate case), skip
   PPR entirely and return an empty map without calling `personalized_pagerank`.

### FR-07: Search Pipeline Step 6d Integration

PPR is inserted as **Step 6d** in `search.rs`, immediately after Step 6b (supersession
injection) and before Step 6c (co-access prefetch). This ordering ensures PPR-surfaced entries
participate in co-access boost prefetch.

**Definitive step ordering (SR-03 resolution):**

```
Step 5:  HNSW search
Step 6:  fetch entries, quarantine filter
Step 6a: status penalty marking
Step 6b: supersession injection
Step 6d: PPR expansion  ← NEW (this feature)
Step 6c: co-access boost prefetch (over full expanded pool)
Step 7:  NLI scoring + fused score computation + sort + truncate
Step 9:  truncate to k (no-op safety)
Step 10: floors
Step 10b: Contradicts collision suppression (col-030)
```

The Background Research section of SCOPE.md contains a stale description placing PPR "after
co-access boost prefetch (Step 6c)". That description is incorrect. The Goals section and
Proposed Approach are authoritative: the correct order is 6b → 6d → 6c → 7.

### FR-08: Step 6d Implementation — Full Algorithm

When Step 6d executes:

1. **Fallback guard:** If `use_fallback == true`, skip Step 6d entirely — zero allocation,
   zero cost. The candidate pool is unchanged. Behavior is bit-for-bit identical to
   pre-crt-030. Proceed to Step 6c.

2. **Build seed scores:** For each candidate in `results_with_scores`, extract `entry_id` and
   `hnsw_score`. Call `phase_affinity_score(entry_id)` directly (no `use_fallback` guard — see
   FR-06). Compute `seed = hnsw_score * phase_affinity_score`.

3. **Normalize:** Sum all seed values. If sum is `0.0`, skip to Step 6c. Otherwise divide
   each value by the sum.

4. **Run PPR:** Call `personalized_pagerank(&typed_graph, &seed_scores, cfg.ppr_alpha,
   cfg.ppr_iterations)`.

5. **Blend existing candidates:** For each entry already in `results_with_scores` that also
   appears in the PPR output map:
   ```
   new_sim = (1.0 - ppr_blend_weight) * current_sim + ppr_blend_weight * ppr_score
   ```
   Update the entry's similarity score in-place.

6. **Expand with new candidates:** From entries in PPR output that are NOT already in the pool
   and whose PPR score exceeds `ppr_inclusion_threshold`:
   - Sort these entries by PPR score descending.
   - Take the top `ppr_max_expand` entries (cap).
   - For each, fetch from store (`entry_store.get(entry_id)` — sequential async calls).
   - If fetch returns an error: silently skip.
   - If the fetched entry has a quarantined status: silently skip.
   - Assign initial similarity:
     ```
     initial_sim = ppr_blend_weight * ppr_score
     ```
     This is the PPR-only entry's synthetic similarity. There is no HNSW component, so the
     blend formula reduces to the PPR-only term. `ppr_blend_weight` serves a dual role here:
     it blends scores for existing candidates AND sets the floor similarity for newly injected
     entries. This dual role is intentional — the weight represents "how much to trust PPR
     signal" uniformly. If independent tuning becomes needed in the future, a separate
     `ppr_inject_weight` field may be added; this is explicitly out of scope for crt-030.
   - Push to `results_with_scores`.

7. **Proceed to Step 6c:** Co-access prefetch now runs over the full expanded pool (HNSW
   candidates plus PPR-surfaced entrants).

### FR-09: PPR-Only Entry Score Treatment (SR-07)

PPR-only entries (injected in FR-08 step 6, not from HNSW) carry an initial similarity of
`ppr_blend_weight × ppr_score`. This is a synthetic value in the range `[0.0, ppr_blend_weight]`.

The fused scorer (`compute_fused_score`), the NLI step (Step 7), and all downstream consumers
must treat this synthetic similarity identically to a real HNSW similarity score. No
special-casing, no origin flags, no skip logic for PPR-only entries is permitted. The fused
scorer and NLI step make no assumptions about similarity score provenance — they process
whatever float value is in the similarity field.

### FR-10: Module Structure

`graph_ppr.rs` is a submodule of `graph.rs`, following the `graph_suppression.rs` pattern:
- Declared as `mod graph_ppr;` in `graph.rs`
- Re-exported: `pub use graph_ppr::personalized_pagerank;` from `graph.rs`
- Does NOT appear independently in `lib.rs`
- Unit tests inline in `graph_ppr.rs` (or in `graph_ppr_tests.rs` if file exceeds 500 lines)

### FR-11: InferenceConfig Extension

Five new fields are added to `InferenceConfig` in `config.rs`, following the established
pattern (serde default function + private default fn + validate() range check + Default update):

| Field | Type | Default | Valid Range | Error |
|-------|------|---------|-------------|-------|
| `ppr_alpha` | `f64` | `0.85` | `(0.0, 1.0)` exclusive | `ConfigError::NliFieldOutOfRange` naming "ppr_alpha" |
| `ppr_iterations` | `usize` | `20` | `[1, 100]` inclusive | `ConfigError::NliFieldOutOfRange` naming "ppr_iterations" |
| `ppr_inclusion_threshold` | `f64` | `0.05` | `(0.0, 1.0)` exclusive | `ConfigError::NliFieldOutOfRange` naming "ppr_inclusion_threshold" |
| `ppr_blend_weight` | `f64` | `0.15` | `[0.0, 1.0]` inclusive | `ConfigError::NliFieldOutOfRange` naming "ppr_blend_weight" |
| `ppr_max_expand` | `usize` | `50` | `[1, 500]` inclusive | `ConfigError::NliFieldOutOfRange` naming "ppr_max_expand" |

Each field carries a `#[serde(default = "default_ppr_*")]` attribute and a doc-comment
explaining its role, valid range, and — for `ppr_blend_weight` specifically — its dual role
(blending for existing candidates, floor similarity for PPR-only entries).

`SearchService` receives these five values at construction (not the full `InferenceConfig`),
following the pattern established by `nli_top_k`, `nli_enabled`, and `fusion_weights`.

### FR-12: crt-029 Naming Disambiguation

The `crt-029` fields (`supports_candidate_threshold`, `supports_edge_threshold`, etc.) govern
the background tick that writes `Supports` edges into `GRAPH_EDGES`. They are entirely distinct
from the PPR query-time fields. This distinction must be explicit in InferenceConfig doc-comments
for the PPR fields to prevent operator confusion.

---

## Non-Functional Requirements

### NFR-01: Latency Budget — Step 6d

Step 6d (PPR computation, personalization vector build, blend, and expansion) must complete
within the following targets, measured in the Tokio thread's synchronous context:

| Scale Point | Nodes | Edges (est.) | Budget |
|-------------|-------|--------------|--------|
| Small | 1K | 5K | < 0.1 ms |
| Medium | 10K | 50K | < 1 ms |
| Large | 100K | 500K | < 10 ms |

At 10K nodes / 50K edges with 20 iterations, PPR computation is expected to be < 1 ms with
no offload needed. The inline synchronous path is the only implementation in crt-030. A
`RayonPool::spawn_with_timeout` offload branch for the 100K+ node case is a deferred
follow-up feature; it is out of scope for crt-030.

The sequential store fetch portion (FR-08 step 6) adds up to `ppr_max_expand` (default 50)
async round-trips. With sub-millisecond store get latency, this is acceptable for v1. If
storage layer changes (remote storage, container packaging) raise per-get latency, batch
fetch must be implemented. That optimization is out of scope for crt-030 but is the defined
follow-up if latency exceeds budget.

### NFR-02: Memory Allocation

PPR builds a `HashMap<u64, f64>` score map over all reachable nodes in the graph. With a
default `ppr_inclusion_threshold` of `0.05`, the vast majority of entries will score below
threshold and be filtered before expansion. The score map itself is bounded by the total
number of graph nodes (all entries with at least one positive edge). No additional
graph-traversal-depth cap is required in v1: the `ppr_inclusion_threshold` filter and
`ppr_max_expand` cap together bound the expansion work. If CoAccess edge density proves
unbounded in production (dense bootstrap from `co_access` where count >= 3), the score map
can grow large before threshold filtering. Monitoring CoAccess edge counts from crt-029 data
is a pre-launch validation requirement.

### NFR-03: Determinism

Given the same `TypedRelationGraph`, same `seed_scores`, same `alpha`, and same `iterations`,
`personalized_pagerank` must return identical output on every call. Node-ID-sorted accumulation
within each iteration is the mechanism that ensures this. This is a correctness requirement.

### NFR-04: Lock Discipline

PPR computation occurs after the `TypedGraphState` read lock is released. The search hot path
acquires a short read lock, clones `typed_graph`, `all_entries`, and `use_fallback`, releases
the lock, then proceeds. PPR must follow this same pattern — no lock is held during PPR
computation. No new lock acquisitions are permitted in Step 6d.

### NFR-05: No Schema Changes

PPR operates entirely on the pre-built in-memory `TypedRelationGraph`. No new SQL queries,
no new tables, no schema migrations.

### NFR-06: FusionWeights Integrity

The `FusionWeights` six-weight sum constraint (`<= 1.0`) must not be violated. PPR influence
enters via pool expansion and score blending (pre-fusion, into the similarity signal), not as
a new fusion weight. No `w_ppr` term is added to `FusedScoreInputs` or `FusionWeights`.

### NFR-07: Synchronous Execution

PPR computation is synchronous (no async, no Rayon). It runs within the Tokio thread's
synchronous execution context. The async store fetches for expansion (FR-08 step 6) are
sequential `.await` calls within the async search handler. A Rayon offload branch
(`PPR_RAYON_OFFLOAD_THRESHOLD`) is explicitly deferred and out of scope for crt-030.

### NFR-08: File Size

`graph_ppr.rs` must not exceed 500 lines (Rust workspace rule). If the implementation plus
inline tests exceeds this limit, tests are split into `graph_ppr_tests.rs` following the
`graph.rs` / `graph_tests.rs` pattern.

### NFR-09: petgraph Feature Set

The implementation uses `petgraph = "0.8"` with `features = ["stable_graph"]` only. No
additional petgraph features (`serde-1`, `rayon`, `graphmap`, `matrix_graph`) may be added in
crt-030. The `rayon` feature is not needed because the Rayon offload branch is deferred.

---

## Acceptance Criteria

### AC-01: PPR Function Location and Signature
`personalized_pagerank(graph: &TypedRelationGraph, seed_scores: &HashMap<u64, f64>, alpha: f64, iterations: usize) -> HashMap<u64, f64>` is implemented in `unimatrix-engine/src/graph_ppr.rs` and re-exported from `graph.rs`.

**Verification:** `grep -r "pub fn personalized_pagerank"` in `graph_ppr.rs`; `grep "pub use graph_ppr::personalized_pagerank"` in `graph.rs`.

### AC-02: edges_of_type Exclusivity
All graph traversal in `graph_ppr.rs` uses `edges_of_type()`. No direct `.edges_directed()` calls appear in `graph_ppr.rs`.

**Verification:** `grep "edges_directed" graph_ppr.rs` returns no results.

### AC-03: Edge Type Restriction
PPR traverses only `Supports`, `CoAccess`, and `Prerequisite` edges. `Supersedes` and `Contradicts` are excluded by construction. The `personalized_pagerank` function doc-comment states the SR-01 non-applicability (SR-01 constrains `graph_penalty` and `find_terminal_active`; it does not restrict new retrieval functions).

**Verification:** Code review; unit test T-PPR-08 asserts Supersedes/Contradicts edges produce zero PPR mass on non-seeded nodes.

### AC-04: SR-01 Non-Applicability Documentation
The `personalized_pagerank` function doc-comment includes explicit text stating: SR-01 constrains `graph_penalty` and `find_terminal_active` to `Supersedes`-only traversal; it does not restrict new retrieval functions from using other edge types.

**Verification:** Code review of doc-comment.

### AC-05: Deterministic Power Iteration
Power iteration runs exactly `iterations` steps (no early exit). Node-ID accumulation is sorted ascending each iteration.

**Verification:** Unit test T-PPR-09 calls `personalized_pagerank` twice with identical inputs and asserts output maps are equal. Code review confirms no convergence guard.

### AC-06: Personalization Vector Construction
The personalization vector in Step 6d is constructed as `hnsw_score × phase_affinity_score(entry_id)`. `phase_affinity_score` is called directly, without a `use_fallback` guard. On cold-start (#414 unavailable or phase absent), `phase_affinity_score` returns `1.0` (ADR-003 col-031, Unimatrix #3687). The vector is normalized to sum `1.0` before passing to `personalized_pagerank`. Zero-sum guard returns empty map without calling PPR.

**Verification:** Code review confirms no `use_fallback` guard wraps the `phase_affinity_score` call in Step 6d. Unit tests for cold-start path.

### AC-07: Out-Degree Normalization
Out-degree for propagation uses only positive-edge out-degree (Supports + CoAccess + Prerequisite edge weights). Nodes with zero positive out-degree do not propagate forward.

**Verification:** Unit test T-PPR-05 — a node with only `Supersedes` out-edges propagates zero mass to any neighbor.

### AC-08: Edge Direction
`Supports` and `Prerequisite` edges are traversed `Direction::Incoming`. `CoAccess` edges are traversed `Direction::Incoming` (symmetric due to bidirectional storage).

**Verification:** Unit test T-PPR-03 — a `Supports` edge `A→B` with B as seed produces non-zero PPR score for A (Incoming traversal from B finds A). Unit test T-PPR-06 — CoAccess edge traversal produces non-zero mass.

### AC-09: InferenceConfig Fields
`ppr_alpha`, `ppr_iterations`, `ppr_inclusion_threshold`, `ppr_blend_weight`, and `ppr_max_expand` are present in `InferenceConfig` with `#[serde(default)]`, validated in `validate()`, and present in `Default::default()`. `SearchService` constructor receives all five values.

**Verification:** TOML round-trip test (flat top-level fields, pattern from entry #3662). `Default::default()` produces values matching the specified defaults.

### AC-10: Config Validation Errors
All five PPR fields reject out-of-range values at startup with a `ConfigError` naming the specific field. Rejection occurs in `validate()` before the server starts serving requests.

**Verification:** Unit tests for each field with a boundary-violating value assert `Err(ConfigError::NliFieldOutOfRange { field: "ppr_alpha", ... })` (or equivalent).

### AC-11: Step 6d Pipeline Position
PPR is inserted as Step 6d in `search.rs`, after Step 6b (supersession injection), before Step 6c (co-access prefetch), before NLI (Step 7). The co-access prefetch at Step 6c runs over the full expanded pool (HNSW + PPR entrants).

**Verification:** Code review of `search.rs` step ordering. Integration test T-PPR-IT-01 verifies a PPR-surfaced entry receives a co-access boost (would not be possible if co-access ran before PPR).

### AC-12: use_fallback Guard
When `use_fallback == true`, Step 6d is skipped entirely — zero allocation, zero cost. The candidate pool, scores, and ordering are bit-for-bit identical to pre-crt-030 behavior.

**Verification:** Unit test asserts Step 6d is a no-op with `use_fallback = true`. The set of returned entry IDs must match a reference run with PPR compiled out.

### AC-13: Pool Expansion
Entries with PPR score `> ppr_inclusion_threshold` that are not already in the candidate pool are sorted by PPR score descending, capped at `ppr_max_expand`, fetched from store, and appended to `results_with_scores`. Quarantined entries and fetch errors are silently skipped.

**Verification:** Integration test T-PPR-IT-01. Unit test for quarantine skip behavior.

### AC-14: PPR-Only Entry Similarity
PPR-only entries (not in HNSW pool) have initial similarity `= ppr_blend_weight × ppr_score`.

**Verification:** Unit / integration test asserts `injected_entry.similarity == ppr_blend_weight * ppr_score` with tolerance `1e-9`.

### AC-15: Existing Candidate Score Blend
Entries already in the pool whose IDs appear in PPR output have their similarity score updated:
`new_sim = (1.0 - ppr_blend_weight) * current_sim + ppr_blend_weight * ppr_score`.

**Verification:** Unit test asserts blend formula is applied correctly with known inputs.

### AC-16: Phase Affinity Used When Available (SR-08)
When #414 is merged and `PhaseFreqTable` contains data for the current phase, `phase_affinity_score` returns values other than `1.0` for known entries, and the PPR personalization vector differs from the pure-HNSW-seeded baseline. A test seeded with a synthetic `PhaseFreqTable` containing non-uniform phase data must produce a different personalization vector than one seeded with a uniform (`1.0`) table.

**Verification:** Unit test with a mock `PhaseFreqTable` containing non-uniform scores for two entries asserts the resulting `seed_scores` differ from `{id: hnsw_score}` for those entries.

### AC-17: Integration Test — Supports Neighbor Surfaced
An integration test in `search.rs` (inline unit test module or `search_tests.rs`) constructs a `TypedRelationGraph` where entry B is an HNSW candidate and entry A has a `Supports` edge `A→B`. After Step 6d runs, entry A must appear in `results_with_scores` with PPR score exceeding `ppr_inclusion_threshold`.

**Verification:** Test asserts `results_with_scores.iter().any(|e| e.entry.id == A_id)` after Step 6d.

### AC-18: CoAccess Edge PPR Mass
A unit test in `graph_ppr.rs` asserts that a node connected to a seed exclusively via `CoAccess` edges (and no other edge types) receives non-zero PPR mass from that seed.

**Verification:** Test constructs a two-node graph (seed S, neighbor N) with a `CoAccess` edge `N→S`, seeds PPR with `{S: 1.0}`, and asserts `result[N] > 0.0`.

---

## Domain Models

### TypedRelationGraph
The in-memory graph held in `TypedGraphState`. Built by `build_typed_relation_graph` from
`all_entries: Vec<EntryRecord>` and `all_edges: Vec<GraphEdgeRow>`. Contains all non-bootstrap-only
non-`Supersedes` edges from `GRAPH_EDGES` (including `Supports`, `CoAccess`, `Prerequisite`,
`Contradicts`). Provides `edges_of_type(node_idx, relation_type, direction)` as the sole
traversal filter boundary.

### RelationEdge
An edge in `TypedRelationGraph`. Contains `weight: f32` (cast to `f64` within PPR computation).
`CoAccess` edges carry weight `count/MAX(count)` from bootstrap. `Supports` edges have weight `1.0`
from NLI. `Prerequisite` edges have weight `1.0`.

### Personalization Vector
A normalized `HashMap<u64, f64>` mapping HNSW candidate IDs to their phase-weighted HNSW scores.
Values sum to `1.0`. Non-seed entries are absent from the map (treated as `0.0` in the PPR formula).

### PPR Score Map
The output of `personalized_pagerank`: a `HashMap<u64, f64>` mapping node IDs to their steady-state
PPR scores. Scores are non-negative and reflect accumulated relevance mass from the personalization
vector after `iterations` steps of power iteration.

### HNSW Candidates
The set of `(EntryRecord, f64)` pairs in `results_with_scores` at the time Step 6d begins.
These are the direct retrieval results from the HNSW index with their similarity scores.

### PPR-Only Entry
An entry not in the HNSW candidate pool that is injected into `results_with_scores` by Step 6d
because its PPR score exceeds `ppr_inclusion_threshold`. Its similarity score is synthetic:
`ppr_blend_weight × ppr_score`. It is indistinguishable from an HNSW candidate in all downstream
processing.

### use_fallback
A boolean flag on `TypedGraphState`. Set `true` on cold-start (graph not yet built) and when a
`Supersedes` cycle is detected during graph build. When `true`, all graph-dependent steps
(Step 6d, Step 10b, `graph_penalty`, `find_terminal_active`) are bypassed.

### phase_affinity_score
A method on `PhaseFreqTable` (#414). Returns a rank-normalized `f32` in `[0.0, 1.0]` representing
how frequently entries in the current phase's category have been accessed. Returns `1.0` on
cold-start (phase absent, entry absent, or `PhaseFreqTable` not yet populated). This neutral return
is the PPR contract (ADR-003 col-031, Unimatrix #3687).

### ppr_blend_weight
An `InferenceConfig` field with dual role:
1. For HNSW candidates: blending coefficient — `(1 - w) * sim + w * ppr_score`.
2. For PPR-only entries: floor similarity coefficient — `w * ppr_score`.
Both roles express the same semantic: "fraction of the score that PPR signal contributes."

---

## User Workflows

### Workflow 1: Normal Search with PPR Active

1. Caller invokes `context_search` with a query string and agent ID.
2. Step 3: query is embedded.
3. Step 5: HNSW returns top-K candidates with similarity scores.
4. Step 6/6a/6b: entries fetched, quarantine filtered, status penalties applied, superseded
   entries injected.
5. **Step 6d (PPR):** Personalization vector built from HNSW candidates using
   `phase_affinity_score`. Vector normalized. `personalized_pagerank` called. Existing candidate
   scores blended. New above-threshold candidates fetched and injected.
6. Step 6c: Co-access prefetch over full expanded pool (HNSW + PPR entrants).
7. Step 7: NLI scoring + fused score computation over all candidates (including PPR-only entries
   treated identically). Sort by fused score. Truncate.
8. Step 9/10/10b: Safety truncation, floors, Contradicts suppression.
9. Final results returned to caller. PPR-surfaced entries appear in results if their fused score
   ranks them within the top-K.

### Workflow 2: Cold-Start (use_fallback = true)

1. Graph not yet built (server just started, or Supersedes cycle detected).
2. Step 6d is skipped entirely — no allocation, no PPR call.
3. Search proceeds from Step 6c as in pre-crt-030 behavior.
4. Results are identical to pre-crt-030.

### Workflow 3: #414 Unavailable (phase_affinity cold-start)

1. `PhaseFreqTable` has no data for the current phase (or #414 has not yet merged).
2. Step 6d runs normally.
3. `phase_affinity_score(entry_id)` returns `1.0` for all entries.
4. Personalization vector is `hnsw_score × 1.0 = hnsw_score` for each HNSW candidate.
5. PPR is seeded purely from HNSW scores — still functional, just without phase personalization.
6. This degradation is silent by design (correct scores, reduced quality).

---

## Constraints

### C-01: No New petgraph Features
`petgraph = "0.8"` with `features = ["stable_graph"]` only. No `serde-1`, `rayon`, `graphmap`,
or `matrix_graph` features may be added.

### C-02: No Schema Changes
PPR uses the pre-built in-memory `TypedRelationGraph`. No new SQL queries, no new tables, no
migration files.

### C-03: No New Fusion Weight
PPR does not add a `w_ppr` term to `FusedScoreInputs` or `FusionWeights`. The six-weight sum
constraint (`<= 1.0`) is not modified. PPR influence enters exclusively through pool expansion
and similarity pre-blend.

### C-04: No Async in PPR Core
`personalized_pagerank` is a pure synchronous function. Async store fetches for expansion are
sequential `.await` calls in `search.rs`, not inside `graph_ppr.rs`.

### C-05: No Tick Changes
PPR uses the `TypedGraphState` rebuilt by the existing background tick. No tick modifications
are introduced by this feature.

### C-06: No NLI, Supersession, or Contradiction Changes
PPR does not modify `graph_penalty`, `find_terminal_active`, the NLI pipeline, contradiction
suppression (col-030), or supersession injection logic.

### C-07: No Feature Flag Toggle
There is no runtime enable/disable toggle for PPR beyond the existing `use_fallback` guard.
This follows the col-030 precedent (ADR-005 col-030, Unimatrix #3630).

### C-08: Lock-Free During Computation
No lock may be held during PPR computation. All needed state (`typed_graph`, `all_entries`,
`use_fallback`) is cloned from under the `TypedGraphState` read lock before Step 6d begins.

### C-09: 500-Line File Limit
`graph_ppr.rs` must not exceed 500 lines. Overflow goes to `graph_ppr_tests.rs`.

### C-10: Sequential Fetch in v1
Entry fetch for PPR-surfaced expansion candidates is sequential in v1. Batching is explicitly
deferred and is only required if per-get store latency exceeds sub-millisecond thresholds (e.g.,
due to remote storage changes). `ppr_max_expand` default of 50 establishes the worst-case
sequential fetch count.

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `petgraph = "0.8"` with `stable_graph` | Rust crate | Already declared in `unimatrix-engine/Cargo.toml`. No new features needed. |
| `TypedRelationGraph` / `TypedGraphState` | Internal | `unimatrix-engine/src/graph.rs`, `unimatrix-server/src/services/typed_graph.rs`. Already available in search hot path. |
| `edges_of_type` method | Internal | Sole traversal filter boundary. Already implemented in `graph.rs`. |
| `graph_suppression.rs` | Internal | Structural template for `graph_ppr.rs` (submodule pattern, doc-comment style, test helpers). |
| `suppress_contradicts` (col-030) | Internal | Establishes Step 10b pattern; PPR follows analogous guard-and-skip structure. |
| `InferenceConfig` / `ConfigError` | Internal | `config.rs` — home for all five new PPR fields. `ConfigError::NliFieldOutOfRange` reused for PPR field errors. |
| `PhaseFreqTable::phase_affinity_score` | Internal | #414 feature. Graceful cold-start (returns `1.0`) when unavailable. PPR calls directly without `use_fallback` guard. |
| `build_typed_relation_graph` Pass 2b | Internal | Already includes `CoAccess`, `Supports`, `Prerequisite` edges from `GRAPH_EDGES`. No changes needed. |

---

## NOT in Scope

- **#396 (depth-1 Supports expansion):** PPR is strictly more general. #396 can be closed
  after crt-030 lands without implementing separately.
- **`ppr_inject_weight` parameter:** Independent tuning of existing-candidate blend weight vs.
  PPR-only entry floor weight is deferred. One parameter (`ppr_blend_weight`) serves both roles
  in crt-030. A second parameter may be added in a future feature if production tuning reveals
  the need.
- **Rayon offload for PPR computation (`PPR_RAYON_OFFLOAD_THRESHOLD`):** Deferred to a
  follow-up feature. The inline synchronous path is the only implementation in crt-030. No
  Rayon parallelism is introduced by this feature.
- **Batch store fetch for expansion candidates:** Deferred. Only required if sequential fetch
  latency exceeds budget due to storage layer changes.
- **PPR in `context_briefing`:** PPR applies only to `context_search`. BriefingService has its
  own semantic search path (vnc-007 ADR-002); no PPR integration there.
- **Background PPR pre-computation:** PPR runs on-the-fly per query. No tick pre-computation.
- **Graph-traversal-depth cap:** No separate depth cap on PPR iteration traversal. The
  `ppr_inclusion_threshold` and `ppr_max_expand` cap bound expansion; they do not limit
  intermediate iteration traversal. The score map may contain thousands of entries before
  threshold filtering in a dense CoAccess graph — see NFR-02.
- **Prerequisite edge write path:** Transparent to PPR — PPR traverses `Prerequisite` edges
  when present. No code change is needed when #412 begins producing Prerequisite edges.
- **Any change to `FusedScoreInputs`, `FusionWeights`, or their weight sums.**
- **Any change to NLI thresholds or contradiction suppression logic (col-030).**
- **Any new MCP tool or change to `context_search` API surface.**

---

## Open Questions

None — all design questions from SCOPE.md are resolved. The following prior open items are
closed and their resolutions are reflected in this specification:

1. **Edge direction for Supports/Prerequisite:** Resolved — `Direction::Incoming` (backward).
   See FR-04.
2. **Co-access timing:** Resolved — Step 6d before Step 6c. See FR-07.
3. **Pool explosion cap:** Resolved — `ppr_max_expand` field, default 50. See FR-08.
4. **Determinism mechanism:** Resolved — node-ID-sorted accumulation. See FR-02.
5. **Blend location:** Resolved — pre-fusion, into similarity signal. See FR-08 steps 5-6.
6. **Step-order contradiction in SCOPE.md:** Resolved — SR-03 from risk assessment. Correct
   order is 6b → 6d → 6c → 7. Background Research section of SCOPE.md is stale. See FR-07.
7. **`ppr_blend_weight` dual role:** Resolved — intentional, documented. See FR-08 step 6
   and Domain Models. See SR-04.
8. **Phase affinity cold-start contract:** Resolved — `phase_affinity_score` returns `1.0`
   directly; no `use_fallback` guard in PPR. See FR-06 and ADR-003 col-031 (Unimatrix #3687).

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 12 entries; entries #3687 (ADR-003
  col-031 two cold-start contracts), #3699 (use_fallback guard pattern), #3677
  (PhaseFreqTable neutral score 1.0), #3685 (rank-based normalization), #3730 (search pipeline
  step numbering and PPR pattern), #3650 (TypedRelationGraph pattern), and #3627 (edges_of_type
  ADR-002) were directly applicable.
- Note on entry #3730: that entry's text describes PPR as slotting "after 6c, before Step 7"
  which is a stale description. This spec fixes the authoritative order as 6b → 6d → 6c → 7
  per SCOPE.md Goals section and Proposed Approach. That entry should be corrected post-spec.
