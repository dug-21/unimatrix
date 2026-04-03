## ADR-002: Phase 0 Insertion Point in Step 6d — Before Phase 1, After Steps 6a–6b

### Context

Step 6d (PPR expansion, introduced in crt-030) is positioned in the search pipeline as:
Step 6b (supersession injection) → Step 6d (PPR) → Step 6c (co-access prefetch) → Step 7 (NLI).
This ordering was established in ADR-005 crt-030 (entry #3735) and is authoritative.

Within Step 6d, the existing phases are:
- Phase 1: Build personalization vector from `results_with_scores` (HNSW seeds × phase affinity)
- Phase 2: `personalized_pagerank(typed_graph, seed_scores, alpha, iterations)`
- Phase 3: Blend PPR scores into existing HNSW candidates
- Phase 4: Identify PPR-only candidates (score > ppr_inclusion_threshold)
- Phase 5: Fetch and inject PPR-only entries (entry_store.get + quarantine check)

crt-042 must insert expansion before Phase 1 so that expanded entries participate in the
personalization vector. If expansion ran after Phase 1, expanded entries would have zero
personalization mass — they would still receive PPR scores via mass diffusion from seeds, but
without direct personalization they produce the same near-zero cross-category scores that
Phase 4/5 produces today. That is the bug crt-042 fixes.

The insertion point has two sub-questions:
1. Must Phase 0 run after Step 6a (status filter) and Step 6b (supersession injection)?
2. Must Phase 0 run before Step 6c (co-access prefetch)?

**After 6a/6b**: Yes. Step 6a applies the status penalty filter to `results_with_scores`.
Step 6b injects supersession entries. Phase 0 should operate on the post-6a-6b seed set so
it expands from the best available seed pool (penalty-marked and supersession-enriched). Running
before 6a would expand from entries that may be subsequently penalized.

**Before 6c**: Yes, by the transitive rule of ADR-005 crt-030. Step 6c prefetches the co-access
boost map over all current `result_ids`. If Phase 0 runs before 6c, expanded entries appear in
`result_ids` and receive co-access boosts. If Phase 0 ran after 6c, expanded entries get
`coac_norm = 0.0` — defeating the goal of full signal treatment for expanded entries.

**ppr_max_expand interaction (combined ceiling)**: Phase 5 still runs after Phase 0 and Phase 1–4.
Phase 5 injects PPR-only entries NOT already in `results_with_scores`. Because Phase 0 has already
added its expanded entries to `results_with_scores`, Phase 5 finds a smaller remaining set to
inject from. The two mechanisms operate on disjoint sets. Combined ceiling:
- Phase 0: up to `max_expansion_candidates` (default 200) new entries
- Phase 5: up to `ppr_max_expand` (default 50) new entries
- Maximum combined pool after expansion: `hnsw_k + max_expansion_candidates + ppr_max_expand`
  (default: 20 + 200 + 50 = 270 entries before PPR scoring and final truncation to k)

This ceiling is documented in the implementation (inline comment at Phase 0 entry point) as
required by SR-04. An AC must verify the maximum post-expansion pool does not exceed 270.

### Decision

Phase 0 is inserted as the first block inside the existing `if !use_fallback` branch in
Step 6d, before any Phase 1 code. The exact position:

```
Step 6d (if !use_fallback):
  [Phase 0 — NEW, crt-042]: graph_expand if ppr_expander_enabled
  Phase 1: Build personalization vector (existing)
  Phase 2: personalized_pagerank (existing)
  Phase 3: Blend (existing)
  Phase 4: Identify PPR-only (existing)
  Phase 5: Fetch and inject PPR-only entries (existing)
```

Phase 0 runs within the same `if !use_fallback` guard as the existing phases. When
`use_fallback = true` (PPR disabled), Phase 0 never executes. When `ppr_expander_enabled = false`
(expander disabled), Phase 0 executes its guard check and returns immediately — no traversal,
no fetch, no latency addition. When both are enabled, Phase 0 executes fully.

### Consequences

- Expanded entries receive co-access boosts from Step 6c (full signal treatment).
- Expanded entries receive non-zero personalization mass in Phase 1 proportional to their
  cosine similarity × phase affinity.
- Combined ceiling (270 entries) is well-defined and documented.
- Phase 5 still runs and may inject additional PPR-reachable entries not reached by Phase 0
  (e.g., entries reachable by mass diffusion through many hops but not by BFS within depth 2).
- The `use_fallback` guard means Phase 0 inherits the same PPR-disable semantics as Phases 1–5.
- The two-flag system (`ppr_enabled` via `use_fallback`, `ppr_expander_enabled`) is orthogonal:
  Phase 0 only activates when BOTH PPR is enabled AND the expander flag is on.
