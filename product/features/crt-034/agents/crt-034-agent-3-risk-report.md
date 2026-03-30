# Agent Report: crt-034-agent-3-risk

## Deliverable

`/workspaces/unimatrix/product/features/crt-034/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 1 |
| High     | 7 |
| Med      | 4 |
| Low      | 2 |

Total: 13 risks identified across R-01 through R-13, plus 4 integration risks (I-01 through I-04) and 6 edge cases (E-01 through E-06).

## Top Risks Requiring Test Coverage

1. **R-01 (Critical)** — Silent absorption of write failures: infallible contract absorbs all errors; the warn! + info! log path is the only defense. Three mandatory test scenarios covering mid-batch failure, continuation, and always-emitting summary log.

2. **R-11 (High)** — ORDER BY count DESC omitted from batch query: without ordering, arbitrary pairs are promoted under cap, bypassing the high-signal-first requirement. AC-04 test is mandatory and must verify *which* pairs were selected, not merely that N edges exist.

3. **R-04 (High)** — INSERT OR IGNORE no-op detection via rows_affected: the two-step write logic branches on rows_affected==0; incorrect value means weight refresh never fires. Three scenarios covering no-duplicate, delta>threshold UPDATE, and delta<=threshold no-op.

4. **R-05 (High)** — Tick ordering violation: promotion after TypedGraphState::rebuild() silently defers co-access signal by one cycle. Primary verification is code review of background.rs call site (AC-05).

5. **R-07 (High)** — Config merge_configs() omission: project-level cap override silently ignored if the merge stanza is missing. Four sub-tests covering override semantics, default fallthrough, and validation boundaries.

6. **R-02 (High)** — max_count=0 division guard: empty table or all-sub-threshold data must return early before the per-pair loop, not propagate a None/0 into weight computation. AC-09 tests are mandatory.

7. **R-13 (High)** — Inserted edge metadata fields: downstream GC (#409) and audit rely on source='co_access', created_by='tick', bootstrap_only=0, relation_type='CoAccess'. AC-12 must check all four fields.

## Coverage Gaps to Flag

- **AC-13 "outside-batch max" scenario**: The spec requires a test where the highest-count pair is not in the capped batch, confirming the scalar subquery computes global max. This scenario is distinct from the AC-13 test as written and should be added explicitly.

- **E-05 (delta boundary)**: Weight delta exactly at 0.1 is NOT updated (strictly greater than). This off-by-one direction must be a named test case; it is easy to implement as `>=` instead of `>`.

- **E-02 (tied counts)**: Tie-breaking under cap with identical counts is unspecified. The spec is silent on secondary sort order. The tester should clarify with the architect whether `ORDER BY count DESC, entry_id_a ASC` is required for deterministic behavior.

- **FM-02 (batch fetch failure)**: The spec covers per-pair write failure (AC-11) but does not have an explicit AC for the case where the initial SELECT batch query itself fails. This failure mode should be tested.

## Self-Check

- [x] Every risk has a Risk ID (R-01 through R-13)
- [x] Every risk has at least one test scenario
- [x] Severity and likelihood assessed for each risk
- [x] Integration risks section present and non-empty (I-01 through I-04)
- [x] Edge cases section present and non-empty (E-01 through E-06)
- [x] Failure modes section present (FM-01 through FM-04)
- [x] RISK-TEST-STRATEGY.md written to feature root (not in test-plan/)
- [x] No placeholder risks — each risk is specific to this feature's architecture
- [x] Security risks section present — no untrusted external inputs; blast radius assessed
- [x] Scope Risk Traceability table present — all 6 SR-XX risks have rows
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection background tick" — found #3579, #3580, #3723. Applied to R-01 (Critical), R-12 (file size).
- Queried: `/uni-knowledge-search` for "risk pattern recurring tick GRAPH_EDGES promotion" — found #3821, #3822, #1616. Applied to R-09, R-05, R-04.
- Queried: `/uni-knowledge-search` for "SQLite migration co_access weight normalization" — found #2428. Applied to R-02, R-03.
- Queried: `/uni-knowledge-search` for "outcome rework infallible tick write error absorbed silently" — found #3723, #1366, #1542. Applied to R-01 escalation to Critical.
- Stored: nothing novel to store — existing patterns #3821 and #3822 already cover the domain. R-06 (tick-counter-reset false-positive SR-05 warn) is feature-specific, not yet a cross-feature pattern.
