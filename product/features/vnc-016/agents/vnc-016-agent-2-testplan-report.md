# Agent Report: vnc-016-agent-2-testplan

**Phase**: Test Plan Design (Stage 3a)
**Agent ID**: vnc-016-agent-2-testplan

---

## Output Files

All files written to `product/features/vnc-016/test-plan/`:

| File | Lines | Purpose |
|------|-------|---------|
| `OVERVIEW.md` | ~90 | Test strategy, risk-to-test mapping, integration harness plan |
| `sql-fix.md` | ~80 | SQL fix verification — grep assertions, expected behavior, edge cases |
| `rust-unit-test.md` | ~105 | Arrange/Act/Assert for both Rust unit tests; constraint compliance |
| `harness-client.md` | ~90 | Code inspection assertions, backward compat, integration verification |
| `usage-gate-fix.md` | ~115 | Gate unit test AAA; grep/compile assertions; AC-10 through AC-13 |
| `integration-tests.md` | ~185 | Full AAA for both integration tests; hard constraints; failure triage |

---

## Risk Coverage Mapping

| Risk ID | Priority | Coverage in Test Plans |
|---------|----------|----------------------|
| R-01 | Critical | `integration-tests.md` — positive test fails against un-fixed code; deliberate omission of `feature_cycle` must break assertion |
| R-02 | Critical | `integration-tests.md` — `force=True` literal required in both tests; unique `cycle_id` per function |
| R-03 | Critical | `rust-unit-test.md` — all three sub-assertions required: `is_ok()`, `len() == 1`, `[0] == (A, B)` |
| R-04 | Critical | `rust-unit-test.md` — negative companion is required and non-optional; guards against always-returns-all-edges regression |
| R-05 | High | `sql-fix.md` + `rust-unit-test.md` — store-layer unit test is the sole direct regression guard; integration test provides end-to-end coverage |
| R-06 | High | `integration-tests.md` + `usage-gate-fix.md` — step 4 uses `agent_id=test_agent_id` (Restricted+Write), not `"human"` |
| R-07 | High | `integration-tests.md` — single `cycle_id` binding at top of each test; same variable in every call |
| R-08 | Med | `integration-tests.md` — negative assertion uses `not any(h["rule_name"] == ...)`, not `hotspots == []` |
| R-09 | Low | `harness-client.md` — code inspection: `if feature_cycle is not None` guard present |
| R-10 | Med | `harness-client.md` — full pytest run as regression gate; `feature_cycle` placed after `edges` |

---

## Integration Harness Plan

**Suites to run in Stage 3c**:

| Suite | Reason |
|-------|--------|
| `smoke` | Mandatory minimum gate |
| `tools` | New tests live here; `client.py` changes touch all tool calls |
| `lifecycle` | `feature_entries` write-then-read is lifecycle behavior |
| `security` | `write_capable` gate change touches capability enforcement |

**New integration tests planned**:
- `test_dependency_on_deprecated_e2e` — positive path, 9-step, `server` fixture
- `test_dependency_on_deprecated_no_finding_without_stale_edge` — negative path, `server` fixture

Both are in `suites/test_tools.py` vnc-015 section. No new test files.

---

## AC Verification Readiness

| AC-ID | Test Plan Section | Verification Method |
|-------|------------------|--------------------|
| AC-01 | integration-tests.md | pytest exit 0 |
| AC-02 | integration-tests.md | Code inspection — 9 steps in order |
| AC-03 | integration-tests.md | Code inspection — exact `any(h["rule_name"] == ...)` |
| AC-04 | sql-fix.md | grep assertions |
| AC-05 | harness-client.md | Code inspection + full pytest run |
| AC-06 | OVERVIEW.md | cargo test --workspace + full pytest |
| AC-07 | integration-tests.md | Code inspection — `force=True` literal |
| AC-08 | integration-tests.md | pytest exit 0 |
| AC-09 | rust-unit-test.md | cargo test -p unimatrix-store |
| AC-10 | usage-gate-fix.md | grep + cargo build |
| AC-11 | usage-gate-fix.md | grep (both gate blocks) |
| AC-12 | usage-gate-fix.md | Code inspection |
| AC-13 | usage-gate-fix.md | cargo test -p unimatrix-server |

---

## Open Questions

None. All OQs from SCOPE.md resolved. No blocking questions for Stage 3b implementers.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #4449 (ADR-001 unit test placement), #4445 (SQL column alias silent failure pattern), #4451 (ADR-002 write_capable gate decision). All directly applicable; used to confirm design decisions align with stored ADRs.
- Queried: `context_search("vnc-016 architectural decisions", category="decision", topic="vnc-016")` — returned #4451 and #4449. Confirmed ADRs are stored.
- Queried: `context_search("integration test patterns SQLite feature_entries")` — returned #4445 (silent failure pattern) and #4399 (counter seed matching in tests).
- Stored: entry #4452 "Gate-fix integration tests must use an agent from the previously-broken trust/capability class" via `/uni-store-pattern` — novel pattern not covered by existing #4416 (which covers rejection tests, not the vacuous-pass risk from using the wrong agent class).
