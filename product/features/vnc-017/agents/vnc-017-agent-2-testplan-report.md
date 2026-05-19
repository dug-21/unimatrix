# Agent Report: vnc-017-agent-2-testplan

**Phase**: Stage 3a — Test Plan Design
**Agent ID**: vnc-017-agent-2-testplan

## Deliverables

| File | Purpose |
|------|---------|
| `product/features/vnc-017/test-plan/OVERVIEW.md` | Test strategy, risk-to-test mapping, integration harness plan |
| `product/features/vnc-017/test-plan/query_incoming_edges.md` | Unit test expectations for the new store read function |
| `product/features/vnc-017/test-plan/redirect_loop.md` | Unit test expectations for the redirect loop in context_correct |
| `product/features/vnc-017/test-plan/response_format.md` | Test expectations for the FR-10 4-variant format table |

## Risk Coverage Summary

| Risk ID | Priority | Coverage Level | Test Location |
|---------|----------|----------------|---------------|
| R-01 | Critical | Full | Rust unit (compile-time + behavioral) |
| R-02 | Critical | Full | Rust unit structural (Supersedes-only target returns empty) |
| R-03 | High | Full | Rust unit (1000-row high-cardinality seed) |
| R-04 | High | Partial | Rust unit (call-count assertion; mock pattern TBD by implementer) |
| R-05 | High | Full | Rust unit (55-edge ceiling test + 50-edge no-truncation test) |
| R-06 | Critical | Full | Rust unit (quarantined, deprecated, mixed Active+Quarantined fan-in) |
| R-07 | High | Full | Rust unit (Supersedes-only path returns empty, no append) |
| R-08 | High | Full | Integration (AC-16: full redirect + graph tick, no DependencyOnDeprecated) |
| R-09 | High | Full | Rust unit (UNIQUE conflict = redirected++, failed==0) |
| R-10 | High | Full | Rust unit (Phase B + loop double-write idempotency) |
| R-11 | High | Full | Integration (exact substring match in CallToolResult) |
| R-12 | Med | Full | Rust unit (SQL-level exclusion = no log for Supersedes-only) |
| R-13 | Med | Full | Existing test suite regression pass |
| R-14 | Low | Accepted | Code review only (no test gate) |

## Integration Suite Plan

- `smoke` — mandatory gate
- `tools` + `lifecycle` — feature-specific suites
- 5 new integration tests in `suites/test_lifecycle.py`
- No new suites required

## Open Questions

1. **Response text singular/plural**: FR-10 format string uses "incoming edges" for all N values including N=1. The test plan specifies plural form always. If the implementation adds special-casing for N=1, the unit tests must be updated to match.

2. **RedirectSummary struct vs inline locals**: The implementation brief gives two options for the accumulator — named struct vs inline `let` bindings. The response_format unit tests are written against a conceptual `RedirectSummary`; the implementer may use inline locals. The unit test assertions need only be on the final produced text string, not on the struct fields directly.

3. **AC-16 implementation feasibility**: The integration test `test_correct_redirected_edges_clear_dependency_detection` requires triggering a graph tick via the MCP interface. Confirm with the harness that the detection event can be triggered and asserted through available infra-001 test patterns (e.g., does `context_status` expose tick events, or is there a direct detection check endpoint?).

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 10 entries. Most relevant: #4463 (ADR-004: ceiling and zero-edge response), #4462 (ADR-003: failure posture, return contract table), #4460 (ADR-001: terminal-active resolution), #4461 (ADR-002: SQL-level Supersedes exclusion). All four ADRs directly informed component test plan structure.
- Queried: `context_search` for "vnc-017 architectural decisions" — returned ADR-004, ADR-001, ADR-002 entries. Confirmed all vnc-017 ADRs found.
- Queried: `context_search` for "graph edge redirect testing patterns" — returned lesson #4437 (test_protocol.py tool count assertion), ADR-009 vnc-015 (context_edge design). Neither changes the test plan for this feature (no new tool being added).
- Stored: nothing novel to store — the test plan patterns used here (high-cardinality seed test for SQL filter validation, compile-time structural test as gate) are standard Rust unit test patterns already covered by existing entries. The R-02 SQL-level vs loop-level exclusion discrimination test pattern may be worth storing after Stage 3c validates it.
