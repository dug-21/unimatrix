# crt-042: PPR Expander — Architecture

## System Overview

The current search pipeline treats HNSW k=20 results as the complete candidate pool before PPR
scoring. PPR can only reach entries already in that pool; cross-category entries reachable
by graph topology but distant in embedding space are invisible. ASS-038 confirmed this with
6,738 graph edges producing zero MRR delta — the bottleneck is architectural, not graph density.

crt-042 adds a **graph expansion phase** (Phase 0) that runs before the existing PPR
personalization vector construction. Phase 0 uses BFS over the in-memory `TypedRelationGraph`
to collect entry IDs reachable from HNSW seeds, fetches and scores those expanded entries, and
merges them into `results_with_scores` so all candidates — seeds and expanded alike — receive
PPR scores. The PPR algorithm itself is unchanged; its input pool widens.

This feature directly unlocks the retrieval improvements crt-041's graph edges (S1/S2/S8) were
designed to produce. It is gated behind `ppr_expander_enabled = false` so the improvement can
be measured in A/B eval before default enablement.

---

## Component Breakdown

### Component 1: `graph_expand` (new `unimatrix-engine/src/graph_expand.rs`)

**Responsibility**: Pure BFS traversal of `TypedRelationGraph`. Given a set of seed entry IDs,
returns the set of entry IDs reachable within `depth` hops via positive edge types, excluding
the seeds themselves, capped at `max_candidates`.

**Key properties**:
- Pure, synchronous, deterministic (BFS in sorted node-ID frontier order per ADR-004 crt-030)
- No I/O, no locking — operates on a cloned `TypedRelationGraph` passed by reference
- Returns `HashSet<u64>` — membership test O(1) for subsequent quarantine filtering
- Positive edge types: `CoAccess`, `Supports`, `Informs`, `Prerequisite`
- Excluded edge types: `Supersedes` (structural chain), `Contradicts` (suppression)
- All traversal via `edges_of_type()` exclusively — no direct `.edges_directed()` calls (SR-01)
- Early exit when `max_candidates` reached (processes frontier in sorted node-ID order)
- Returns empty set when: seeds empty, graph empty, depth = 0

**Behavioral contract** (authoritative — see ADR-006 for direction rationale; cite entry #3754):

From seed B, edges `B → X` (any positive type) surface X for consideration.
Given seeds `{B}` and edge `B → C` (B Informs C), graph_expand surfaces `C` in the result.
Given seeds `{B}` and edge `A → B` (A Informs B), graph_expand does **NOT** surface `A` —
this is an incoming edge to the seed; Outgoing-only traversal does not follow it.
Entries pointing TO seeds are not surfaced by graph_expand — they remain available to PPR's
reverse walk in Phase 2.

**Module placement**: `#[path = "graph_expand.rs"] mod graph_expand;` inside `graph.rs`,
re-exported as `pub use graph_expand::graph_expand;`. Mirrors the `graph_ppr.rs` /
`graph_suppression.rs` split established in crt-030 (ADR-001 crt-030, entry #3731).

---

### Component 2: Phase 0 insertion in `search.rs` (Step 6d extension)

**Responsibility**: Async orchestration layer that calls `graph_expand`, fetches and scores
expanded entries, and merges them into `results_with_scores` before Phase 1 begins.

**Gate**: executes only when `self.ppr_expander_enabled = true`. When false, Step 6d behavior
is bit-for-bit identical to pre-crt-042 (AC-01).

**Phase 0 execution sequence**:
1. Collect seed IDs from `results_with_scores` (existing HNSW results after Steps 6a–6b)
2. Call `graph_expand(&typed_graph, &seed_ids, expansion_depth, max_expansion_candidates)`
3. Subtract IDs already in `results_with_scores` (seeds exclude themselves per AC-04)
4. For each expanded ID (in sorted order for determinism):
   a. `entry_store.get(expanded_id)` — async fetch, skip on error
   b. `SecurityGateway::is_quarantined(&entry.status)` — skip quarantined entries (AC-07)
   c. `self.vector_store.get_embedding(expanded_id)` — O(N) HNSW scan; skip if None (AC-08)
   d. `cosine_similarity(query_embedding, entry_embedding)` — true cosine (not a floor)
   e. `results_with_scores.push((entry, cosine_sim))`
5. Record Phase 0 wall-clock duration via `debug!` tracing (timing instrumentation — SR-01)

**Async boundary**: `entry_store.get()` and `vector_store.get_embedding()` are async, matching
the existing Phase 5 pattern. `graph_expand` itself is synchronous. No `spawn_blocking` needed
— pure CPU traversal is bounded (depth 2, max 200 nodes).

**Lock order invariant**: the `typed_graph` clone is acquired under a short read lock before
Step 6d begins (existing behavior). Phase 0 uses the pre-cloned value. No lock is held during
BFS traversal or the subsequent async fetch loop. This preserves the existing lock-ordering
comment in search.rs (~line 671).

---

### Component 3: `InferenceConfig` additions (`infra/config.rs`)

**Responsibility**: Operator control surface for the expander. Three new fields follow the
existing PPR field pattern exactly (`#[serde(default = "fn_name")]`).

| Field | Type | Default | Valid Range | Comment |
|-------|------|---------|-------------|---------|
| `ppr_expander_enabled` | `bool` | `false` | n/a | Feature flag. Default false until eval gate passes. |
| `expansion_depth` | `usize` | `2` | `[1, 10]` | BFS hop depth from seeds. |
| `max_expansion_candidates` | `usize` | `200` | `[1, 1000]` | BFS candidate cap. |

Validation (in `InferenceConfig::validate()`) runs **unconditionally** — regardless of
`ppr_expander_enabled`. This catches misconfigured values at server start, not at flag-flip
time in production (ADR-004). The pattern mirrors the unconditional `ppr_max_expand` validation.

Three new `SearchService` fields mirror the config: `ppr_expander_enabled: bool`,
`expansion_depth: usize`, `max_expansion_candidates: usize`. Wired in `SearchService::new()`
following the five-field PPR wiring pattern.

---

### Component 4: Eval profile (`ppr-expander-enabled.toml`)

**Responsibility**: Profile A in the crt-042 A/B eval gate measurement. Enables the expander
at default depth/cap against the live DB snapshot. Profile B is the existing `conf-boost-c.toml`.

**Location**: `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml`

**Eval gate**: MRR >= 0.2856 (no regression vs. baseline 0.2856) AND P@5 > 0.1115. Any
increase in P@5 is the signal that cross-category entries are now reachable.

---

## Component Interactions

```
search.rs (Step 6d)
├── [if ppr_expander_enabled]
│   Phase 0:
│   ├── graph_expand(typed_graph, seed_ids, depth, max) → HashSet<u64>
│   │   └── edges_of_type() × per hop × per positive edge type [graph.rs]
│   ├── entry_store.get(id) × per expanded id [async, Store]
│   ├── SecurityGateway::is_quarantined() × per entry [gateway.rs]
│   └── vector_store.get_embedding(id) × per entry [VectorIndex — O(N) HNSW scan]
│       └── cosine_similarity(query_emb, entry_emb)
│
├── Phase 1: seed_scores from ALL results_with_scores (seeds + expanded)
├── Phase 2: personalized_pagerank(typed_graph, seed_scores, alpha, iterations) [graph_ppr.rs]
├── Phase 3: blend PPR scores for all HNSW candidates
├── Phase 4: PPR-only candidates (entry IDs in ppr_scores NOT in results_with_scores)
└── Phase 5: fetch + inject up to ppr_max_expand PPR-only entries
```

**Data flow**:
- `results_with_scores: Vec<(EntryRecord, f64)>` accumulates entries through Phases 0–5
- After Phase 0: up to `hnsw_k + max_expansion_candidates` entries (default: 20 + 200 = 220)
- After Phase 5: up to `220 + ppr_max_expand` entries (default: 220 + 50 = 270 maximum)
- The 270-entry maximum is the documented combined ceiling (SR-04)

**PPR algorithm receives a wider personalization vector**: expanded entries with non-zero cosine
similarity receive personalization mass proportional to their cosine score × phase affinity.
This is the core unlock — previously, cross-category entries received zero personalization mass.

---

## Technology Decisions

See individual ADR files for full rationale:

- `ADR-001`: `graph_expand.rs` as `#[path]` submodule of `graph.rs` (not inline in search.rs)
- `ADR-002`: Phase 0 insertion point — before Phase 1, after Steps 6a–6b
- `ADR-003`: True cosine similarity for expanded entries (not a floor constant), O(N) impact documented
- `ADR-004`: Config validation unconditional — both fields always validated at server start
- `ADR-005`: `debug!` timing instrumentation in Phase 0; no metrics infrastructure

---

## Integration Points

### Existing components consumed (read-only)

| Component | How consumed | Notes |
|-----------|-------------|-------|
| `TypedRelationGraph` | Cloned before Step 6d; passed by ref to `graph_expand` | No new lock acquisition |
| `edges_of_type()` | Sole traversal boundary inside `graph_expand` | SR-01 invariant |
| `entry_store.get()` | Async, one call per expanded entry | Same pattern as Phase 5 |
| `SecurityGateway::is_quarantined()` | Per expanded entry after fetch | Same as Phase 5 quarantine check |
| `vector_store.get_embedding()` | O(N) HNSW scan per expanded entry | Primary latency driver |
| `InferenceConfig` | Three new fields added | Backward-compatible via `#[serde(default)]` |
| `SearchService::new()` | Three new fields wired in | Five-field PPR wiring pattern |

### S1/S2/S8 edge source verification (pre-implementation gate — SR-03)

crt-041 writes S1 and S2 edges with the single-direction pattern: `source_id < target_id`
(confirmed in graph_enrichment_tick.rs, line 92: `t2.entry_id > t1.entry_id`). S8 CoAccess
edges also use `a = min(ids), b = max(ids)` (line 330: `a = entry_ids[i].min(entry_ids[j])`).

**S8 CoAccess**: run_co_access_promotion_tick (crt-035) writes bidirectional edges. S8 writes
only one direction — but CoAccess edges from the promotion tick already cover both directions
for co-access pairs. S8's one-direction write is additive; the tick's bidirectional write
provides the reverse. This must be confirmed against the GRAPH_EDGES state at delivery time.

**S1/S2 Informs edges**: written single-direction only (source < target). With Outgoing-only
traversal from a seed, only entries the seed points TO are reachable — the reverse direction
(entries that point to the seed) is not reachable. For S1/S2, this means: if seed A and entry B
share tags, and A < B, then the edge is `A → B`. Outgoing from A reaches B. Outgoing from B
reaches nothing via this edge. Half the S1/S2 graph is invisible from any given seed in the
lower-ID position.

**Blocking gate**: the delivery agent must verify whether S1/S2 edge directionality is single-
or bi-directional in the deployed GRAPH_EDGES table. If single-direction, a back-fill migration
(same pattern as crt-035 CoAccess back-fill, entry #3889) must be filed as a prerequisite
before crt-042 ships. The fix site is the crt-041 write path (write both A→B and B→A), not
the traversal direction. Changing traversal to Bidirectional would break the existing reverse-
PPR semantics (entry #3750).

---

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|-----------------|--------|
| `graph_expand` | `fn graph_expand(graph: &TypedRelationGraph, seed_ids: &[u64], depth: usize, max_candidates: usize) -> HashSet<u64>` | New — `unimatrix-engine/src/graph_expand.rs` |
| `edges_of_type` | `fn(&self, node_idx: NodeIndex, relation_type: RelationType, direction: Direction) -> impl Iterator<Item = EdgeReference<'_, RelationEdge>>` | Existing — `graph.rs:203` |
| `personalized_pagerank` | `fn(graph: &TypedRelationGraph, seed_scores: &HashMap<u64, f64>, alpha: f64, iterations: usize) -> HashMap<u64, f64>` | Existing — `graph_ppr.rs` (unchanged) |
| `SecurityGateway::is_quarantined` | `fn(status: &Status) -> bool` | Existing — `gateway.rs` |
| `vector_store.get_embedding` | `fn(entry_id: u64) -> Option<Vec<f32>>` (async via AsyncVectorStore) | Existing — `VectorIndex::get_embedding` at `index.rs:312` |
| `InferenceConfig.ppr_expander_enabled` | `bool`, default `false` | New field — `infra/config.rs` |
| `InferenceConfig.expansion_depth` | `usize`, default `2`, range `[1, 10]` | New field — `infra/config.rs` |
| `InferenceConfig.max_expansion_candidates` | `usize`, default `200`, range `[1, 1000]` | New field — `infra/config.rs` |
| `SearchService.ppr_expander_enabled` | `bool` | New field — `search.rs` |
| `SearchService.expansion_depth` | `usize` | New field — `search.rs` |
| `SearchService.max_expansion_candidates` | `usize` | New field — `search.rs` |

---

## Combined Expansion Ceiling

Phase 0 (crt-042) and Phase 5 (existing PPR-only injection) are complementary, not conflicting.
Their combined ceiling per search request:

```
After HNSW:      k=20 entries
After Phase 0:   + max_expansion_candidates (default 200) → up to 220 entries
After Phase 5:   + ppr_max_expand (default 50)            → up to 270 entries
```

270 is the documented maximum pool size before PPR scoring and final truncation to k. This is
intentional design: Phase 0 expands via direct graph reachability; Phase 5 injects any
remaining PPR-scoring entries not yet in the pool. Phase 5 still runs regardless of whether
Phase 0 is enabled — the two mechanisms operate on disjoint sets when Phase 0 runs first
(Phase 5 uses `NOT in results_with_scores` which now includes Phase 0 entries).

---

## Latency Profile

The dominant latency cost is `vector_store.get_embedding()` — O(N) scan of the HNSW in-memory
index per expanded entry. At corpus size ~7,000 active entries with 200 expanded entries:
200 × O(7000) = ~1.4M f32 comparisons per search when fully expanded.

This cost is **not committed as acceptable before measurement**. The feature flag defaults to
`false`. Before `ppr_expander_enabled` can become the default, the following gate must be
satisfied (SR-01 remediation):

- Measure P95 added latency for Phase 0 with expander enabled across the eval scenario set.
- The ceiling for enabling by default is: **P95 Phase 0 latency addition ≤ 50ms over the
  pre-crt-042 baseline** — a delta, not an absolute. Measure the baseline (expander disabled)
  in the same eval run before enabling; the gate is the addition, not the total P95.
- Timing instrumentation via `debug!` with wall-clock ms is wired into Phase 0 for this purpose.

Future optimization path: batch embedding lookup or index-based O(1) retrieval by entry_id.
The delivery agent must investigate whether the HNSW id_map supports direct data_id → f32
slice lookup (bypassing the full layer scan). If yes, the latency concern is substantially
reduced and may not require the back-pressure gate.

---

## Open Questions

1. **S1/S2 back-fill scope**: Does the deployed GRAPH_EDGES table already have bidirectional
   S1/S2 Informs edges (from any earlier migration or tick change), or is it strictly
   single-direction? The delivery agent must verify this before writing Phase 0 code. If
   single-direction, a new issue must be filed (back-fill migration, same pattern as crt-035).

2. **S8 CoAccess directionality**: S8 writes `a < b` single-direction. The co-access promotion
   tick (crt-035) writes bidirectional. Does the S8 write path also need to write both
   directions, or does the crt-035 tick path cover all CoAccess pairs reliably?

3. **O(1) embedding lookup feasibility**: `VectorIndex.id_map.entry_to_data` maps entry_id →
   data_id (O(1)). The HNSW layer-0 stores point vectors accessible by data_id. If the
   delivery agent can retrieve `Vec<f32>` via data_id without the full IntoIterator scan
   (bypassing `get_embedding`'s layer traversal logic), Phase 0 latency drops from O(N) to
   O(1) per expanded entry. This investigation is a delivery prerequisite for SR-01 mitigation.

4. **Eval gate failure owner**: if the eval gate fails (MRR regression or P@5 flat) after
   S1/S2 directionality is confirmed bidirectional, who owns the investigation? The SCOPE.md
   eval gate is designed to measure, not to guarantee pass. The delivery brief should name an
   owner and decision path for this scenario (SR-05).
