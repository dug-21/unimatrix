## ADR-003: True Cosine Similarity for Expanded Entries via `get_embedding` — O(N) Impact Documented

### Context

Expanded entries (those injected by Phase 0 via `graph_expand`) need an initial similarity
score to enter `results_with_scores`. This score is used as their contribution to the Phase 1
personalization vector and as their starting point in the fused scoring formula. Three options
were considered:

**Option A — Constant floor (e.g., 0.01)**: Simple, zero latency. Every expanded entry gets
the same initial score regardless of its actual semantic relationship to the query.

**Option B — PPR-derived initial score (`ppr_blend_weight * ppr_score`)**: Mirrors the existing
Phase 5 injection. Uses `ppr_blend_weight * ppr_score` as `initial_sim`. Requires running PPR
before Phase 0 — a circular dependency (Phase 0 must run before Phase 1 which feeds Phase 2/PPR).

**Option C — True cosine similarity from stored embedding**: Compute
`cosine_similarity(query_embedding, stored_entry_embedding)` where the entry embedding is
retrieved via `vector_store.get_embedding(entry_id)`. Returns the true semantic distance
between query and the expanded entry.

**Option A** (constant floor) removes all semantic signal. Expanded entries would compete for
PPR mass based solely on graph topology, with no semantic discriminator. With 20-seed sets and
alpha=0.85, PPR mass arriving at 200 expanded entries is already diluted — all end near-zero.
The constant floor ensures expanded entries that are semantically distant from the query
receive identical treatment to those semantically close. This negates the value of the
personalization vector for these entries.

**Option B** is structurally impossible: PPR has not run at Phase 0 time. Phase 0 must precede
Phase 1 (which builds the personalization vector that feeds Phase 2/PPR).

**Option C** gives genuinely relevant cross-category entries a way to compete. An entry with
strong graph connectivity to seeds AND semantic similarity to the query receives a high
personalization score in Phase 1 and accumulates PPR mass proportional to that score. An entry
with strong graph connectivity but low semantic similarity receives a low initial score — its
PPR mass is diluted. This discriminator is what makes the expander useful: cross-category
entries that happen to be semantically relevant are surfaced; graph-connected but semantically
irrelevant entries are not artificially amplified.

**The O(N) cost of Option C**:

`vector_store.get_embedding(entry_id)` is implemented as an IntoIterator scan over the HNSW
in-memory index layer by layer (confirmed O(N) at `unimatrix-vector/src/index.rs:312`, documented
in entry #3658). At corpus size ~7,000 active entries with 200 expanded entries:
200 × O(7000) = ~1.4M f32 comparisons per search when fully expanded at depth 2, max 200.

**O(1) investigation required by delivery agent**: The `VectorIndex.id_map.entry_to_data`
HashMap provides an O(1) mapping from `entry_id → data_id`. If the HNSW layer-0 stores
point vectors addressable by data_id (bypassing the full IntoIterator layer scan), the delivery
agent must implement and use that O(1) path instead. If not available without significant rework,
the O(N) path proceeds and the feature flag enforcement is the latency gate (see ADR-005).

**Skip policy**: If `get_embedding` returns `None` for an expanded entry (no stored embedding
or an embedding lookup miss), the entry is silently skipped — not added to `results_with_scores`.
An entry without a retrievable embedding cannot receive a meaningful cosine score and would
inject noise. This matches the "no embedding → no score → skip" contract established for the
tick path (crt-014 bugfix, entry #1724).

### Decision

Expanded entries receive true cosine similarity computed from `vector_store.get_embedding(id)`.
The delivery agent must investigate whether an O(1) entry_id → embedding path exists in
`VectorIndex` before implementing the O(N) fallback. The result of that investigation must be
documented in the implementation PR (file a follow-up issue if O(1) is feasible but deferred).

If `get_embedding` returns `None`, the entry is skipped (not added to `results_with_scores`).

This differs from Phase 5's existing injection strategy (`initial_sim = ppr_blend_weight * ppr_score`).
Phase 5 injects entries that PPR has already scored highly; Phase 0 injects entries before PPR
runs. The two phases serve different purposes and appropriately use different initial score sources.

### Consequences

- Expanded entries with genuine semantic relevance receive meaningful personalization mass.
- Expanded entries with zero semantic relevance (none → cosine near 0.0) receive minimal mass.
- O(N) per expanded entry is the primary latency risk when `ppr_expander_enabled = true` (SR-01).
- The feature flag defaults to `false`; O(N) cost is not incurred on the default path.
- Before enabling by default, latency must be measured (ADR-005) and an O(1) path investigated.
- Future improvement: if O(1) embedding lookup is implemented, the latency concern is resolved
  and the flag may be enabled by default sooner.

Related: entry #3658 (O(N) get_embedding ADR, crt-029). ADR-005 (timing instrumentation).
