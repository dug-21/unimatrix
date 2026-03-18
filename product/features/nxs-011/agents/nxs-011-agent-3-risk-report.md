# Agent Report: nxs-011-agent-3-risk

**Agent**: nxs-011-agent-3-risk
**Mode**: architecture-risk
**Feature**: nxs-011 — sqlx Migration, Connection Pools + Async-Native Storage

## Output

- RISK-TEST-STRATEGY.md written to: `product/features/nxs-011/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 3 |
| High | 8 |
| Medium | 4 |
| Low | 1 |
| **Total** | **15** |

## Top 3 Risks for Tester Attention

**R-02 — Drain Task Teardown Race (Critical/High)**
~1,445 async test bodies each open and close a Store. Any test that omits `Store::close().await` leaves a live tokio task holding a write_pool connection. Entry #2057 confirms this pattern has caused silent test corruption in adjacent features. TC-02 (mandatory close in every test) is the primary mitigation — the tester must verify this is mechanically enforced across the entire converted test suite, not just spot-checked.

**R-08 — ExtractionRule block_on Bridge Panic (High/High)**
This is the only unresolved architectural question with a binary outcome: either the `ExtractionRule::evaluate()` full async conversion is done (safe, larger scope) or a block_on bridge is used (smaller scope, guaranteed panic on the active tokio runtime). The tester cannot write a meaningful integration test for the observe crate until the delivery team confirms which path was taken. This risk must be flagged as a delivery blocker until resolved.

**R-03 — Migration Failure Leaves Inconsistent State (Critical/Med)**
`migration.rs` (983 lines) is being translated from rusqlite to sqlx for the first time. The SQL logic is preserved verbatim but the execution path is new and untested against sqlx. Entry #378 confirms migration regressions are discovered only when old-schema databases are tested — not fresh ones. AC-17 (migration regression harness covering all 12 version transitions) is the formal gate, but the tester must ensure each transition test starts from the *previous* version schema state, not a clean database.

## Scope Risk Traceability Summary

All 10 scope risks (SR-01 through SR-10) are traced:
- SR-01, SR-03, SR-04 map to architecture risks R-15, R-05, R-03 respectively with ADR-backed resolutions.
- SR-02 maps to R-07; documented in ADR-005 with impl-completeness test replacement.
- SR-08, SR-09 map to R-04 and R-02 respectively with FR-level spec requirements and acceptance criteria.
- SR-05, SR-06, SR-07, SR-10 have no corresponding architecture risks — they are addressed by hard constraints (C-06, C-08, C-09) and NF-06 test count baseline.

## Knowledge Stewardship

- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" — no domain-relevant lesson-learned entries found; general process outcomes returned.
- Queried: /uni-knowledge-search for "risk pattern async storage pool migration" — found entry #2044 (RPITIT non-object-safety, informs R-07) and #2057 (drain task shutdown protocol, informs R-02).
- Queried: /uni-knowledge-search for "SQLite migration connection pool async spawn_blocking" — found entry #2060 (ADR-003 migration sequencing, informs R-03) and #378 (migration tests need old-schema DBs, informs R-03 scenario coverage).
- Stored: nothing novel to store — R-02 and R-07 are feature-specific manifestations of existing patterns already in Unimatrix (#2057, #2044). No new cross-feature pattern emerged.
