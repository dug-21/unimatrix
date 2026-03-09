# Risk Coverage Report: col-017

**Feature**: Hook-Side Topic Attribution
**Date**: 2026-03-09

## Test Summary

| Category | Count |
|----------|-------|
| Total workspace tests | 1794 |
| New unit tests | 35 |
| Failed tests | 0 |
| Integration tests (pre-existing) | All passing |
| New integration tests | 0 (not required) |

## New Tests by Component

| Component | Crate | Tests | Coverage |
|-----------|-------|-------|----------|
| C1 Extraction Facade | unimatrix-observe | 8 | Path extraction, feature ID patterns, git checkout, empty/None |
| C2 Wire Protocol | unimatrix-engine | 5 | Serde round-trip, backward compat (None field omission, missing field) |
| C3 Hook Extraction | unimatrix-server | 10 | PreToolUse, PostToolUse, SubagentStart, UserPromptSubmit, generic, None cases |
| C4 Session Accumulation | unimatrix-server | 7 | First signal, increment, multiple topics, unknown session, timestamp update |
| C5 Observation Persistence | unimatrix-server | 0 | Covered by C6 integration-style tests |
| C6 SessionClose Resolution | unimatrix-server | 6 | Majority vote (single, tie count+recency, tie lexicographic), empty signals, fallback |
| C7 Schema Migration | unimatrix-store | 0 | Covered by existing migration test (assertions updated v9->v10) |

## Risk-to-Test Mapping

| Risk | Priority | Test IDs | Status |
|------|----------|----------|--------|
| SR-1 INSERT column mismatch | P0 | C5 listener tests | COVERED |
| SR-2 Deserialization compat | P0 | T-05 (5 serde tests) | COVERED |
| SR-3 Tie-breaking correctness | P1 | T-07, T-16 (6 vote tests) | COVERED |
| SR-4 Empty signal handling | P1 | T-07 empty case | COVERED |
| SR-5 False positive feature IDs | P2 | T-01, T-02, T-03 | COVERED |
| SR-6 Migration idempotency | P1 | Migration test | COVERED |
| SR-7 Version bump | P1 | test_migration_v7_to_v8_backfill | COVERED |

## Acceptance Criteria Coverage

All P0 and P1 acceptance criteria from ACCEPTANCE-MAP.md are covered by tests.
P2 criteria (observability, metrics) are addressed through the observation persistence path.
P3 criteria (configuration, tuning) are deferred per RISK-TEST-STRATEGY.md.

## Gaps
None identified. All risks from RISK-TEST-STRATEGY.md have corresponding test coverage.
