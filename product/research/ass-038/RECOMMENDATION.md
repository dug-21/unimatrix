# ASS-038: Strategic Recommendations

**Status**: Complete  
**Based on**: ASS-037 FINDINGS.md, ASS-038 FINDINGS.md

---

## Central Finding

The "density was the bottleneck" hypothesis from ASS-037 is **disproved**. Six-fold edge density increase (1,086 → 6,738 edges, 37% → 81% coverage) produced zero PPR delta on 2,376 scenarios including 20 purpose-built cross-category scenarios.

The bottleneck is **architectural**: the current PPR implementation is a re-ranker within the k=20 HNSW candidate set. Cross-category ground-truth entries are semantically distant from their queries — they are outside k=20 by construction. PPR cannot surface them regardless of graph density.

---

## Recommendation 1 — Deliver edge generation sources S1 and S2 (High Priority)

**What**: Implement S1 (tag co-occurrence, ≥3 shared tags) and S2 (structural vocabulary overlap, ≥2 domain terms) as background tick signals that write `Informs` edges to the production graph.

**Why**: These sources are:
- SQL-only (no model dependency, no latency risk)
- High coverage: S1+S2 together cover 656/1134 entries (58%) with non-CoAccess edges at 1,134-entry corpus
- GNN training data: labeled with `signal_origin`, which W3-1 needs to learn edge type weights
- Density prerequisite: a future PPR expander architecture requires a dense graph to traverse — S1/S2 ensure that density exists when the PPR change lands

Note: delivering S1/S2 to production does not require PPR to be working or the architecture change to be complete. The edges are inert until a consuming system uses them.

## Recommendation 2 — Redesign PPR as an expander, not a re-ranker (Medium Priority)

**Current architecture**:
```
HNSW(query, k=20)  →  k=20 candidates  →  PPR re-ranks within k=20
```

**Required architecture**:
```
HNSW(query, k=20)  →  k=20 seeds  →  graph_expand(seeds, depth=2, max_candidates=200)
                   →  k=20 + graph-reachable pool  →  score + rank from expanded set
```

The change: PPR must be able to score and return entries that did NOT appear in the HNSW top-k. The graph traversal expands the candidate set before ranking. This makes cross-category bridging possible — the semantically distant entry becomes reachable via graph topology from a k=20 seed.

**Scale consideration**: `graph_expand` adds query-time graph traversal overhead. With 6,738 edges and max_candidates=200, this is bounded and fast (SQLite graph walk). This is a meaningful latency addition and should be gated on a feature flag.

## Recommendation 3 — Deliver S8 as a batch enrichment process (Low Priority)

**What**: Search co-retrieval edges (S8) — pairs of entries that co-appear in search results — generated from `audit_log` as a periodic batch job.

**Why**: S8 produced 2,770 edges covering 411 entries with 21.3% cross-category ratio. Unlike S1/S2, S8 is a behavioral signal (actual retrieval patterns) that may encode information not captured by tag or vocabulary overlap. Valuable for W3-1.

**Constraint**: S8 is a batch-only signal — new edges can only be generated from logged search history. It requires no real-time computation. A periodic tick reading from `audit_log` would generate S8 edges continuously as new searches accumulate.

## Recommendation 4 — Begin W3-1 scoping with the labeled graph as training data (Medium Priority)

The combined graph provides the feature vector inputs W3-1 needs:

| Feature type | Coverage | Notes |
|-------------|----------|-------|
| Edge signal_origin | 5 origins (co_access, nli, S1, S2, S8) | Required for GNN edge feature |
| Node: category | 100% (1134 entries) | 5 classes |
| Node: confidence | 100% | f64 Wilson-score composite |
| Node: access_count | 91.4% | Usage history proxy |
| Node: tag_count | 83.4% (945 entries) | From entry_tags |
| Node: degree_centrality | Computable | Derived from combined graph |

W3-1 design task: given this labeled edge set and node feature spec, design a GNN that predicts which signal origin (S1, S2, S8, CoAccess) produces Informs-quality edges for a given entry pair. This gives automatic signal weight learning — the GNN replaces the hand-configured `ppr_blend_weight`.

## What to Deprioritize

| Item | Reason |
|------|--------|
| S3 (session keywords) | 19 sessions, 47 entries — corpus too sparse for keyword overlap pairs |
| S4 (lexical citation) | 4–9 title-match pairs — insufficient yield at current corpus size |
| S5 (supersession chains) | 2 active→active links — most chains end in quarantined entries |
| S6/S7 (behavioral, session-correlated) | `audit_log` session_id is empty for most operations; not reconstructible |
| PPR density experiments | Density is no longer the hypothesis — architecture redesign is the required work |
| Increasing ppr_blend_weight | Does not help if entries are outside k=20; zero marginal value until expander is built |

---

## Evidence Summary

| Finding | Evidence |
|---------|---------|
| Density is not the PPR bottleneck | 6,738 edges (6.2x baseline) → zero delta (Phase 3) |
| Cross-category entries are outside k=20 | 6/10 UC ground-truth entries not in k=20 HNSW set |
| PPR cannot promote isolated k=20 entries | uc4-03: entry at rank 11 in k=20, zero edges to other k=20 members → no promotion |
| S1/S2/S8 generate viable labeled edges | 1,052 + 1,830 + 2,770 new edges; all labeled with signal_origin |
| GNN training data is ready | 6,738 labeled edges, 5 origins, node features 83-100% coverage |
| Tier-3 sources are infeasible at current scale | S3/S4/S5/S6/S7 each yield <20 viable pairs from active→active entries |
