# Agent Report: crt-025-agent-3-risk

**Agent**: Risk Strategist (Architecture-Risk mode)
**Feature**: crt-025 — WA-1: Phase Signal + FEATURE_ENTRIES Tagging
**Output**: `/workspaces/unimatrix/product/features/crt-025/RISK-TEST-STRATEGY.md`

## Summary

Produced RISK-TEST-STRATEGY.md with 14 risks across 4 priority levels.

## Risk Summary

| Priority | Count | Risks |
|----------|-------|-------|
| Critical | 2 | R-01 (current_phase mutation timing), R-02 (phase snapshot skew — analytics drain) |
| High | 8 | R-03 (outcome removal), R-04 (cross-cycle threshold), R-05 (migration idempotency), R-06 (phase normalization), R-08 (backward compat), R-10 (fresh DB path), R-11 (FeatureEntry match arms), R-14 (record_feature_entries call sites) |
| Medium | 4 | R-07 (seq duplication), R-09 (hook hard-fail), R-12 (self-exclusion from baseline), R-13 (orphaned phase-end) |

## Top Risks for Implementer Attention

1. **R-01 + R-02 (Critical)**: The entire value of this feature — accurate GNN training labels — depends on phase being captured at the right moment. R-01 (synchronous mutation) and R-02 (analytics drain snapshot) are the two failure modes that silently corrupt training data with no observable error. Both are resolved by design (ADR-001), but the tests must actually verify the ordering guarantees, not just that the path compiles.

2. **R-03 (High, High likelihood)**: Removing `"outcome"` from `CategoryAllowlist` is guaranteed to break at least 4 test functions by name (per ADR-005). This is a known change that must be tracked through all fixtures — including `boosted_categories` config fixtures that may reference `"outcome"` (pattern #2312).

3. **R-06 (High, High likelihood)**: Phase string normalization is the GNN's data quality gate. The validation layer must lowercase-normalize before the space check, not after. Both `phase` and `next_phase` must receive identical normalization. This is where fragmented labels are most likely to slip through during implementation.

## Scope Risk Traceability

All 8 SR-XX scope risks traced. SR-04 and SR-05 have no corresponding architecture-level risks (accepted / resolved in spec). SR-01, SR-02, SR-03, SR-06, SR-07, SR-08 all map to architecture-level risks with test scenarios.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — #1688 (spawn_blocking hot-path lesson) confirmed the session-state-mutation risk pattern is real in this codebase.
- Queried: `/uni-knowledge-search` for schema migration patterns — #1264 (pragma_table_info) and #836 (new-table procedure) confirmed migration test approach.
- Queried: `/uni-knowledge-search` for analytics drain risk — #2125 and #2057 directly applicable to R-02.
- Queried: `/uni-knowledge-search` for CategoryAllowlist changes — #2312 (boosted_categories config gotcha) flagged as edge case in R-03.
- Stored: nothing novel to store — existing patterns #2125/#2057 already capture the drain-path risk. Phase-tagging GNN label risk is feature-specific (first instance of this pattern); will store if it recurs in WA-2/W3-1.
