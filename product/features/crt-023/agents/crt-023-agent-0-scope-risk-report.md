# Agent Report: crt-023-agent-0-scope-risk

## Output
- SCOPE-RISK-ASSESSMENT.md written to: `product/features/crt-023/SCOPE-RISK-ASSESSMENT.md` (39 lines)

## Risk Summary

| Severity | Count |
|----------|-------|
| High | 2 (SR-02, SR-03) |
| Med | 5 (SR-01, SR-04, SR-05, SR-06, SR-07, SR-08) |
| Low | 2 (SR-05, SR-09 severity inversions noted in table) |

Total: 9 risks across technology (3), scope boundary (3), and integration (3) categories.

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-02 (High)** — Rayon pool saturation. NLI is the first multi-pair batched workload on the shared pool. Historical entries #735, #1628, #1688 all trace MCP instability to this class of problem. Pool sizing must be an explicit ADR decision before implementation.

2. **SR-03 (High)** — `Mutex<Session>` serialises NLI inference across all concurrent callers. At 20 pairs × 200ms worst-case, a single search call holds the mutex for ~4s. Architect must decide session-pool vs single-session as an ADR before spec is written.

3. **SR-09 (High impact if wrong)** — Embedding handoff contract to the fire-and-forget post-store NLI task. The scope relies on reusing the already-computed embedding but does not specify how it is threaded through. Ownership/lifetime in async Rust makes this easy to get silently wrong (recompute on miss, or use stale data).

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found entries #735, #1628, #1688 (spawn_blocking/pool saturation lessons); #141, #167 (glass-box validation, gate handling)
- Queried: `/uni-knowledge-search` for "outcome rework" — no novel patterns; found standard gate/rework convention (#142)
- Queried: `/uni-knowledge-search` for "risk pattern" (category: pattern) — found #1542 (background tick counter error semantics), #1544 (ADR-002 hold-on-error); confirmed circuit breaker pattern is already stored
- Queried: `/uni-knowledge-search` for "ONNX model download inference pool saturation" — found #67 (ADR-001 Mutex<Session>), #69 (hf-hub), #82 (lazy init); confirmed no NLI-specific patterns stored yet
- Stored: nothing novel to store — pool saturation risk is already captured in entries #735/#1628/#1688; NLI-specific patterns should be stored after architect/spec sessions produce ADR decisions
