# crt-030: Personalized PageRank — Architecture

## System Overview

Unimatrix's `context_search` pipeline today ranks candidates using a fused score over six
signals (similarity, NLI, confidence, co-access, utility, provenance). Graph edges in
`GRAPH_EDGES` — specifically `Supports`, `CoAccess`, and `Prerequisite` — are invisible to
retrieval: HNSW returns only directly similar entries, creating a self-reinforcing imbalance
where lesson-learned and outcome entries that support popular decision entries are never
surfaced and their confidence stays low.

crt-030 introduces **Personalized PageRank (PPR)** as Step 6d in the search pipeline.
PPR propagates relevance mass from the HNSW seed set through positive-edge chains
(Supports, CoAccess, Prerequisite), surfacing multi-hop neighbors that HNSW cannot reach.
PPR-surfaced entries join the candidate pool before co-access prefetch (Step 6c) and NLI
scoring (Step 7), so they receive the full treatment of every downstream signal.

This feature is strictly additive: no schema changes, no new fusion weight, no NLI pipeline
changes. PPR influence enters through pool expansion and score blending, not a separate
`FusionWeights` term.

## Component Breakdown

### Component 1: `graph_ppr.rs` — Pure PPR Function

**Location**: `crates/unimatrix-engine/src/graph_ppr.rs`
**Declared as**: `#[path = "graph_ppr.rs"] mod graph_ppr;` in `graph.rs`, then
`pub use graph_ppr::personalized_pagerank;`

Responsibilities:
- Implement `personalized_pagerank()` as a pure, synchronous, deterministic function.
- Traverse only Supports, CoAccess, and Prerequisite edges via `edges_of_type()` exclusively.
- Execute power iteration for exactly `iterations` steps (no early exit — determinism
  constraint).
- Accumulate contributions in node-ID-sorted order each iteration (correctness constraint,
  not optimization).
- Normalize out-degree using only positive-edge out-degree (Supports + CoAccess +
  Prerequisite). Nodes with zero positive out-degree receive teleportation mass only.
- Handle degenerate input: empty graph, no positive edges, zero-sum personalization vector.

This component has **no I/O, no async, no mutable global state**, following the exact
structural contract of `graph_suppression.rs` (col-030).

### Component 2: `search.rs` Step 6d — Pipeline Integration

**Location**: `crates/unimatrix-server/src/services/search.rs`

Responsibilities:
- Build the personalization vector from HNSW candidate scores weighted by
  `phase_affinity_score` (called directly — no `use_fallback` guard, per ADR-003).
- Normalize the personalization vector to sum 1.0.
- Call `personalized_pagerank()` and process the resulting score map.
- Blend PPR scores into existing HNSW candidates' similarity values.
- Expand the pool with PPR-only entries that exceed `ppr_inclusion_threshold`, capped at
  `ppr_max_expand`.
- Fetch PPR-only entries from store sequentially (up to `ppr_max_expand` async get calls).
- Guard the entire step: `if use_fallback { skip }` — zero cost, zero allocation.

This component inserts **between Step 6b and Step 6c**. Exact position:

```
Step 6b: Supersession candidate injection
Step 6d: PPR expansion (NEW — crt-030)
Step 6c: Co-access boost prefetch (over full expanded pool)
Step 7:  NLI scoring + fused scoring + sort + truncate
```

### Component 3: `config.rs` — InferenceConfig Extension

**Location**: `crates/unimatrix-server/src/infra/config.rs`

Responsibilities:
- Add five new fields to `InferenceConfig` with `#[serde(default = "fn")]` and doc-comments.
- Add five `fn default_*()` private functions returning compiled defaults.
- Add five validation checks in `InferenceConfig::validate()`.
- Update `Default::default()` to include all five new fields.
- Merge five new fields in the global+project config merge block.

## Component Interactions

```
search.rs (Step 6d)
  │
  ├── reads: typed_graph (TypedRelationGraph) — already cloned out from lock at line 638
  ├── reads: phase_freq_table handle (PhaseFreqTableHandle) — snapshot extracted before Step 6d
  │         calls phase_affinity_score() directly (no use_fallback guard — ADR-003 col-031)
  ├── reads: cfg.ppr_alpha, ppr_iterations, ppr_inclusion_threshold, ppr_blend_weight, ppr_max_expand
  ├── reads: entry_store.get() — sequential async fetches for PPR-only entries
  │
  └── calls: personalized_pagerank(&typed_graph, &seed_scores, alpha, iterations)
               │
               └── calls: typed_graph.edges_of_type(node, Supports, Incoming)
                           typed_graph.edges_of_type(node, CoAccess, Incoming)
                           typed_graph.edges_of_type(node, Prerequisite, Incoming)
                           (NEVER: .edges_directed() directly — AC-02)
```

Lock ordering at Step 6d is an extension of the col-031 chain (ADR-004 col-031):
```
EffectivenessStateHandle read  → released before Step 6
TypedGraphStateHandle read     → released at line 648 (before Step 6a)
PhaseFreqTableHandle read      → released before scoring loop (pre-loop snapshot at col-031 block)
```
Step 6d uses the already-cloned `typed_graph` (no additional lock). It reads `phase_affinity_score`
from the snapshot that the col-031 pre-loop block extracted. No new lock acquisition in Step 6d.

## Technology Decisions

See ADR files for full context. Summary:

| Decision | Choice | ADR |
|---|---|---|
| Module structure | `graph_ppr.rs` submodule of `graph.rs` | ADR-001 |
| PPR function signature | `(graph, seed_scores, alpha, iterations) -> HashMap<u64, f64>` | ADR-002 |
| Edge direction semantics | Incoming for Supports/Prerequisite/CoAccess | ADR-003 |
| Determinism mechanism | Node-ID-sorted accumulation per iteration | ADR-004 |
| Pipeline step position | Step 6d: after 6b, before 6c | ADR-005 |
| Personalization vector | hnsw_score × phase_affinity_score, cold-start → ×1.0 | ADR-006 |
| ppr_blend_weight dual role | Intentional; one "PPR trust" parameter | ADR-007 |
| Latency / RayonPool offload | Inline synchronous path only; offload deferred to follow-up (100K+ scale) | ADR-008 |
| PPR score map memory profile | Upper-bounded by node count; no traversal depth cap needed | ADR-009 |

## Integration Points

### Existing Components Consumed

| Component | How crt-030 Consumes It |
|---|---|
| `TypedRelationGraph` | Passed by reference to `personalized_pagerank`; cloned at line 638 of search.rs |
| `edges_of_type()` | Sole traversal method; used for Supports/CoAccess/Prerequisite |
| `TypedGraphState.use_fallback` | Step 6d guard: `if use_fallback { return; }` |
| `PhaseFreqTable.phase_affinity_score()` | Called directly (no guard) in personalization vector construction |
| `PhaseFreqTableHandle` | Already held by `SearchService`; snapshot already extracted pre-loop |
| `entry_store.get()` | Sequential async fetches for PPR-only pool entries |
| `InferenceConfig` | Source of all five PPR config fields |
| `RayonPool.spawn_with_timeout()` | Not used by crt-030; offload path deferred to follow-up issue (ADR-008) |

### New Public Surface

| Item | Location | Description |
|---|---|---|
| `personalized_pagerank` | `unimatrix_engine::graph` | Re-exported from `graph_ppr.rs` |
| `ppr_alpha` | `InferenceConfig` | Damping factor, default 0.85 |
| `ppr_iterations` | `InferenceConfig` | Iteration count, default 20 |
| `ppr_inclusion_threshold` | `InferenceConfig` | New-entry PPR score floor, default 0.05 |
| `ppr_blend_weight` | `InferenceConfig` | PPR trust weight, default 0.15 |
| `ppr_max_expand` | `InferenceConfig` | Pool expansion cap, default 50 |

### Non-Touched Components

The following are explicitly **not modified** by crt-030:

- `graph_penalty`, `find_terminal_active` — Supersedes-only functions, SR-01 applies
- `graph_suppression.rs` — Contradicts suppression, unrelated
- `FusedScoreInputs` struct — No new fusion weight (PPR enters via pool + blend)
- `FusionWeights` / `compute_fused_score` — Six-weight formula unchanged
- NLI pipeline — No changes
- Background graph inference tick (`nli_detection_tick.rs`) — No changes
- Schema / SQL — No changes; PPR operates on the pre-built in-memory graph

## Integration Surface

| Integration Point | Type / Signature | Source |
|---|---|---|
| `personalized_pagerank` | `fn(&TypedRelationGraph, &HashMap<u64, f64>, f64, usize) -> HashMap<u64, f64>` | `graph_ppr.rs` |
| `TypedRelationGraph.edges_of_type` | `fn(&self, NodeIndex, RelationType, Direction) -> impl Iterator<Item = EdgeReference<'_, RelationEdge>>` | `graph.rs:192` |
| `RelationEdge.weight` | `f32` | `graph.rs:119` — used as `weight as f64` in PPR accumulation |
| `TypedRelationGraph.node_index` | `HashMap<u64, NodeIndex>` | `graph.rs:171` |
| `TypedRelationGraph.inner` | `StableGraph<u64, RelationEdge>` | `graph.rs:169` — node-ID read via `graph.inner[node_idx]` |
| `PhaseFreqTable.phase_affinity_score` | `fn(&self, u64, &str, &str) -> f32` | `phase_freq_table.rs:194` |
| `PhaseFreqTable.use_fallback` | `bool` | `phase_freq_table.rs:58` — PPR does NOT guard on this; `phase_affinity_score` handles it internally |
| `TypedGraphState.use_fallback` | `bool` | `typed_graph.rs` — PPR DOES guard on this (cold-start / cycle condition) |
| `InferenceConfig.ppr_alpha` | `f64`, default `0.85`, range `(0.0, 1.0)` exclusive | `config.rs` (new) |
| `InferenceConfig.ppr_iterations` | `usize`, default `20`, range `[1, 100]` | `config.rs` (new) |
| `InferenceConfig.ppr_inclusion_threshold` | `f64`, default `0.05`, range `(0.0, 1.0)` exclusive | `config.rs` (new) |
| `InferenceConfig.ppr_blend_weight` | `f64`, default `0.15`, range `[0.0, 1.0]` inclusive | `config.rs` (new) |
| `InferenceConfig.ppr_max_expand` | `usize`, default `50`, range `[1, 500]` | `config.rs` (new) |
| `results_with_scores` | `Vec<(EntryRecord, f64)>` — modified in-place at Step 6d | `search.rs:621` |
| Step 6d position | After `Step 6b` (line ~755), before `Step 6c` (line ~757) | `search.rs` |

## Latency Budget (SR-01 and SR-02)

### Power Iteration Complexity

PPR power iteration cost is O(I × E_pos) where:
- I = `ppr_iterations` (default 20)
- E_pos = positive-edge count (Supports + CoAccess + Prerequisite edges)

Node-ID-sorted accumulation adds O(N log N) per iteration via a sort (or O(N) via a
pre-computed sorted key list, which is the implementation's choice).

### Scale Table

| Entry Count | Est. Positive Edges | PPR Wall Time (20 iters) | Step 6d Total (incl. fetch) | Offload? |
|---|---|---|---|---|
| 1K | ~1K–5K | < 0.1 ms | < 5 ms (50 fetches × ~0.1 ms) | No |
| 10K | ~10K–50K | < 1 ms | < 10 ms (50 fetches × ~0.1 ms) | No |
| 100K | ~100K–500K | ~10–50 ms | ~15–60 ms | Follow-up issue (deferred) |

These estimates assume in-memory SQLite fetches at 0.05–0.2 ms/entry and pure-Rust
iteration with HashMap operations.

### RayonPool Offload (SR-01) — DEFERRED

The offload path (`PPR_RAYON_OFFLOAD_THRESHOLD`) is **out of scope for crt-030**. crt-030
ships the inline synchronous call only:

```rust
let ppr_scores = personalized_pagerank(&typed_graph, &seed_scores, alpha, iterations);
```

The 100K-node scale estimate (~10–50 ms PPR computation) defines the trigger condition for
a follow-up issue. At current production scale (< 10K entries), the inline path adds < 1 ms
and there is no async starvation risk. The `PPR_RAYON_OFFLOAD_THRESHOLD` constant and
conditional branch will be introduced in the follow-up when 100K+ scale is reached.

See ADR-008 for the full deferral rationale.

**SR-02 (sequential fetch latency)**: At `ppr_max_expand = 50`, sequential store fetches
add at most 50 round-trips. With in-memory SQLite at sub-millisecond latency, this is
acceptable for v1. An upper bound of **10 ms added latency at 50 entries** is the
architectural ceiling. If store get latency grows (e.g., remote storage), the fetch
must be batched — that is a follow-up out of scope for crt-030.

## SR Risk Resolutions

### SR-03 (Step Order Contradiction) — Resolved

The authoritative pipeline order is: `6b → 6d (PPR) → 6c (co-access prefetch) → 7 (NLI)`.

Rationale: PPR must expand the pool before co-access prefetch so that PPR-surfaced entries
participate in the co-access boost. The Background Research section's alternative phrasing
("after co-access boost prefetch") is incorrect — Goals item 2 and Proposed Approach are
authoritative. This architecture document and all ADRs use the Goals-authoritative order.

### SR-06 (phase_affinity_score cold-start contract) — Resolved

Step 6d calls `phase_affinity_score()` **directly**, with no `use_fallback` guard from
`PhaseFreqTable`. The method returns `1.0` when `use_fallback = true` — a neutral multiplier
that degrades gracefully to HNSW-score-only seeds. This is the PPR cold-start contract
documented in ADR-003 col-031 (entry #3687) and in `phase_freq_table.rs:178-196`.

The `use_fallback` guard in fused scoring (col-031) guards `TypedGraphState.use_fallback`
to skip `phase_explicit_norm` — this is a different field and a different guard for a
different purpose. Do not confuse the two.

### SR-05 (PPR score map memory profile) — Resolved

The PPR score map holds one `f64` per reachable node. The theoretical maximum is all nodes
in the graph (HashMap<u64, f64>). At 100K nodes this is 100K × (8+8) = ~1.6 MB — well
within process memory budget. No traversal depth cap is needed; the score map is bounded
by node count, not edge density.

The `ppr_inclusion_threshold` filters candidates before insertion into the pool;
`ppr_max_expand` caps how many are fetched. Neither limits the PPR computation itself.
The pre-cap score map may be large (thousands of entries) before filtering in a dense
CoAccess graph, but this is a HashMap holding f64 values — heap cost is linear in node
count and is not a concern at any realistic knowledge base scale.

### SR-07 (PPR-only entries with synthetic similarity) — Resolved

PPR-only entries enter `results_with_scores` with `similarity = ppr_blend_weight × ppr_score`.
This is a synthetic value in `[0.0, ppr_blend_weight]` — with the default of 0.15, the
maximum synthetic similarity is 0.15. This value is lower than any real HNSW cosine
similarity that would normally enter the pool (HNSW typically returns scores in [0.3, 1.0]),
which means PPR-only entries naturally rank below HNSW candidates without special-casing.

The fused scorer receives `FusedScoreInputs { similarity: synthetic, ... }` and makes no
assumption about the provenance of the similarity score — it is a plain `f64` in `[0, 1]`.
NLI scoring also makes no assumption: it scores all entries in `results_with_scores` by
the query string, regardless of how they arrived. No special-casing is required or
permitted for PPR-only entries in the fused scorer or NLI step.

## Open Questions

None — all questions from SCOPE.md are resolved. The open items below are follow-ups
that are explicitly out of scope for crt-030:

1. **Batched store fetch for PPR entries**: If storage layer moves to remote, the 50
   sequential fetches become a latency cliff. Follow-up issue to be filed post-merge.

2. **ppr_inject_weight separate from ppr_blend_weight**: If operators need independent
   tuning of the blend weight (existing candidates) vs. inject weight (new PPR-only
   entries), a separate `ppr_inject_weight` field should be added. Deferred; dual-role
   is intentional and documented (ADR-007).

3. **#414 phase affinity dependency**: If #414 is not merged before crt-030, the
   `× 1.0` cold-start path is always active. A follow-up integration test verifying
   that #414 data is used when available (not just that fallback works) should be
   added post-merge.
