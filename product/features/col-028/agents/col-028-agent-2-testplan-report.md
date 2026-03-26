# Agent Report: col-028-agent-2-testplan

Phase: Stage 3a — Test Plan Design
Feature: col-028 Unified Phase Signal Capture (Read-Side + query_log)

## Output Files

- `/workspaces/unimatrix/product/features/col-028/test-plan/OVERVIEW.md`
- `/workspaces/unimatrix/product/features/col-028/test-plan/session-state.md`
- `/workspaces/unimatrix/product/features/col-028/test-plan/tools-read-side.md`
- `/workspaces/unimatrix/product/features/col-028/test-plan/usage-d01-guard.md`
- `/workspaces/unimatrix/product/features/col-028/test-plan/migration-v16-v17.md`

## Risk Coverage Summary

| Priority | Risks | Covered By |
|----------|-------|-----------|
| Critical | R-01 (dedup), R-02 (column drift), R-03 (phase race) | AC-07 (positive+negative), AC-17 round-trip, AC-12 code review |
| High | R-04 through R-09 | AC-16 infra test, AC-22 grep, AC-23 compile, AC-05, AC-06, AC-20 |
| Medium | R-10 through R-13, IR-01 through IR-04 | AC-16 drain-flush, T-V17-04, T-V17-05, AC-10 both arms, eval helper update |
| Low | R-14 through R-16, edge cases | AC-11 existing, AC-24 code review, AC-07 canary |

All 24 AC-IDs from ACCEPTANCE-MAP.md are mapped to test scenarios.
No AC is without a test or explicit code-review gate.

## Integration Suite Plan

Suites to run in Stage 3c:
- `smoke` — mandatory gate
- `tools` — four read-side handlers modified
- `lifecycle` — D-01 guard sequence, phase in query_log
- `confidence` — context_get weight changed (1→2)

New infra-001 tests to add (Stage 3c):
1. `test_briefing_then_get_does_not_consume_dedup_slot` → `suites/test_lifecycle.py`
2. `test_context_search_phase_persisted_to_query_log` → `suites/test_lifecycle.py`

Both use `server` fixture (function scope).

## Key Design Decisions in Test Plans

### AC-07 Negative Arm
The negative arm of the D-01 guard test is documented explicitly in
`usage-d01-guard.md`. It demonstrates — via direct UsageDedup manipulation or
via a counterfactual documentation test — that WITHOUT the guard, briefing DOES
consume the dedup slot and subsequent context_get produces access_count += 0.
This satisfies the requirement that the guard be proven load-bearing, not redundant.

### AC-12 Manual Gate
Phase snapshot placement is a code-review gate (not automatable). The test plan
(`tools-read-side.md` Part E) documents the exact four-handler inspection procedure
the reviewer must perform, including the `context_search`-specific C-04 check for
exactly one `get_state` call.

### AC-16/AC-17 Real Analytics Drain
Both phase round-trip tests mandate using the real analytics drain (pattern #3004).
No mocks for the drain path. This is required because the SR-01 risk (positional
column drift) can only be detected at runtime through the full INSERT → drain →
SELECT → deserialize path. A mock would bypass the exact sites that can drift.

### T-V17-05 Pre-existing Row Pattern
The v16 database builder in `migration_v16_to_v17.rs` must create `query_log`
WITHOUT the phase column (the v16 shape: 9 columns, no phase). T-V17-05 inserts
a row pre-migration and confirms phase = None post-migration, verifying
`row_to_query_log` correctly maps NULL → None at index 9.

## Open Questions

1. **Analytics drain flush mechanism**: Pattern #3004 prescribes flushing the drain
   in AC-16 and AC-17 tests. The exact flush API (channel drop, explicit flush call,
   or sleep) depends on how the analytics drain is wired in server tests. The
   implementer should follow the exact pattern used in existing crt-025 integration
   tests (which also tested phase-snapshot analytics writes).

2. **UsageDedup testability**: The negative arm of AC-07 requires either direct
   access to `UsageDedup.access_counted` or the ability to construct a `UsageDedup`
   independently. If `UsageDedup` is private to `services/usage.rs`, the negative
   arm must use the documented-counterfactual approach (documented in
   `usage-d01-guard.md` with the alternative pattern).

3. **handler-level unit tests for AC-01 through AC-04**: These require constructing
   a `UnimatrixHandler` in a test context and observing the `UsageContext` passed to
   `UsageService`. If there is no existing handler unit test infrastructure, the
   implementer should use the same approach as existing handler tests in the codebase
   (check `mcp/tools.rs` test module for precedents).

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: "decision", topic: "col-028") —
  found #3505 (ADR-002 phase snapshot), #3507 (ADR-004 confirmed_entries cardinality),
  #3508 (ADR-005 confirmed_entries contract), #3513 (ADR-001 phase helper free function),
  #3518 (ADR-006 UsageContext doc comment). All five directly applied.
- Queried: `/uni-knowledge-search` (query: "session state testing patterns UsageDedup
  integration tests") — found #3004 (analytics drain causal test pattern), #3180
  (SessionState field additions require test helper update), #3210 (SessionRegistry
  access patterns), #3479 (two-site atomicity enforcement), #3513 (ADR-001). All applied.
- Stored: nothing novel to store — all patterns applied (#3004, #3180, #3479, #3513,
  #3503, #3510) were already documented in Unimatrix. col-028 test plans apply them
  but do not introduce a cross-feature testing pattern not already captured.
  The AC-07 negative-arm documentation approach (counterfactual test for load-bearing
  guards) may be worth storing after Stage 3c validates whether the approach was
  effective — deferred to Stage 3c stewardship.
