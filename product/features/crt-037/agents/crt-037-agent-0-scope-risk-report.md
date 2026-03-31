# Agent Report: crt-037-agent-0-scope-risk

## Output
- Produced: `product/features/crt-037/SCOPE-RISK-ASSESSMENT.md`
- Line count: 36 (constraint: < 100)

## Risk Summary
- High: 2 (SR-01, SR-07)
- Medium: 4 (SR-02, SR-03, SR-05, SR-06, SR-08) — 5 if SR-08 counted separately
- Low: 1 (SR-04)
- Total risks: 8

## Top 3 for Architect/Spec Writer Attention

1. **SR-01** — NLI neutral score reliability as an Informs signal. The neutral band is noisy; correctness depends on all four guards firing together (neutral > 0.5, cosine, temporal, cross-feature). Architect must specify a typed composite guard struct — not parallel index-matched lists.

2. **SR-07** — PPR direction semantics. The `Direction::Outgoing` reverse-walk contract (entry #3744) is undocumented in the ADR and must be explicitly verified for the fourth `edges_of_type` call. A wrong direction produces zero mass flow from lessons to decisions with no error signal.

3. **SR-08** — Discriminator tag routing in the merged Phase 7 rayon batch. If Phase 4b metadata attachment and Phase 8b routing logic diverge, Informs pairs silently fall into the Supports write path (entailment threshold) and are dropped. Architect must specify the tag struct.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — entries #3579, #2758, #1203 found; no direct relevance to this feature's domain
- Queried: `/uni-knowledge-search` for "risk pattern" category:pattern — entries #1616 (dedup ordering), #3742 (scope divergence warn-pattern), #3525 (NaN propagation) found; #1616 applied to SR-01/SR-08 recommendation
- Queried: `/uni-knowledge-search` for "NLI graph inference tick neutral score" — entry #3937 found (NLI neutral-zone pattern, crt-037-tagged); confirmed scope aligns with existing pattern
- Queried: `/uni-knowledge-search` for "PPR Direction::Outgoing" — entries #3744, #3754 found; SR-07 references #3744 directly
- Stored: nothing novel to store — PPR direction risk already captured in entry #3744; NLI neutral-zone signal already in #3937
