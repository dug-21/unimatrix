# Agent Report: crt-022-agent-3-risk

**Agent**: Risk Strategist (Architecture-Risk mode)
**Feature**: crt-022 — Rayon Thread Pool + Embedding Migration (W1-2)
**Artifact produced**: `product/features/crt-022/RISK-TEST-STRATEGY.md`

## Summary

11 risks identified. 2 Critical, 4 High, 4 Med, 2 Low.

| Priority | Count |
|----------|-------|
| Critical | 2 |
| High | 4 |
| Med | 4 (+ 2 security) |
| Low | 2 |

## Top Risks for Human Attention

**R-02 (Critical)** — `spawn_with_timeout` cancels the async wait but leaves the rayon thread occupied. Under repeated timeouts (e.g., adversarial or pathological inputs), the pool drains silently. The pool remains at configured thread count but effective capacity drops. No operator-visible signal until all threads are hung. Mitigation is pool sizing (ADR-003, floor 4) plus the `spawn_with_timeout` semantic documentation in ADR-002.

**R-04 (Critical)** — A call site migrated to `spawn` instead of `spawn_with_timeout` on an MCP handler path silently removes timeout coverage. Entry #1688 documents exactly this failure mode compounding across call sites. The CI grep step (C-09) catches `spawn_blocking` survivors but does not enforce which `RayonPool` method was used. Convention must be enforced by code review and documented in module rustdoc.

**R-03 (High)** — Mutex poisoning: a panic inside `session.run()` poisons `Mutex<Session>` in `OnnxProvider`. The bridge correctly returns `Cancelled`, but subsequent callers also receive `Cancelled` forever until the `EmbedServiceHandle` retry state machine fires. The architecture's recovery path (embed service retry) does not receive a signal from `RayonError::Cancelled` at call sites — there is a gap between bridge-level errors and the state machine trigger.

**R-08 (High)** — Pool size 4 (default floor) under simultaneous contradiction scan + quality-gate loop + 2 MCP calls exhausts the pool. A third concurrent MCP embedding call queues. On deployments with large knowledge bases (>1000 entries, scan takes minutes), this queuing is visible as latency. Operators must be guided to increase `rayon_pool_size` via config.

## Scope Risk Traceability

All 7 SR-XX risks traced. SR-01, SR-03, SR-04, SR-06 fully resolved by architecture. SR-02, SR-05, SR-07 resolved with test coverage requirements. SR-05's macro-expansion gap documented as a residual in Security Risks.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection rayon tokio thread pool" — found #1688, #735. Elevated R-04 to Critical.
- Queried: `/uni-knowledge-search` for "outcome rework spawn_blocking migration" — found #1700, #1627. Informed R-03 and R-08.
- Queried: `/uni-knowledge-search` for "risk pattern rayon pool panic timeout bridge" — found #2491, #2535, #2537 (all crt-022-tagged, already stored by prior agents).
- Stored: nothing novel to store — all relevant patterns already captured by researcher and architect agents in this feature's design phase.
