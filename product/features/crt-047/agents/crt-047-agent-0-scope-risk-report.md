# Agent Report: crt-047-agent-0-scope-risk

## Output
- Produced: `product/features/crt-047/SCOPE-RISK-ASSESSMENT.md`
- Line count: 40 (limit: 100)
- Risks identified: 8 (SR-01 through SR-08)

## Risk Summary
| Severity | Count |
|----------|-------|
| High     | 3 (SR-01, SR-03, SR-07) |
| Medium   | 4 (SR-02, SR-04, SR-05, SR-08) |
| Low      | 1 (SR-06) |

## Top 3 Risks for Architect Attention
1. **SR-07** (High): Baseline window ordering key — `computed_at` is mutable under `force=true`; using it as the ordering key makes the N-cycle window non-deterministic. Use `feature_cycle` (primary key) instead.
2. **SR-01** (High): AUDIT_LOG operation string for deprecations must be verified across all write paths before the orphan attribution SQL is designed. Silent mis-count if any path uses a different operation string.
3. **SR-03** (High): Three migration paths (db.rs + migration.rs + legacy DDL) must all be updated. Integration test must open a real v23 database through `Store::open()`, not just the migration function in isolation.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for lesson-learned failures/gate rejection — found #4095 (parallel schema version conflict), #4153 (three migration paths), #378 (old-schema DB tests); all directly applicable.
- Queried: `/uni-knowledge-search` for risk patterns — found #3426 (formatter regression), #1616 (background tick ordering); not applicable to this domain.
- Queried: `/uni-knowledge-search` for AUDIT_LOG join patterns — found #4047 (AuditEvent 5-surface update), #102 (audit log reverse scan); informed SR-01.
- Queried: `/uni-knowledge-search` for SUMMARY_SCHEMA_VERSION — found #3794 (ADR-002 crt-033 bump policy); confirmed SR-04 blast-radius understanding.
- Stored: nothing novel to store — SR-07 (mutable ordering key in rolling baseline) is specific to crt-047's design choice and not yet observed across 2+ features. Will reassess post-delivery.
