# Agent Report: crt-042-agent-0-scope-risk

## Output
- Produced: `product/features/crt-042/SCOPE-RISK-ASSESSMENT.md`
- Risks: 7 total — 2 High, 4 Med, 1 Med/Low
- Document length: 35 lines (limit: 100)

## Risk Summary

| Severity | Count |
|----------|-------|
| High | 2 (SR-01, SR-03) |
| Med | 4 (SR-02, SR-04, SR-05, SR-06) |
| Med/Low | 1 (SR-07) |

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-01 — O(N) embedding scan, latency deferral without a ceiling**: 200 × O(N) calls at 7k corpus = ~1.4M comparisons per search. The scope defers measurement to post-ship A/B eval, which is reasonable, but leaves no defined ceiling. The architect must wire latency instrumentation into Phase 0 and the delivery brief must name a P95 ceiling as the condition for enabling the flag by default. Without a ceiling, "measure first" becomes indefinite deferral.

2. **SR-03 — S1/S2 Informs edge directionality is a potential blocker, not a delivery check**: If crt-041 wrote S1/S2 edges single-direction (source_id < target_id), Outgoing-only traversal sees half the graph. The scope treats this as a "prerequisite check" — but the remediation (back-fill migration or write-site change in a shipped feature) has real scope implications. This should be a hard blocking gate before Phase 0 implementation begins.

3. **SR-06 — Graph traversal direction semantics ambiguity (historical pattern, entry #3754)**: crt-030 produced 4 post-merge spec corrections because conceptual "Incoming" language and `Direction::Outgoing` code were both correct but looked contradictory to reviewers. The spec writer must write all traversal ACs behaviorally ("A surfaces when B is a seed and A→B exists") and cite #3754 explicitly. This is a low-cost intervention that prevented a full gate-3b rejection in crt-030.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for lesson-learned failures/gate rejections — found entries #3579, #2758, #1203, #2577, #3935 (gate discipline patterns, none directly applicable to retrieval expansion)
- Queried: `/uni-knowledge-search` for risk patterns on hot path/latency — found entry #3675 (tick candidate bound pattern, adjacent but tick-path not search-path)
- Queried: `/uni-knowledge-search` for O(N) embedding lookup — found entry #3658 (ADR confirming O(N) with prior SR-02 mitigation strategy at tick level, directly applicable)
- Queried: `/uni-knowledge-search` for graph edge directionality — found entries #3750, #3754 (PPR direction semantics ADR and lesson-learned, directly applicable)
- Stored: nothing novel — SR-03 (prerequisite check misclassification) and SR-01 (latency deferral without ceiling) are feature-specific. The direction-semantics risk pattern is already stored as entry #3754. No cross-feature generalization beyond what is already captured.
