# Personalized PageRank for Multi-Hop Relevance Propagation

## Problem Statement

The search pipeline ranks candidates using a fused score over six signals (similarity, NLI,
confidence, co-access, utility, provenance). Graph edges in `GRAPH_EDGES` — specifically
`Supports`, `CoAccess`, and `Prerequisite` — are invisible to retrieval today. `HNSW` returns
only entries that are directly similar to the query embedding. Entries connected to high-scoring
candidates via one or more positive-edge hops can never surface even if they are semantically
relevant, because there is no path from the query to them.

This creates a self-reinforcing access imbalance: low-confidence `lesson-learned` and `outcome`
entries that support popular `decision` entries are never retrieved, never accessed, and their
confidence stays low. PPR breaks this loop by propagating relevance mass from the HNSW seed set
through positive-edge chains, with multi-hop decay and path additivity.

Additionally, `GRAPH_EDGES.CoAccess` edges (bootstrapped from the `co_access` table where
`count >= 3`) are currently dead weight in retrieval — the co-access boost in the fused scorer
reads `co_access` directly. PPR is what activates `GRAPH_EDGES.CoAccess` as a positive
relevance channel.

Affected: all `context_search` callers. Urgency: category access imbalance compounds over time.

## Goals

1. Implement `personalized_pagerank()` in `unimatrix-engine/src/graph.rs` using power iteration
   over positive edges (Supports, CoAccess, Prerequisite) only.
2. Run PPR in `context_search` after HNSW, before co-access prefetch and NLI. Pipeline order:
   HNSW → **PPR expansion (Step 6d)** → co-access prefetch (Step 6c) → NLI (Step 7).
3. Surface new candidates: entries with PPR score above `ppr_inclusion_threshold` and not already
   in the HNSW candidate pool are added to the pool (capped at `ppr_max_expand` top-N entries).
4. Blend PPR scores into existing HNSW candidates using `ppr_blend_weight` (pre-fusion, into
   similarity signal).
5. Construct the personalization vector as `hnsw_score × phase_affinity_score` for each HNSW
   candidate, with graceful cold-start fallback to `hnsw_score × 1.0` when no phase data is
   available (#414 dependency). Normalize to sum 1.0 before iteration.
6. Make all PPR parameters configurable via `InferenceConfig`: `ppr_alpha`, `ppr_iterations`,
   `ppr_inclusion_threshold`, `ppr_blend_weight`, `ppr_max_expand`.
7. Validate all five new config fields in `InferenceConfig::validate()` with structured errors.
8. Power iteration uses deterministic node-ID-sorted accumulation — correctness constraint.
9. Ensure PPR is fully bypassed (zero cost) when `use_fallback = true` (cold-start or cycle
   detected in graph).
10. Supersede issue #396 (depth-1 Supports expansion) — PPR is strictly more general; #396 does
    not need to land separately.

## Non-Goals

- PPR does not modify `graph_penalty`, `find_terminal_active`, or any supersession penalty logic.
  SR-01 (Supersedes-only for penalty traversal) is not relaxed — it is simply not applicable to
  the new PPR function.
- PPR does not read `Supersedes` or `Contradicts` edges. Those edge types are excluded by
  construction, not by SR-01 constraint extension.
- PPR does not replace the fused scoring formula. It expands and adjusts the candidate pool
  before scoring; it does not change `FusedScoreInputs` or `FusionWeights`.
- PPR does not add a new `FusedScoreInputs` term (no `w_ppr` weight in the six-weight sum).
  Influence is expressed through pool expansion and the HNSW similarity pre-blend, not a
  separate weight.
- No schema changes. All required edge data is already stored in `GRAPH_EDGES` via Pass 2b of
  `build_typed_relation_graph`.
- No changes to the NLI pipeline, contradiction suppression (col-030), or supersession injection.
- No background tick changes. PPR uses the pre-built `TypedGraphState` from the existing tick.
- Feature flags or runtime enable/disable toggle beyond the existing `use_fallback` guard.

## Background Research

### Existing TypedRelationGraph Infrastructure

`unimatrix-engine/src/graph.rs` provides:
- `TypedRelationGraph`: `StableGraph<u64, RelationEdge>` with `node_index: HashMap<u64, NodeIndex>`.
- `edges_of_type(node_idx, relation_type, direction)` — the sole traversal filter boundary (SR-01).
  All PPR traversal must use this method. Direct `.edges_directed()` calls in PPR are prohibited.
- Five edge types: `Supersedes`, `Contradicts`, `Supports`, `CoAccess`, `Prerequisite`.
  PPR uses the last three only.
- `RelationEdge.weight: f32` — already present; CoAccess edges are weighted by
  `count/MAX(count)` from the bootstrap path; Supports edges have weight 1.0 from NLI.
- `build_typed_relation_graph` includes all non-bootstrap-only non-Supersedes edges from
  `GRAPH_EDGES` in Pass 2b. CoAccess, Supports, and Prerequisite edges are already in the
  in-memory graph.
- `petgraph = { version = "0.8", default-features = false, features = ["stable_graph"] }` is
  already declared in `unimatrix-engine/Cargo.toml`. No new dependency needed.

### TypedGraphState and Search Hot Path

`unimatrix-server/src/services/typed_graph.rs`:
- `TypedGraphState` holds `typed_graph: TypedRelationGraph`, `all_entries: Vec<EntryRecord>`,
  and `use_fallback: bool`.
- The search hot path acquires a short read lock, clones all three fields, releases the lock,
  then proceeds with the clones. PPR must follow this same pattern — no lock held during
  computation.
- `use_fallback = true` on cold-start and when a Supersedes cycle is detected. Any PPR call
  must be guarded: `if !use_fallback { run ppr } else { skip }`.

### Search Pipeline Step Numbering

From `search.rs`:
- Step 3: embed query
- Step 5: HNSW search
- Step 6: fetch entries, quarantine filter
- Step 6a: status penalty marking
- Step 6b: supersession injection
- Step 6c: co-access boost prefetch
- Step 7: NLI scoring + fused score computation + sort + truncate
- Step 9: truncate to k (no-op safety)
- Step 10: floors
- Step 10b: Contradicts collision suppression (col-030)

PPR inserts as **Step 6d**: after Step 6b (supersession injection), BEFORE co-access prefetch
(Step 6c), before NLI (Step 7). Correct pipeline order: 6b → 6d (PPR expansion) → 6c
(co-access prefetch over full expanded pool) → 7 (NLI). PPR runs first so the full expanded
pool participates in co-access boost prefetch.

### InferenceConfig Pattern

`InferenceConfig` in `config.rs` is the established home for all search/inference config.
Recent additions (crt-024, crt-026, crt-029, col-031) all follow the same pattern:
- Field with `#[serde(default = "fn_name")]` and doc-comment
- Private `fn default_*()` function returning the default value
- Range/invariant check in `validate()` using `ConfigError::NliFieldOutOfRange` (reused for
  any inference field, not NLI-specific) or a new dedicated error variant
- Update to `Default::default()` impl

The PPR fields (`ppr_alpha`, `ppr_iterations`, `ppr_inclusion_threshold`, `ppr_blend_weight`)
follow this exact pattern.

### #396 Status

Issue #396 (depth-1 Supports expansion via `find_supports_neighbors`) is still open; no code
has landed. PPR strictly supersedes it. The spec calls for PPR to replace, not stack on top of,
#396. Implementing PPR makes #396 redundant — it can be closed after this feature lands.

### suppress_contradicts Pattern (col-030)

`graph_suppression.rs` provides the model for a new graph traversal module:
- Pure function, no I/O, deterministic
- Uses `edges_of_type` exclusively (both directions for Contradicts; PPR will need both
  directions for undirected edge types like CoAccess)
- Declared as a submodule of `graph.rs`, re-exported from there
- Unit tests in the same file

PPR should follow the same modular structure: a new `graph_ppr.rs` declared as a submodule
of `graph.rs` and re-exported, with unit tests inline.

### CoAccess Edge Direction

CoAccess edges in `GRAPH_EDGES` are loaded bidirectionally (A→B and B→A) from the bootstrap
path. PPR over CoAccess must traverse both `Direction::Outgoing` and `Direction::Incoming` per
node — same as `suppress_contradicts` does for Contradicts. Supports edges are directional
(source supports target), but for relevance propagation the direction should flow toward seeds
(i.e., traverse `Direction::Incoming` on a Supports edge from a seed).

The PR for issue #398 specifies: "edges `u→v` in {Supports, CoAccess, Prerequisite}". The
power iteration formula uses outgoing edges from each node. The personalization vector seeds the
query. Implementations must be careful about edge direction semantics: a Supports edge `A→B`
means A supports B (A provides evidence for B). For PPR to propagate relevance from B (seed)
back to A (discoverable entry), the traversal must follow `Direction::Incoming` on B to find A.
This is a key design decision to surface clearly in the implementation spec.

## Proposed Approach

### Layer 1: Pure PPR Function in unimatrix-engine

Add `graph_ppr.rs` as a submodule of `graph.rs`, exporting `personalized_pagerank`:

```
pub fn personalized_pagerank(
    graph: &TypedRelationGraph,
    seed_scores: &HashMap<u64, f64>,  // already phase-weighted, will be normalized
    alpha: f64,
    iterations: usize,
) -> HashMap<u64, f64>
```

**Personalization vector construction (caller responsibility, Step 6d in search.rs):**
- For each HNSW candidate: `personalization[id] = hnsw_score × phase_affinity_score`
  where `phase_affinity_score` comes from the #414 frequency table for the current agent's
  active phase. If #414 data is unavailable (cold-start): `phase_affinity_score = 1.0`.
- For entries not in HNSW pool: `personalization[id] = 0.0` (no prior).
- Normalize the vector to sum 1.0 before passing to `personalized_pagerank`.
- Zero-sum guard: if all values are 0.0 (degenerate), return empty map immediately.

**Algorithm:** Power iteration, `iterations` steps (exact count — no early exit — required for
determinism). Damping factor `alpha` (default 0.85).

**Edge direction semantics (resolved):**
- `Supports` edges: traverse `Direction::Incoming`. A Supports edge `A→B` means "A supports B".
  PPR mass flows B→A: when B (a decision) is a seed, A (a lesson-learned) is surfaced.
- `Prerequisite` edges: traverse `Direction::Incoming`. Same reasoning: "B requires A" — when B
  is a seed, A is surfaced.
- `CoAccess` edges: edges are stored bidirectionally (A→B and B→A from bootstrap). Traverse
  `Direction::Incoming` — same as Supports/Prerequisite for a consistent "pull from sources"
  model. Symmetric in effect since both directions exist.

**Determinism:** Accumulate contributions in node-ID-sorted order each iteration. This is a
correctness constraint, not a performance optimization.

**Out-degree normalization:** uses only positive-edge out-degree (Supports + CoAccess +
Prerequisite). Nodes with zero positive out-degree do not propagate forward (only receive
from teleportation). Edge weights from `RelationEdge.weight as f64` are used for weighted
propagation.

### Layer 2: Search Pipeline Integration in search.rs

**Revised pipeline ordering (approved):**
HNSW (Step 5) → fetch/filter (Step 6/6a/6b) → **PPR expansion (Step 6d, new)** →
co-access prefetch (Step 6c, renumbered) → NLI + fused scoring (Step 7)

This ordering ensures PPR-surfaced entries participate in co-access boost prefetch. The
co-access boost map is built over the full expanded pool (HNSW + PPR entrants), so any
PPR-surfaced entry with legitimate co-access history receives its boost.

Step 6d implementation:
1. If `use_fallback`, skip entirely — zero allocation, zero cost.
2. Build `seed_scores`: map HNSW candidate IDs to `hnsw_score × phase_affinity_score`.
   Use phase affinity from #414 frequency table if available; else `× 1.0`.
3. Normalize `seed_scores` to sum 1.0. If degenerate (all zero), skip PPR.
4. Call `personalized_pagerank(&typed_graph, &seed_scores, cfg.ppr_alpha, cfg.ppr_iterations)`.
5. For entries already in the pool: blend
   `new_sim = (1 - ppr_blend_weight) * sim + ppr_blend_weight * ppr_score`.
6. For entries in PPR output but not in pool (PPR score > `ppr_inclusion_threshold`):
   - Sort by PPR score descending, take top `ppr_max_expand` entries.
   - Fetch each entry from store (skip on error or quarantined status).
   - Assign initial similarity = `ppr_blend_weight × ppr_score` (PPR-only entries have no
     HNSW score, so the blend reduces to the PPR-only term). Note: `ppr_blend_weight` serves
     dual roles here — blending for existing candidates AND setting floor similarity for new
     ones. This is intentional: the weight represents "how much to trust PPR signal" in both
     cases. If independent tuning is needed, a separate `ppr_inject_weight` can be added later.
   - Push to `results_with_scores`.
7. Pool (now expanded) proceeds to Step 6c (co-access prefetch over full pool), then Step 7.

### Layer 3: Configuration

Five new fields in `InferenceConfig`:
- `ppr_alpha: f64` — default 0.85, range (0.0, 1.0) exclusive
- `ppr_iterations: usize` — default 20, range [1, 100]
- `ppr_inclusion_threshold: f64` — default 0.05, range (0.0, 1.0) exclusive
- `ppr_blend_weight: f64` — default 0.15, range [0.0, 1.0] inclusive
- `ppr_max_expand: usize` — default 50, range [1, 500]

`SearchService` receives these five values at construction (not the full `InferenceConfig`),
following the pattern of `nli_top_k`, `nli_enabled`, `fusion_weights`.

## Acceptance Criteria

- AC-01: `personalized_pagerank(graph, seed_scores, alpha, iterations) -> HashMap<u64, f64>` is
  implemented in `unimatrix-engine/src/graph_ppr.rs`, re-exported from `graph.rs`.
- AC-02: PPR uses `edges_of_type()` exclusively for all graph traversal — no direct
  `.edges_directed()` calls in `graph_ppr.rs`.
- AC-03: PPR traverses only `Supports`, `CoAccess`, and `Prerequisite` edges. `Supersedes` and
  `Contradicts` edges are excluded by construction.
- AC-04: SR-01 non-applicability is documented in the `personalized_pagerank` function comment:
  SR-01 constrains `graph_penalty` and `find_terminal_active` to Supersedes-only; it does not
  restrict new retrieval functions from using other edge types.
- AC-05: Power iteration runs exactly `iterations` steps (no early exit) — required for
  determinism. Accumulation is sorted by node ID each iteration (correctness constraint).
- AC-06: Personalization vector is constructed as `hnsw_score × phase_affinity_score`, then
  normalized to sum 1.0 before iteration. Phase affinity from #414 frequency table if available;
  else `× 1.0` (cold-start). Zero-sum guard: if all values are 0.0, return empty map.
- AC-07: Out-degree normalization uses only positive-edge out-degree (Supports + CoAccess +
  Prerequisite), not total degree. Nodes with zero positive out-degree do not propagate forward.
- AC-08: Edge direction: Supports and Prerequisite traverse `Direction::Incoming` (backward).
  CoAccess traverse `Direction::Incoming` (edges are stored bidirectionally so this is symmetric).
- AC-09: `ppr_alpha`, `ppr_iterations`, `ppr_inclusion_threshold`, `ppr_blend_weight`,
  `ppr_max_expand` are added to `InferenceConfig` with `#[serde(default)]`, validated in
  `validate()`, and present in `Default::default()`.
- AC-10: `ppr_alpha` out-of-range rejects at startup with a `ConfigError` naming the field.
  Same for all five PPR fields.
- AC-11: PPR is inserted as Step 6d in `search.rs` — after Step 6b (supersession injection),
  BEFORE Step 6c (co-access prefetch), before NLI (Step 7). Co-access prefetch runs over the
  full expanded pool (HNSW + PPR entrants).
- AC-12: When `use_fallback = true`, Step 6d is skipped entirely — zero cost, zero allocation.
  The candidate pool is unchanged. Behavior is bit-for-bit identical to pre-crt-030.
- AC-13: Entries with PPR score > `ppr_inclusion_threshold` not already in pool are sorted by
  PPR score descending, capped at `ppr_max_expand`, fetched from store, and added to
  `results_with_scores`. Quarantined and error-fetched entries are silently skipped.
- AC-14: PPR-only entries (not in HNSW pool) have initial similarity =
  `ppr_blend_weight × ppr_score` (no HNSW component).
- AC-15: Entries already in the pool have their similarity score blended:
  `new_sim = (1 - ppr_blend_weight) * sim + ppr_blend_weight * ppr_score`.
- AC-16: Unit tests in `graph_ppr.rs` cover: empty graph, graph with no positive edges, single
  seed with direct Supports neighbor (Incoming direction), multi-hop chain (A→B→C where C is
  seed), path additivity (two independent paths to seed accumulate), CoAccess edges traversed,
  Supersedes/Contradicts edges not traversed by PPR, deterministic output across multiple calls.
- AC-17: Integration test in `search.rs` (inline unit test module) verifies that a Supports
  neighbor of an HNSW candidate appears in the expanded pool when PPR score exceeds threshold.
- AC-18: `GRAPH_EDGES.CoAccess` edge participation is verified by a unit test asserting that a
  node connected only via CoAccess edges receives non-zero PPR mass from a seed.

## Constraints

- `petgraph = "0.8"` with `features = ["stable_graph"]` is the only graph library available in
  `unimatrix-engine`. The PPR implementation must not require additional petgraph features
  (no `serde-1`, `rayon`, `graphmap`, `matrix_graph`).
- Max 500 lines per file (Rust workspace rule). If `graph_ppr.rs` + tests exceeds this, split
  tests into `graph_ppr_tests.rs` following the `graph.rs` / `graph_tests.rs` pattern.
- The search hot path must not hold any lock during PPR computation. Clone `typed_graph` and
  `all_entries` out from under the lock (already done for existing graph traversal at line 638–649
  in `search.rs`). PPR computation happens after lock release.
- `context_search` has a `MCP_HANDLER_TIMEOUT` deadline. PPR must be synchronous (no async)
  and run within the Tokio thread's sync context. At 10K entries and ~50K edges, 20 iterations
  is < 1ms — no rayon offload needed. At 100K entries (~500K edges), consider using the existing
  `RayonPool::spawn_with_timeout` pattern if benchmarks show > 10ms.
- Entry fetch for PPR-surfaced candidates (AC-12) requires async store calls (`entry_store.get`).
  These are sequential within Step 6d; pool expansion is typically O(10s) of entries, so
  sequential fetch is acceptable. A future optimization may batch these, but is out of scope.
- The `FusionWeights` six-weight sum constraint (`<= 1.0`) must not be violated. PPR influence
  enters via pool expansion and score blending, not as a new fusion weight.
- `use_fallback` guard must be respected — PPR is strictly disabled during cold-start and cycle
  conditions. No fallback PPR path needed.
- No new SQL queries or schema changes. PPR operates entirely on the pre-built in-memory graph.
- crt-029 fields (`supports_candidate_threshold`, `supports_edge_threshold`, etc.) are for the
  background Supports edge write tick — distinct from PPR. There is no naming conflict, but
  the distinction must be clear in config documentation.

## Open Questions (all resolved)

1. **Edge direction for Supports**: **Resolved — Incoming (backward).** Supports A→B: PPR
   traverses Incoming on B to surface A. Same for Prerequisite. CoAccess is stored
   bidirectionally; Incoming traversal is symmetric in effect.

2. **co-access timing**: **Resolved — Step 6d before Step 6c.** PPR expands the pool first;
   co-access prefetch then runs over the full expanded pool so PPR entrants receive their boost.

3. **Pool explosion cap**: **Resolved — yes, add `ppr_max_expand` (default 50).** Sort PPR
   candidates by score descending, take top N above threshold.

4. **Determinism**: **Resolved — sort by node ID during accumulation.** Correctness constraint,
   not optional.

5. **Blend location**: **Resolved — pre-fusion, into similarity.** PPR influence enters via
   pool expansion and score blending before `compute_fused_score`. No new fusion weight.

6. **Prerequisite edges**: **Resolved — transparent inclusion from day one.** No code change
   needed when #412 begins producing Prerequisite edges.

## Tracking

GitHub Issue: #398
Supersedes: #396 (depth-1 Supports expansion — never landed; PPR is strictly more general)
Depends on: #414 (phase affinity frequency table — graceful cold-start if unavailable)
