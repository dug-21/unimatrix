# Agent Report: crt-053-agent-0-scope-risk

**Mode:** scope-risk
**Produced:** product/features/crt-053/SCOPE-RISK-ASSESSMENT.md

## Risk Summary
6 risks identified.
- High severity: SR-01, SR-02, SR-03
- Med severity: SR-04, SR-05, SR-06

## Top 3 for Architect/Spec Attention
1. **SR-01 (High/High)** — Platform cannot measure search-heuristic effectiveness (eval harness has no forbidden-absent / rank-below gate; Unimatrix #4888). Do NOT scope an eval gate; behavior assertions in the integration suite are the only validation lever.
2. **SR-03 (High/High)** — Scope-creep into adjacent locked "gaps." Direct precedent: vnc-018 inverted write-only PPR negative tests without an ADR (Unimatrix #4495). Encode SCOPE.md's five Locked Decisions as hard architecture constraints.
3. **SR-02 (High/Med)** — Filter must keep 6b terminal-active heads (`:814–821`, already Active) while excluding superseded seeds (`:770`). Predicate = `Status::Active` on the seed set only.

## Knowledge Stewardship
- Queried: context_search for lesson-learned (gate rejection / PPR / graph_expand / deprecated) and pattern (search relevance / confidence). Findings: #4536 (status-guard invisible without supersession false-positive test), #4538 (semantically-dead params on active-only modes), #4495 (write-only negative tests are gates — vnc-018 precedent), #4888 (eval harness measures positive relevance only — explicitly names crt-053), #4077 (BFS-vs-PPR direction-semantics traps). All directly informed SR-01/02/03/05/06.
- Stored: nothing novel to store — the recurring patterns this feature exhibits are already captured (#4495, #4536, #4888); no new cross-feature pattern emerged from a single surgical filter.
