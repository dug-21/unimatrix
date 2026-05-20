# Agent Report: vnc-019-agent-3-risk

## Output

- Produced: `product/features/vnc-019/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count | Risk IDs |
|----------|-------|----------|
| Critical | 3 | R-01, R-02, R-03 |
| High | 5 | R-04, R-05, R-06, R-07, R-11 |
| Med | 7 | R-08, R-09, R-10, R-12, R-13, R-14, R-15 |
| Low | 1 | R-16 |
| **Total** | **16** | |

## Top Risks Requiring Test Emphasis

**R-01 (Critical)** — visited-set / resolve_supersessions ordering. The BFS pseudocode in ARCHITECTURE.md is correct (resolve before enqueue, key visited on terminal ID) but the ordering is subtle enough to misimplement. Requires explicit multi-path deduplication test with two deprecated nodes resolving to the same terminal.

**R-02 (Critical)** — direction="both" edge deduplication and canonical direction. Lesson #4077 documents two prior features (crt-030, crt-042) where direction semantics bugs survived Gate 3a and Gate 3b. The dedup key must use the canonical stored direction (source→target), not the BFS iteration direction. Behavioral outcome tests (not Direction enum inspection) are the only reliable check.

**R-03 (Critical)** — seed count at or exceeding max_nodes cap. Seeds are added in the seed phase before BFS executes. A call with 201 seeds must produce exactly 200 nodes with truncated=true and depth_reached=0. The boundary condition (exactly 200 seeds) must also be tested — this is the case where BFS is entirely skipped.

**R-04 (High)** — empty-edges OR-chain SQL guard. A dynamically built OR-chain with zero clauses produces a syntax error or full-table scan. The empty-edge skip (AC-19) is a required guard; its absence produces no compile error and no test failure on happy-path tests — only a runtime SQL error on isolated-seed calls.

**R-05 (High)** — validate_no_unsupported_params regression. Adding the "subgraph" arm must not silently remove the rejection of seed_ids and max_depth on chain/current/neighbors. Six specific regression combinations must be tested (3 modes × 2 params), not inferred from passing subgraph tests.

## Self-Check

- [x] Every risk has a Risk ID (R-01 through R-16)
- [x] Every risk has at least one test scenario
- [x] Severity and likelihood assessed for each risk
- [x] Integration risks section present and non-empty (5 integration risks)
- [x] Edge cases section present and non-empty (11 edge cases)
- [x] Failure modes section describes expected behavior under each failure
- [x] RISK-TEST-STRATEGY.md written to feature root (not in test-plan/)
- [x] No placeholder risks — each risk is specific to vnc-019's designed architecture
- [x] Security Risks section present — untrusted inputs and blast radius assessed (5 security risks)
- [x] Scope Risk Traceability table present — all SR-01 through SR-07 have a row
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship

- Queried: `context_search` for lesson-learned/failures/gate/rejection/graph/BFS — found #4077 (direction semantics lesson, 2 prior incidents) and #4473 (warn+continue failure-path masking). Both directly informed risk elevation for R-02 and R-04.
- Queried: `context_search` for risk patterns in BFS/graph/staleness — found #4071 (BFS depth-limit guard pattern), #4486 (post-BFS metadata hydration pattern). Both confirmed architecture decisions are consistent with existing patterns.
- Queried: `context_search` for SQLite OR-chain batch query — confirmed #4486 and #4492 (ADR-003) are the canonical references.
- Stored: entry #4494 "BFS with node substitution: key visited set on resolved ID, not raw neighbor ID" via `context_store` — novel pattern not covered by existing entries; applicable to any future BFS with node rewriting.
