# PPR Expander

## Problem Statement

The current PPR implementation in `search.rs` (Step 6d) is a **re-ranker**: it scores
entries already in the HNSW k=20 candidate pool and may inject additional entries reachable
from those seeds by graph traversal. The structural problem is that entries PPR can reach via
`GRAPH_EDGES` are frequently **outside k=20** — semantically distant entries (cross-category)
are exactly what the graph is designed to surface, and HNSW semantic similarity cannot reach
them by definition.

ASS-038 confirmed this with 6,738 graph edges: zero MRR delta. The behavioral ground truth
scenarios (ASS-039, 1,443 unique scenarios) confirmed it again. The bottleneck is architectural,
not graph density: PPR traversal is bounded by whatever HNSW happened to retrieve. Entries
relevant to the query via graph topology but distant in embedding space are invisible.

**Who is affected**: every retrieval request. All groups downstream (Group 3 graph enrichment,
Group 6 behavioral signals) produce zero retrieval improvement without the expander because
their edges connect entries outside k=20.

**Why now**: crt-041 (S1/S2/S8 edges) has shipped. Graph density has increased. The expander
is the unlock that makes all graph enrichment work produce measurable P@5 and MRR improvement.

---

## Goals

1. Change the retrieval pipeline so HNSW k=20 results are treated as **seeds**, not the full
   candidate pool.
2. Implement `graph_expand(seeds, depth, max_candidates)` — a BFS/DFS traversal of
   `TypedRelationGraph` starting from HNSW seed node IDs, collecting reachable entry IDs across
   positive edge types (CoAccess, Supports, Informs, Prerequisite) up to a configurable depth
   and candidate cap.
3. Merge expanded entry IDs with the HNSW seed set to form the full candidate pool (up to
   `hnsw_k + max_expansion_candidates` entries) before PPR scoring.
4. Run PPR over the expanded pool — all candidates (seeds + expanded) receive PPR scores and
   enter the fused scoring pass.
5. Gate the expander behind a config flag (`ppr_expander_enabled`, default `false`) so the
   feature can be measured in A/B eval before becoming the default.
6. Add `expansion_depth` (default 2) and `max_expansion_candidates` (default 200) to
   `InferenceConfig` as the operator control surface.
7. Deliver a new eval profile `ppr-expander-enabled.toml` alongside the feature for the eval
   gate measurement.
8. Preserve all existing security invariants: every entry injected via `graph_expand` must
   pass the quarantine check before entering the candidate pool.

---

## Non-Goals

- **No change to the PPR algorithm itself** (`personalized_pagerank` in `graph_ppr.rs`). The
  expander widens the input pool; PPR internals are unchanged.
- **No change to the fused scoring formula** (weights, normalization, co-access boost). The
  expanded pool feeds into the same scoring pass.
- **No new edge types or new graph edges**. Edge writing is crt-040/crt-041's scope. This
  feature only reads `TypedRelationGraph`.
- **No SQL neighbor query at query time**. `graph_expand` operates on the pre-built
  in-memory `TypedRelationGraph` (same as PPR today). No new store reads per query beyond
  the existing `entry_store.get()` calls for injected entries.
- **No schema migration**. `GRAPH_EDGES` schema is unchanged. `InferenceConfig` TOML fields
  use `#[serde(default)]` — backward-compatible, no migration needed.
- **No expander in the background tick**. The expander operates only on the hot search path.
- **No change to `TypedGraphState::rebuild()`**. The graph build/cache infrastructure is
  unchanged.
- **No enabling the expander by default in this feature**. The flag ships as `false`. Enabling
  it by default is a separate decision once the eval gate is passed.
- **No goal-conditioned or behavioral signal integration** (Groups 5/6 scope).

---

## Background Research

### Current retrieval pipeline (Step 6d, `search.rs`)

```
Step 5: HNSW(k=20) → search_results: Vec<SearchResult>
Step 6: Fetch entries, quarantine filter → results_with_scores: Vec<(EntryRecord, f64)>
Step 6a: Status filter / penalty marking
Step 6b: Supersession injection
col-031: Phase snapshot extraction (pre-loop)
Step 6d: PPR expansion (crt-030)
  Phase 1: Build personalization vector from HNSW seeds × phase affinity
  Phase 2: personalized_pagerank(typed_graph, seed_scores, alpha, iterations)
  Phase 3: Blend PPR scores into existing HNSW candidates (ppr_blend_weight)
  Phase 4: Identify PPR-only candidates (score > ppr_inclusion_threshold)
  Phase 5: Fetch and inject PPR-only entries (entry_store.get, quarantine check)
Step 6c: Co-access boost map prefetch
Step 7: NLI scoring (if enabled) → fused score → sort → truncate to k
```

Key observation: `personalized_pagerank` receives `seed_scores` built from
`results_with_scores` — exclusively the HNSW results. PPR can only score nodes
already in `typed_graph.node_index`. If a cross-category ground truth entry has no
HNSW candidate in the seed set, it receives zero personalization mass and a zero
(or near-zero) PPR score, regardless of how many edges connect it to seeds.

The current Phase 4 "PPR-only candidates" injection does reach entries outside HNSW,
but only those reachable by PPR mass diffusion. With small seed sets (20 entries) and
alpha=0.85, mass diffuses to ~2-hop neighbors — but mass arriving at cross-category
entries is tiny (diluted through many intermediate hops), so their PPR score rarely
exceeds `ppr_inclusion_threshold=0.05`. This is the documented zero-delta mechanism.

### ASS-038 / ASS-039 confirmed diagnosis

- 6,738 graph edges, zero MRR delta from PPR (ASS-038).
- Behavioral ground truth re-run confirms zero delta (ASS-039).
- Root cause: cross-category ground truth entries are outside k=20 by construction.
- PPR as re-ranker cannot reach them; PPR as expander can.

### TypedRelationGraph — in-memory traversal

`TypedRelationGraph` (`unimatrix-engine/src/graph.rs`) is a `petgraph::StableGraph<u64, RelationEdge>` backed by `node_index: HashMap<u64, NodeIndex>`. It is pre-built by the background tick and exposed via `TypedGraphStateHandle` (Arc<RwLock<TypedGraphState>>). The search path clones the graph under a short read lock before any traversal (lock already released before Step 6d).

`edges_of_type(node_idx, relation_type, direction)` is the sole traversal boundary (SR-01). All new traversal must use this method exclusively — no `.edges_directed()` or `.neighbors_directed()` calls.

Positive edge types for expansion: `CoAccess`, `Supports`, `Informs`, `Prerequisite`.
Excluded: `Supersedes` (structural chain, not retrieval relevance), `Contradicts` (suppression, negative signal).

### PPR function contract (ADR-002 crt-030)

`personalized_pagerank` is pure, synchronous, and deterministic. Signature unchanged. The
expander widens the set of node IDs that participate as seeds — the function itself is unmodified.

### Config infrastructure

`InferenceConfig` (`infra/config.rs`) is the established pattern for all runtime-tunable parameters. Fields use `#[serde(default = "fn_name")]` for backward compatibility. New fields are added to the `struct` body, `impl Default`, and the default value functions section. Five PPR fields already exist (`ppr_alpha`, `ppr_iterations`, `ppr_inclusion_threshold`, `ppr_blend_weight`, `ppr_max_expand`). Three new fields follow the same pattern.

All PPR config fields are wired from `InferenceConfig` → `SearchService::new()` → stored as `SearchService` fields → used in Step 6d. The expander fields follow the same wiring path.

### Feature flag pattern

No dedicated feature flag infrastructure exists in the codebase. Existing on/off gates use
boolean `InferenceConfig` fields (e.g., `nli_enabled: bool`). The expander flag follows
this pattern: `ppr_expander_enabled: bool` with `default = false`.

### Eval harness

`product/research/ass-039/harness/run_eval.py` orchestrates:
1. `unimatrix snapshot` — WAL-isolated DB copy
2. `unimatrix eval run --configs <profile.toml>` — replay scenarios through Rust eval engine
3. Aggregation — mean MRR + P@k from per-scenario JSON

Profile TOMLs in `product/research/ass-037/harness/profiles/` supply `[inference]` overrides.
A new profile `ppr-expander-enabled.toml` with `ppr_expander_enabled = true` provides Profile A.
Profile B (expander disabled) is the existing `conf-boost-c.toml` baseline.

Baseline: MRR = 0.2856 (live DB, 2026-04-02, conf-boost-c). P@5 = 0.1115.
The eval gate for crt-042: MRR >= 0.2856 AND P@5 > 0.1115 (first time P@5 should respond).

### write_graph_edge / quarantine patterns (from crt-040, crt-030)

PPR-injected entries already pass a quarantine check in Phase 5 (search.rs line ~960):
`SecurityGateway::is_quarantined(&entry.status)`. Expanded entries must use the same check.

### Latency concern

The roadmap flags latency as a risk requiring measurement before default enablement. BFS/DFS
over the in-memory graph with depth=2 and max=200 is bounded. The graph has 6,738+ edges
across ~7,000+ nodes (post crt-041). A depth-2 BFS from 20 seeds visits at most
`20 × avg_degree + 20 × avg_degree²` nodes. With avg_degree ~2-3, that is roughly 100-200
nodes — bounded well within `max_expansion_candidates=200`. Graph traversal is CPU-bound,
pure in-memory, synchronous. No rayon dispatch needed; runs inline in the async search handler
(same as PPR today, which also runs synchronously per ADR-002 crt-030).

The latency addition is bounded by: BFS traversal O(V+E) on the expanded subgraph + up to
`max_expansion_candidates` sequential `entry_store.get()` async calls. The latter is the
dominant cost and is already paid by the existing PPR Phase 5 injection (same pattern).

---

## Proposed Approach

### 1. `graph_expand` function in `unimatrix-engine`

New pure function in a new submodule `graph_expand.rs` (mirroring the `graph_ppr.rs` /
`graph_suppression.rs` split pattern — ADR from crt-030). Declared via `#[path]` in
`graph.rs`, re-exported from there.

```
fn graph_expand(
    graph: &TypedRelationGraph,
    seed_ids: &[u64],
    depth: usize,
    max_candidates: usize,
) -> HashSet<u64>
```

- BFS from each seed_id, traversing positive edges (CoAccess, Supports, Informs, Prerequisite)
  in **Outgoing direction only** up to `depth` hops.
- Returns the set of reachable entry IDs **excluding** the seed IDs themselves.
- Stops when `max_candidates` is reached (early exit), processing frontier in sorted node-ID order.
- Pure, synchronous, deterministic (BFS in sorted node-ID order per ADR-004 crt-030).
- All traversal via `edges_of_type()` exclusively (SR-01 / AC-02 pattern).
- Returns empty set on empty seeds, zero-node graph, or depth=0.

Rationale for Outgoing-only: bidirectionality is solved at the write side. Symmetric edges
(CoAccess) are stored in both directions at write time (entry #3889 back-fill). PPR's
reverse-walk accumulation handles predecessor relationships. Traversal direction is Outgoing
throughout — consistent with existing PPR and suppression walk patterns.

### 2. `SearchService` integration (Step 6d, `search.rs`)

New Phase 0 inserted before the existing Phase 1 (personalization vector construction):

```
Step 6d: PPR expansion (crt-030, extended by crt-042)
  Phase 0 [NEW]: graph_expand — widen seed pool if ppr_expander_enabled
    seed_ids = HNSW result entry IDs
    expanded_ids = graph_expand(&typed_graph, &seed_ids, expansion_depth, max_expansion_candidates)
    for each expanded_id not already in results_with_scores:
      entry = entry_store.get(expanded_id)  [quarantine check — mandatory]
      cosine_sim = vector_store.get_embedding(expanded_id) → cosine_similarity(query_emb, entry_emb)
      results_with_scores.push((entry, cosine_sim))
  Phase 1: Build personalization vector from ALL results_with_scores (seeds + expanded)
  Phase 2: personalized_pagerank over full expanded pool
  Phase 3–5: unchanged (blend + inject for any PPR-reachable entries still outside pool)
```

The expanded entries receive their true cosine similarity (computed from stored embedding),
not an artificial score. This differs from the existing Phase 5 injection (which uses
`ppr_blend_weight * ppr_score` as initial_sim for PPR-only entries). Expanded entries
have real semantic similarity scores and participate fully in fused scoring.

If `vector_store.get_embedding()` returns None for an expanded entry, skip it (no embedding
→ no cosine similarity → cannot score properly).

### 3. `InferenceConfig` additions (3 new fields)

```toml
ppr_expander_enabled = false   # bool, default false
expansion_depth = 2            # usize, range [1, 10]
max_expansion_candidates = 200 # usize, range [1, 1000]
```

All three follow the existing `#[serde(default = "fn_name")]` pattern. Added to struct,
`impl Default`, and default value functions.

### 4. `SearchService` struct + `new()` wiring

Three new fields mirror the existing PPR field pattern:
- `ppr_expander_enabled: bool`
- `expansion_depth: usize`
- `max_expansion_candidates: usize`

Wired from `InferenceConfig` in `SearchService::new()` (same five-field pattern as existing PPR).

### 5. Eval profile

`product/research/ass-037/harness/profiles/ppr-expander-enabled.toml`:
```toml
[profile]
name = "ppr-expander-enabled"
description = "PPR expander enabled (crt-042). HNSW k=20 seeds → graph_expand depth=2 max=200 → expanded pool → PPR → fused scoring."
distribution_change = true   # P@5 may change

[inference]
ppr_expander_enabled = true
expansion_depth = 2
max_expansion_candidates = 200
```

`distribution_change = true` because expanding the candidate pool changes P@k distribution.

---

## Acceptance Criteria

- AC-01: When `ppr_expander_enabled = false` (default), search output is bit-for-bit identical
  to pre-crt-042 behavior for all existing test cases.
- AC-02: When `ppr_expander_enabled = true`, `graph_expand` is called with the HNSW seed entry
  IDs before the PPR personalization vector is built.
- AC-03: `graph_expand` returns entry IDs reachable within `expansion_depth` hops via positive
  edges (CoAccess, Supports, Informs, Prerequisite) in **Outgoing direction only**.
- AC-04: `graph_expand` excludes seed IDs from the returned set (seeds already in pool).
- AC-05: `graph_expand` returns at most `max_expansion_candidates` IDs (early exit).
- AC-06: `graph_expand` returns an empty set when called with empty seeds, an empty graph,
  or `depth = 0`.
- AC-07: Every entry added to `results_with_scores` via `graph_expand` must pass the quarantine
  check (`SecurityGateway::is_quarantined`). Quarantined entries are silently skipped.
- AC-08: Every expanded entry that passes the quarantine check receives a cosine similarity score
  computed from `cosine_similarity(query_embedding, stored_entry_embedding)`. Entries with no
  stored embedding are skipped.
- AC-09: The expanded entries are present in `results_with_scores` before Phase 1 (personalization
  vector construction) executes — they receive non-zero personalization mass if their cosine
  similarity is above zero.
- AC-10: `graph_expand` traversal uses `edges_of_type()` exclusively — no direct
  `.edges_directed()` or `.neighbors_directed()` calls (SR-01 / AC-02 pattern).
- AC-11: All traversal is BFS (or equivalent) with a visited-set to prevent revisiting nodes.
- AC-12: `ppr_expander_enabled`, `expansion_depth`, and `max_expansion_candidates` are added
  to `InferenceConfig` with `#[serde(default)]`. Omitting them from TOML is valid and loads
  defaults.
- AC-13: `InferenceConfig::validate()` enforces: `expansion_depth` in `[1, 10]` and
  `max_expansion_candidates` in `[1, 1000]` — always, regardless of `ppr_expander_enabled`.
  Catches invalid configs at server start before the flag is ever flipped.
- AC-14: Eval profile `ppr-expander-enabled.toml` is committed and `run_eval.py` executes
  successfully with `--profile ppr-expander-enabled.toml`.
- AC-15: Eval gate passes with expander enabled: MRR >= 0.2856 (no regression). P@5 is
  measured and compared to the 0.1115 baseline — any increase is evidence the expander works.
- AC-16: `graph_expand` has inline unit tests covering: empty seeds, zero-depth, single hop,
  two-hop, mixed edge types, max_candidates early exit, quarantine isolation (graph_expand
  itself is pure and does not check quarantine — that is the caller's responsibility in search.rs).
- AC-17: A regression test confirms that when `ppr_expander_enabled = true`, a cross-category
  entry connected by a graph edge to an HNSW seed appears in the final result set (was not
  reachable with the old re-ranker approach).

---

## Constraints

### Technical

1. **SR-01 boundary**: all `TypedRelationGraph` traversal must go through `edges_of_type()`.
   Direct `.edges_directed()` or `.neighbors_directed()` calls are prohibited at new sites
   (established in crt-030, documented in graph.rs module header).

2. **No per-query store reads for the graph**: `graph_expand` operates on the pre-built
   in-memory `TypedRelationGraph`. Zero new SQLite queries in the expansion traversal itself.
   Only the follow-up `entry_store.get()` calls (one per expanded entry, already the pattern
   in PPR Phase 5).

3. **Lock order**: the typed graph read lock is already acquired and released before Step 6d
   (per existing lock-ordering comment in search.rs ~line 671). `graph_expand` uses the cloned
   `typed_graph` — no lock held during traversal. This invariant must be preserved.

4. **Async boundary**: `graph_expand` must be synchronous and pure (same contract as
   `personalized_pagerank` per ADR-002 crt-030). The `entry_store.get()` calls for expanded
   entries are async and run in the existing async search handler context.

5. **500-line file limit**: `graph_expand.rs` is a new file following the
   `graph_ppr.rs` / `graph_suppression.rs` split pattern. Tests live in a separate
   `graph_expand_tests.rs` if inline tests push the file over the limit.

6. **`get_embedding` is O(N)**: `vector_store.get_embedding(id)` scans the in-memory HNSW
   index linearly (confirmed O(N) per ADR crt-029, entry #3658). With up to 200 expanded
   entries, this is 200 × O(N) calls. At current corpus size (~7,000 active entries), this
   is ~1.4M comparisons per search when fully expanded. This is the primary latency concern.
   The feature flag allows measurement before default enablement. Future optimization: batch
   embedding lookup or index-based retrieval.

7. **`ppr_max_expand` interaction**: the existing Phase 5 cap (`ppr_max_expand`, default 50)
   limits PPR-only injection. With the expander, Phase 5 still runs but may inject fewer
   additional entries (expanded entries reduce the set outside the pool). No conflict; the
   two mechanisms are complementary.

8. **Dependencies**: crt-041 (write_graph_edge, S1/S2/S8 edges) must be merged before crt-042
   ships for the expander to traverse a dense enough graph to produce P@5 improvement. The
   expander works on any graph density; the improvement magnitude depends on edge density.

---

## Design Decisions (Resolved)

**Q1 — Traversal direction: Outgoing only.**
Bidirectionality is solved at the write side, not the read side. Symmetric relations (CoAccess)
store both directions at write time (back-fill migration confirmed in entry #3889). Directed
relations (Informs) use Outgoing traversal; PPR reverse-walk accumulation handles predecessors
of seeds. The ADR pattern: solve bidirectionality at write, traverse Outgoing-only at read.

**S1/S2 write-side prerequisite check (delivery prerequisite):** S1 (tag co-occurrence)
and S2 (structural vocabulary) are symmetric by nature. If crt-041 writes only one direction
(source_id < target_id, single edge), `graph_expand` Outgoing traversal can only reach entries
a seed points TO — half the S1/S2 graph is invisible. The crt-042 delivery agent must verify
before implementation: does crt-041 write bidirectional Informs edges for S1/S2? If
single-direction, the correct fix is at the crt-041 write site (write both A→B and B→A), not
by changing traversal direction. S8 CoAccess should already be bidirectional (crt-035 pattern).

**Q2 — Initial sim for expanded entries: true cosine similarity.**
The constant floor (0.01) removes semantic signal entirely, making PPR mass the sole driver.
But with 20-entry seed set and alpha=0.85, PPR mass arriving at 200 expanded entries is already
diluted — all end up near-zero with no semantic discriminator. True cosine gives genuinely
relevant cross-category entries a way to compete. Before treating ~50ms as fixed cost: the
delivery agent must investigate whether the HNSW index supports O(1) entry_id → vector lookup.
If yes, the latency concern largely disappears.

**Q3 — BFS frontier order: sorted node-ID.**
Matches ADR-004 crt-030. Deterministic for testing. Budget-boundary bias toward older entries
is real but not worth optimizing before measuring whether the expander works. Sort by edge
weight after the feature proves its value.

**Q4 — Config validation: always validate (both fields), regardless of `ppr_expander_enabled`.**
Reversed from AC-13 in the draft scope. The NLI pattern (validate only when enabled) was the
source of subtle config bugs — do not repeat it. Pre-validating catches `expansion_depth = 0`
at server start instead of at the moment someone flips the flag in production. Cost is zero.

**Q5 — Latency budget: no hard ceiling pre-committed.**
Measure first, gate second. The feature flag is the enforcement mechanism. The delivery agent
must add wall-clock timing instrumentation to the Phase 0 path (a `debug!` trace with duration
in ms) so the A/B eval captures latency data alongside MRR. The ceiling becomes a
post-measurement decision, not a pre-commitment.

**Combined Phase 0 + Phase 5 ceiling:** Phase 0 injects up to `max_expansion_candidates` (200)
entries. Phase 5 (existing PPR-only injection) injects up to `ppr_max_expand` (50) additional
entries. Maximum combined expansion per search: 250 entries beyond HNSW k=20. This is by design
and must be documented in the implementation.

---

## Tracking

https://github.com/dug-21/unimatrix/issues/492
