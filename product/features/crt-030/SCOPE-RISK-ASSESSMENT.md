# Scope Risk Assessment: crt-030

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Power iteration at 20 iterations over all graph nodes is O(N×E) per search call. At 10K nodes/50K edges it is negligible, but the SCOPE.md threshold for offloading is 100K nodes — if the knowledge base grows past that before a RayonPool offload is implemented, search latency will silently degrade. | Med | Low | Architect should define the measurable latency budget for Step 6d and specify the offload trigger point explicitly, not leave it as a future optimization note. |
| SR-02 | Entry fetch for PPR-surfaced candidates uses sequential async store calls (AC-13). With `ppr_max_expand=50` and a store get per entry, this adds up to 50 round-trips inside the search hot path. At high QPS this serializes fetch latency. | Med | Med | Architect should specify whether Step 6d fetch is batched or sequential in the first implementation, and set an explicit upper bound on acceptable added latency. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | The SCOPE.md PPR pipeline order states Step 6d is "after Step 6b (supersession injection), BEFORE Step 6c (co-access prefetch)" (Goals section and Proposed Approach). The Background Research section says "after co-access boost prefetch (Step 6c), before NLI (Step 7)". These are contradictory. The Proposed Approach resolves it as 6d before 6c, but both phrasings exist. | High | High | Spec must unambiguously fix the step order: the correct ordering is 6b → 6d (PPR) → 6c (co-access) → 7 (NLI) per Goals item 2. The Background Research section wording is wrong and should be corrected. |
| SR-04 | `ppr_blend_weight` controls both the score adjustment for existing candidates AND the initial similarity assignment for PPR-only entries (AC-14: `ppr_blend_weight × ppr_score`). One parameter serving two distinct roles can produce unintuitive behavior — raising the blend weight to improve existing-candidate adjustment also raises the floor similarity for newly injected entries. | Med | Med | Spec writer should confirm this dual-role is intentional and document it explicitly. If the roles need independent tuning, a separate `ppr_inject_weight` parameter should be scoped now, not as a follow-up. |
| SR-05 | `ppr_max_expand` default of 50 is presented as a cap on injected entries, but there is no cap on the size of the graph traversal itself — PPR iterates over all reachable nodes. In a dense graph (e.g., heavily bootstrapped CoAccess edges), the score map returned by `personalized_pagerank` may contain thousands of candidates before the `ppr_max_expand` filter is applied. The inclusion threshold filters first, then max_expand caps — but if the threshold is low, the pre-cap set can be very large. | Med | Med | Architect should confirm memory allocation profile for the PPR score map and whether a graph-traversal-depth cap is needed (separate from max_expand). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `phase_affinity_score` has a doc-comment two-caller contract (ADR #3687): PPR expects 1.0 on cold-start; fused scoring guards on `use_fallback` before calling. A future caller that misreads the contract will silently get wrong scores. The PPR personalization vector build in Step 6d must call `phase_affinity_score` directly (no guard), relying on the method's 1.0 cold-start behavior. | High | Med | Spec writer must include explicit reference to ADR #3687 in the Step 6d personalization vector construction spec. Implementation must not add a `use_fallback` guard around the `phase_affinity_score` call in PPR — that would break the PPR cold-start contract. |
| SR-07 | PPR injects entries into `results_with_scores` that have no HNSW similarity score. The fused scorer, NLI step, and any downstream consumer that assumes `results_with_scores` entries all have real similarity scores must handle PPR-only entries whose similarity is `ppr_blend_weight × ppr_score` (a synthetic value). If the fused scorer or NLI step has an implicit assumption about similarity score range or provenance, PPR entries may score anomalously. | Med | Med | Spec writer should verify fused scorer and NLI step make no assumptions about similarity score origin, and add an AC requiring PPR-only entries pass through NLI scoring without special-casing. |
| SR-08 | `#414` (phase affinity frequency table) is a declared dependency with graceful cold-start fallback. If #414 is not merged before crt-030, the fallback path (×1.0) is always active and PPR personalization is purely HNSW-score-seeded. This degrades quality but does not break correctness. The risk is that crt-030 ships and the #414 integration is never validated in production because the fallback silently takes over. | Low | Med | Track #414 merge status as a pre-release gate for full PPR quality. Spec should include an AC that verifies #414 data is used when available (not just that fallback works). |

## Assumptions

- **Goals item 2 / Background Research step numbering conflict**: SCOPE.md Background Research section says PPR runs "after co-access boost prefetch (Step 6c)" but Goals item 2 and Proposed Approach both say PPR runs *before* Step 6c. Assumption: Goals and Proposed Approach are authoritative; Background Research description is stale. If wrong, co-access will not run over PPR-surfaced entries, defeating the stated rationale.
- **CoAccess edge density is bounded**: CoAccess edges are bootstrapped from `co_access` where count >= 3, which is a meaningful threshold. If production data has unbounded CoAccess density, PPR iteration cost grows with the assumption it is bounded. Needs confirmation from crt-029 edge-write data before launch.
- **Sequential fetch is acceptable for ppr_max_expand=50**: SCOPE.md accepts sequential store fetches for up to 50 PPR candidates. This assumption is valid only if store get latency remains sub-millisecond. If storage layer changes (e.g., remote storage, W2-1 container packaging), this becomes a latency cliff.

## Design Recommendations

- **SR-03 (Critical)**: Fix the step-order contradiction in the spec before architecture proceeds. Correct order: 6b → 6d (PPR) → 6c (co-access prefetch) → 7.
- **SR-06 (High)**: Spec writer must cite ADR #3687 when specifying the personalization vector build. No `use_fallback` guard around `phase_affinity_score` in PPR. (Referenced from Unimatrix entry #3687.)
- **SR-04 (Med)**: Explicitly document or eliminate the dual role of `ppr_blend_weight`. If roles diverge in practice, add a second config parameter now — config changes after shipping are costly.
- **SR-01 / SR-02**: Architect should include a latency budget table for Step 6d at three scale points (1K, 10K, 100K entries) and define the offload trigger condition precisely.
