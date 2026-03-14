# Agent Report: crt-019-agent-0-scope-risk

## Output

Produced: `product/features/crt-019/SCOPE-RISK-ASSESSMENT.md` (39 lines)

## Risk Summary

| Severity | Count |
|----------|-------|
| High | 3 (SR-01, SR-03, SR-07) |
| Med | 5 (SR-02, SR-04, SR-05, SR-06, SR-08) |
| Low | 0 |

## Top 3 Risks for Architect/Spec Writer Attention

**SR-01 (High/High)** — Bayesian empirical prior requires sufficient voted-entry population to produce stable α₀/β₀; with ~192 live entries and historically sparse votes, the empirical path may never activate in practice. The spec must define a minimum population threshold and specify that the cold-start default is used below it. Without this, α₀/β₀ could be computed from 1-2 voted entries and produce noisy scores.

**SR-03 (High/High)** — `rerank_score()` references `SEARCH_SIMILARITY_WEIGHT` as a compiled constant in the engine crate. The adaptive blend requires making this value runtime-variable. Converting to a parameter keeps the crate stateless; introducing shared mutable state conflicts with the engine crate's design. Multiple existing tests assert the constant is exactly 0.85 — all must change. The architect needs to commit to one approach before spec so the 6+ call sites in search.rs are updated consistently.

**SR-07 (High/Med)** — Implicit helpful vote for `context_get` must be folded into the existing `record_access` call, not added as a separate `spawn_blocking` task. Entry #735 documents spawn_blocking pool saturation from multiple per-call fire-and-forget tasks as a prior production incident. The safe implementation path is `UsageContext.helpful = Some(true)` — one task, not two.

## Knowledge Stewardship

- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" -- found #1006 (ADR-003 gate checks), #1044 (crt-018 risk strategy lesson), #735 (spawn_blocking saturation), #770 (mutex deadlock), #771 (tokio starvation from direct lock_conn)
- Queried: /uni-knowledge-search for "outcome rework confidence scoring" -- found #213 (crt-002 outcome), #255 (ADR-004 batched recomputation), #485 (ADR-005 penalty multipliers)
- Queried: /uni-knowledge-search for "risk pattern" (category: pattern) -- no directly applicable risk patterns found; existing patterns are agent tiering and dispatch
- Stored: entry #1480 "Parameter-passing over shared state when promoting engine constants to runtime values" via /uni-store-pattern -- cross-feature pattern visible in crt-019 (SEARCH_SIMILARITY_WEIGHT -> adaptive blend), generalizes to any constant-to-dynamic promotion in the engine crate
