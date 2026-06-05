# Agent Report: vnc-025-agent-3-risk (architecture-risk mode)

## Deliverable

`product/features/vnc-025/RISK-TEST-STRATEGY.md` — 15 risks (R-01..R-15), all SR-01..SR-09
traced, security + edge-case + failure-mode sections complete.

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 1 (R-01 merge/hole correctness) |
| High | 5 (R-02 offset arithmetic, R-03 overflow×reorder, R-04 delta→durable-row, R-05 content leak, R-09 PreCompact parity) |
| Medium | 5 (R-06 lock/poison, R-07 audit emission, R-08 drain/sweep+silent-eviction, R-10 cycle-review clear, R-13 prompt injection) |
| Low | 4 (R-11 config plumbing, R-12 HTTP convergence, R-14 hook.rs move, R-15 hole exhaustion) |

## Highlights for Human / Downstream Attention

1. **R-02/R-06 poisoned-mutex policy**: `offset` is attacker-controlled u64; an arithmetic
   panic inside `apply_delta` poisons the per-session mutex and bricks that session. No ADR
   pins the poison-handling policy — the tester will force this decision; recommend
   treat-as-empty + always-Ack.
2. **R-01 is the test-effort center of gravity**: ADR-002's hole bookkeeping needs a
   property/permutation harness; R-01/R-02/R-03/R-15 share it (tune the cap to reuse).
3. **R-09 ships-dark parity**: golden test (from_path vs streamed from_bytes) and
   empty-buffer byte-identity are both hard gates — drift is invisible until F3 otherwise.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_search (4 queries: dispatch/registry lessons, risk
  patterns, audit fire-and-forget, transcript merge) — surfaced the audit-write failure
  cluster (#2299, #2249, #2266, #4379), #3902 (sanitize_session_id dispatch interaction),
  #4140 (silent-eviction loss), #3426 (golden-output), #2984 (copied test expectations),
  #3253 (test-name inventory). All cited as evidence in the strategy.
- Stored: nothing novel to store — the recurring cross-feature patterns relevant here are
  already captured (#4379 audit-context, #3426 golden-output); the poisoned-mutex concern is
  single-feature so far and lives in the risk document.
