# col-025 Test Plan Overview — Feature Goal Signal

## Test Strategy

This feature spans two crates (`unimatrix-store` and `unimatrix-server`) across
eight architectural components. The test approach uses three tiers:

| Tier | Scope | Location |
|------|-------|----------|
| Unit | Per-component pure logic (query derivation, byte guards, normalization, struct field access) | `#[cfg(test)] mod tests` in source files |
| Integration (store) | Migration round-trips, DB helper correctness | `crates/unimatrix-store/tests/` |
| Integration (infra-001) | End-to-end MCP wire protocol, lifecycle flows, tool parameter validation | `product/test/infra-001/suites/` |

**Non-negotiable principle**: no risk in the Risk Strategy is left without at
least one named test. Every non-negotiable Gate 3c scenario maps to a specific
test case in the appropriate component test plan.

---

## Risk-to-Test Mapping

| Risk ID | Priority | Description | Test File(s) | Key Test Name(s) |
|---------|----------|-------------|--------------|------------------|
| R-01 | Medium | `insert_cycle_event` signature change breaks call sites | `schema-migration-v16.md` | pre-delivery grep; compile gate |
| R-02 | High | Migration test cascade breaks CI | `schema-migration-v16.md` | `test_current_schema_version_is_16`, `test_v15_to_v16_migration_adds_goal_column`, `test_v15_to_v16_migration_idempotent` |
| R-03 | Medium | Resume DB error silently degrades | `session-resume.md` | `test_resume_db_error_degrades_to_none_with_warn`, `test_resume_no_cycle_start_row`, `test_resume_null_goal_row` |
| R-04 | High | SubagentStart goal-present falls through to transcript | `subagent-start-injection.md` | `test_subagent_start_goal_present_routes_to_index_briefing`, `test_subagent_start_goal_wins_over_nonempty_prompt_snippet` |
| R-05 | High | `synthesize_from_session` old format breaks tests | `briefing-query-derivation.md` | `test_derive_briefing_query_step2_returns_current_goal`, updated existing tests |
| R-06 | High | `SessionState` struct literals not updated | `session-state-extension.md` | compile-time gate; `test_session_state_current_goal_field_exists` |
| R-07 | Medium | UTF-8 truncation panic on UDS path | `cycle-event-handler.md` | `test_uds_goal_truncation_at_utf8_char_boundary`, `test_uds_goal_exact_max_bytes_stored_verbatim` |
| R-08 | Medium | Goal written to wrong column binding | `schema-migration-v16.md` | `test_insert_cycle_event_full_column_assertion` |
| R-09 | Low | No-goal path changes downstream behavior | `briefing-query-derivation.md`, infra-001 lifecycle | existing tests pass unmodified (AC-10) |
| R-10 | Low | Resume query returns wrong row (multiple cycle_start) | `session-resume.md` | `test_get_cycle_start_goal_multiple_start_rows_returns_first` |
| R-11 | High | `CONTEXT_GET_INSTRUCTION` breaks existing format_index_table tests | `format-index-table-header.md` | `test_format_index_table_starts_with_instruction_header`, all audited tests updated |
| R-12 | High | SubagentStart IndexBriefingService wiring untested | `subagent-start-injection.md` | infra-001 `test_cycle_start_with_goal_subagent_receives_index_briefing` |
| R-13 | Medium | UDS truncate-then-overwrite retry incorrect | `cycle-event-handler.md` | `test_uds_truncate_then_overwrite_last_writer_wins` |
| R-14 | Low | Old binary error on v16 DB | `schema-migration-v16.md` | existing schema version gate (no new test needed) |

---

## Scope Risk Traceability

| SR | Resolution | Test Coverage |
|----|-----------|---------------|
| SR-01: Migration test cascade | R-02 | `migration_v15_to_v16.rs` new file + cascade audit on three existing files |
| SR-02: Unbounded goal text | R-07, R-13 | UTF-8 boundary test, MCP reject test (AC-13a), UDS truncate test (AC-13b), retry test |
| SR-03: SubagentStart precedence | R-04, R-12 | Five branch tests + infra-001 integration for wiring |
| SR-04: sessions.keywords boundary | — | No test needed; scope guard at code review |
| SR-05: Session resume DB failure | R-03 | Four `get_cycle_start_goal` variant tests + AC-15 warn log assertion |
| SR-06: derive_briefing_query shared path | — | Resolved by architecture; AC-04 (MCP) + AC-07 (UDS) verify both callers |

---

## Cross-Component Test Dependencies

The following component dependencies must be respected in test order and mock
strategy:

```
schema-migration-v16  ──► session-resume (DB helper get_cycle_start_goal)
                      ──► cycle-event-handler (insert_cycle_event with goal)
session-state-extension ──► briefing-query-derivation (current_goal field)
                        ──► subagent-start-injection (current_goal check)
                        ──► cycle-event-handler (set_current_goal call)
                        ──► session-resume (set_current_goal from DB)
cycle-event-handler ──► mcp-cycle-wire-protocol (UDS receives ImplantEvent)
format-index-table-header ──► briefing-query-derivation (output uses format_index_table)
                          ──► subagent-start-injection (output uses format_index_table)
```

Tests for `session-resume` and `cycle-event-handler` require a live `SqlxStore`
with the v16 schema. Migration tests must pass before these.

Tests for `briefing-query-derivation` and `subagent-start-injection` require
`SessionState.current_goal` to be present — `session-state-extension` must be
implemented first.

---

## Integration Harness Plan (infra-001)

### Suites to Run

This feature touches:
- MCP tool parameter (`context_cycle` gains `goal`)
- Store/retrieval behavior (cycle_events DB write, session resume)
- Schema changes (v15 → v16)
- SubagentStart hook arm (new routing branch)
- format_index_table output changes (briefing and CompactPayload)

**Mandatory gate**: `pytest -m smoke` before Gate 3c.

| Suite | Why | Priority |
|-------|-----|----------|
| `smoke` | Minimum gate; catch catastrophic regressions | Mandatory |
| `tools` | `context_cycle` gains `goal` parameter; validate MCP wire handling, reject behavior | Must run |
| `lifecycle` | `context_cycle → restart → SessionRegister` resume flow; schema restart persistence | Must run |
| `protocol` | Ensure MCP handshake and tool discovery still work with the updated schema | Must run |
| `edge_cases` | Unicode boundary values, empty DB operations, goal edge cases | Must run |

Suites `confidence`, `contradiction`, `security`, `volume` are not required
because goal does not touch scoring, NLI, security scanning, or volume paths.
If any smoke test in those suites fails, apply the standard triage protocol.

### New Integration Tests Needed

The following scenarios are only observable through the MCP interface (not
testable by unit tests alone):

| Scenario | Target Suite | Test Function Name |
|----------|-------------|-------------------|
| `context_cycle(start, goal:"...")` stores goal, accessible after restart | `test_lifecycle.py` | `test_cycle_start_with_goal_persists_across_restart` |
| `context_cycle(start, goal:"...")` followed by `context_briefing` uses goal as query (end-to-end) | `test_lifecycle.py` | `test_cycle_goal_drives_briefing_query` |
| `context_cycle(start, goal > 1024 bytes)` returns error, no DB write | `test_tools.py` | `test_cycle_start_goal_exceeds_max_bytes_rejected` |
| `context_cycle(start, goal: "")` treats empty as no-goal | `test_tools.py` | `test_cycle_start_empty_goal_treated_as_no_goal` |
| `context_cycle(start, goal: "  whitespace  ")` normalized to None | `test_tools.py` | `test_cycle_start_whitespace_goal_normalized_to_none` |
| `context_cycle(start, goal: 1024-byte string)` accepted at boundary | `test_tools.py` | `test_cycle_start_goal_at_exact_max_bytes_accepted` |
| SubagentStart with goal-present session returns IndexBriefingService output | `test_lifecycle.py` | `test_cycle_start_with_goal_subagent_receives_index_briefing` |
| `context_briefing` output starts with CONTEXT_GET_INSTRUCTION header | `test_tools.py` | `test_briefing_response_starts_with_context_get_instruction` |

**Fixture recommendations**:
- `server` fixture (fresh DB) for all `test_tools.py` tests.
- `shared_server` fixture for lifecycle tests where state must accumulate across test steps.

### When NOT to Add New Integration Tests

The following behaviors are sufficiently covered by unit tests and do not need
new infra-001 tests:
- UTF-8 boundary truncation (pure function, unit-testable)
- `derive_briefing_query` step priority (pure function, unit-testable)
- `make_session_state` struct literal updates (compile-time check)
- `synthesize_from_session` return value (pure function, unit-testable)

---

## Acceptance Criteria Coverage Map

| AC-ID | Component Test Plan | Coverage Approach |
|-------|--------------------|--------------------|
| AC-01 | cycle-event-handler.md | DB round-trip with full column assertion |
| AC-02 | cycle-event-handler.md | Null/absent goal write + downstream unchanged |
| AC-03 | session-resume.md | DB → SessionRegister resume → current_goal Some |
| AC-04 | briefing-query-derivation.md | `derive_briefing_query` unit test, step 2 wins |
| AC-05 | briefing-query-derivation.md | `derive_briefing_query` unit test, step 1 wins |
| AC-06 | briefing-query-derivation.md | `derive_briefing_query` unit test, step 3 fallback |
| AC-07 | briefing-query-derivation.md | CompactPayload path calls `derive_briefing_query` |
| AC-08 | subagent-start-injection.md | Goal-present routes to IndexBriefingService |
| AC-09 | schema-migration-v16.md | `migration_v15_to_v16.rs` with idempotency |
| AC-10 | All components | Existing test suite passes unmodified |
| AC-11 | All components | Named test cases map 1:1 to AC-01–AC-06 |
| AC-12 | subagent-start-injection.md | Goal wins over non-empty prompt_snippet |
| AC-13a | mcp-cycle-wire-protocol.md | MCP handler rejects > 1024 bytes |
| AC-13b | cycle-event-handler.md | UDS truncates at UTF-8 boundary |
| AC-14 | session-resume.md | Missing row → None, registration succeeds |
| AC-15 | session-resume.md | DB error → None + warn log, registration succeeds |
| AC-16 | schema-migration-v16.md | All cascade files updated to version 16 |
| AC-17 | mcp-cycle-wire-protocol.md | Empty/whitespace goal normalized to None |
| AC-18 | format-index-table-header.md | Header present exactly once |

---

## Non-Negotiable Gate 3c Scenarios

All nine must be present as named test cases:

| # | Test Name | Component Plan | Risk/AC |
|---|-----------|----------------|---------|
| 1 | `test_v15_to_v16_migration_idempotent` | schema-migration-v16.md | R-02 / AC-09 |
| 2 | `test_subagent_start_goal_present_routes_to_index_briefing` | subagent-start-injection.md | R-04 / AC-08 |
| 3 | `test_subagent_start_goal_wins_over_nonempty_prompt_snippet` | subagent-start-injection.md | R-04 / AC-12 |
| 4 | `test_subagent_start_goal_absent_uses_existing_transcript_path` | subagent-start-injection.md | R-12 / AC-10 |
| 5 | `test_uds_goal_truncation_at_utf8_char_boundary` | cycle-event-handler.md | R-07 / AC-13b |
| 6 | `test_insert_cycle_event_full_column_assertion` | schema-migration-v16.md | R-08 / AC-01 |
| 7 | `test_resume_db_error_degrades_to_none_with_warn` | session-resume.md | R-03 / AC-15 |
| 8 | `test_format_index_table_starts_with_instruction_header_exactly_once` | format-index-table-header.md | R-11 / AC-18 |
| 9 | `test_uds_truncate_then_overwrite_last_writer_wins` | cycle-event-handler.md | R-13 |
