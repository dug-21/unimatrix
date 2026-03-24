# Risk Coverage Report: col-025 — Feature Goal Signal

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `insert_cycle_event` signature change breaks call sites | Compile gate (workspace builds clean); `test_insert_cycle_event_full_column_assertion` | PASS | Full |
| R-02 | Migration v15→v16 cascade breaks CI | `test_v15_to_v16_migration_idempotent`, `test_v15_to_v16_migration_adds_goal_column`, `test_current_schema_version_is_16`, `test_schema_version_is_16_after_migration`; cascade files updated per pattern #2933 | PASS | Full |
| R-03 | Resume DB error silently degrades | `test_resume_loads_goal_from_cycle_events`, `test_resume_no_cycle_start_row_sets_none`, `test_resume_null_goal_row_sets_none`, `test_resume_no_feature_cycle_skips_goal_lookup`; `test_resume_db_error_degrades_to_none_with_warn` **MISSING** (Gate 3c scenario #7) | PARTIAL | Partial — DB error path not covered as named test |
| R-04 | SubagentStart goal-present branch falls through | `test_subagent_start_goal_present_routes_to_index_briefing`, `test_subagent_start_goal_absent_uses_existing_path`, `test_subagent_start_goal_empty_string_falls_through`; `test_subagent_start_goal_wins_over_nonempty_prompt_snippet` **MISSING** (Gate 3c scenario #3) | PARTIAL | Partial — prompt_snippet precedence test missing |
| R-05 | `synthesize_from_session` old format breaks tests | `test_synthesize_from_session_returns_current_goal`, `test_synthesize_from_session_returns_none_when_goal_absent`, `test_synthesize_from_session_ignores_topic_signals`; existing tests updated | PASS | Full |
| R-06 | `SessionState` struct literals not updated | `test_register_session_initializes_current_goal_to_none`, `test_session_state_current_goal_field_exists`, `test_set_current_goal_sets_and_overwrites`; compile-time gate passes | PASS | Full |
| R-07 | UTF-8 truncation panic on UDS path | `test_uds_goal_truncation_at_utf8_char_boundary`, `test_uds_goal_exact_max_bytes_stored_verbatim`, `test_uds_goal_over_max_bytes_ascii_truncated`, `test_uds_goal_two_byte_char_at_boundary` | PASS | Full |
| R-08 | Goal written to wrong column binding | `test_insert_cycle_event_full_column_assertion` (all columns asserted by name) | PASS | Full |
| R-09 | No-goal path changes downstream behavior | Existing test suite unmodified; all 3461 unit/integration tests pass; `test_no_goal_briefing_behavior_unchanged` | PASS | Full |
| R-10 | Resume query returns wrong row (multiple cycle_start) | `test_get_cycle_start_goal_multiple_start_rows_returns_first` | PASS | Full |
| R-11 | CONTEXT_GET_INSTRUCTION header breaks existing format_index_table tests | `test_format_index_table_starts_with_instruction_header_exactly_once`, all existing tests updated with `strip_briefing_header` helper | PASS | Full |
| R-12 | SubagentStart IndexBriefingService wiring untested | `test_subagent_start_goal_present_routes_to_index_briefing`; `test_cycle_start_with_goal_persists_across_restart` (infra-001) | PASS | Full |
| R-13 | UDS truncate-then-overwrite retry incorrect | `test_uds_truncate_then_overwrite_last_writer_wins` **MISSING** (Gate 3c scenario #9) | None | Gap — retry sequence not implemented as named test |
| R-14 | Old binary error on v16 DB | Existing schema version gate behavior; `test_current_schema_version_is_16` | PASS | Full |

---

## Test Results

### Unit Tests (cargo test --workspace)

All tests pass on clean run. Initial run showed 3 intermittent failures (pre-existing timing issue in concurrent test pool, GH#303), fixed by running sequentially.

- **Total**: 3461
- **Passed**: 3461
- **Failed**: 0
- **Ignored**: 27

Breakdown by crate (relevant to col-025):
- `unimatrix-server --lib`: 1970 passed (includes all new col-025 unit tests)
- `unimatrix-store` integration tests (migration_v15_to_v16.rs): 13 passed

### Integration Tests (infra-001)

#### Smoke Gate (mandatory)
- **Command**: `pytest suites/ -m smoke --timeout=60`
- **Result**: 20 passed, 0 failed
- **Status**: PASS — gate cleared

#### Suite: `protocol` (13 tests)
- **Result**: 13 passed, 0 failed

#### Suite: `tools` (93 tests including 6 new col-025 tests)
- **Result**: 92 passed, 0 failed, 1 xfailed (pre-existing GH#305: `test_retrospective_baseline_present`)
- **New col-025 tests**: 6 passed
  - `test_cycle_start_goal_accepted` — PASS
  - `test_cycle_start_goal_exceeds_max_bytes_rejected` — PASS
  - `test_cycle_start_goal_at_exact_max_bytes_accepted` — PASS
  - `test_cycle_start_empty_goal_treated_as_no_goal` — PASS
  - `test_cycle_start_whitespace_goal_normalized_to_none` — PASS
  - `test_briefing_response_starts_with_context_get_instruction` — PASS

#### Suite: `lifecycle` (36 tests including 2 new col-025 tests)
- **Result**: 34 passed, 0 failed, 2 xfailed (pre-existing GH#291: `test_dead_knowledge_entries_deprecated_by_tick`; GH#305: retrospective)
- **New col-025 tests**: 2 passed
  - `test_cycle_start_with_goal_persists_across_restart` — PASS
  - `test_cycle_goal_drives_briefing_query` — PASS

#### Suite: `edge_cases` (24 tests)
- **Result**: 23 passed, 0 failed, 1 xfailed (pre-existing: `test_100_rapid_sequential_stores`)

#### Total Integration Tests Run
- **Total**: 166 (smoke: 20, protocol: 13, tools: 93, lifecycle: 36, edge_cases: 24)
- **Passed**: 162
- **Failed**: 0
- **XFailed (pre-existing)**: 4

---

## Gaps

### Gap 1: `test_resume_db_error_degrades_to_none_with_warn` (Gate 3c scenario #7, R-03, AC-15)

**Status**: Missing as a named test.

**Risk**: R-03 (Medium) — Session resume when DB lookup returns error must set `current_goal = None`, emit `tracing::warn!`, and complete registration. The `tracing::warn!` log assertion is a required AC-15 criterion.

**What exists**: `test_resume_loads_goal_from_cycle_events`, `test_resume_no_cycle_start_row_sets_none`, and `test_resume_null_goal_row_sets_none` all pass. The DB error path is handled by the `unwrap_or_else` pattern in the implementation (ADR-004). However, no test injects a DB error to assert the warn log and degradation behavior.

**Impact**: The degradation contract is implemented correctly (code review confirmed) but is not verified by an automated test. A future regression in the `unwrap_or_else` handler would not be caught by the current test suite.

### Gap 2: `test_subagent_start_goal_wins_over_nonempty_prompt_snippet` (Gate 3c scenario #3, R-04, AC-12)

**Status**: Missing as a named test.

**Risk**: R-04 (High) — When `current_goal` is `Some` and `prompt_snippet` is non-empty, the goal must win unconditionally (ADR-003 SR-03 inversion guard).

**What exists**: `test_subagent_start_goal_present_routes_to_index_briefing` tests the goal-present path with a non-goal query string, confirming the routing works. However, it does not explicitly set `prompt_snippet` in the dispatch payload and assert `prompt_snippet` is NOT used as the query. The test `test_subagent_start_non_subagent_source_skips_goal_branch` tests a different scenario (wrong source, not prompt_snippet precedence).

**Impact**: The ADR-003 inversion guard (goal wins over prompt_snippet) is implemented correctly in the code. The specific scenario where a non-empty `prompt_snippet` is present alongside a non-empty `current_goal` is not tested in isolation.

### Gap 3: `test_uds_truncate_then_overwrite_last_writer_wins` (Gate 3c scenario #9, R-13)

**Status**: Missing as a named test.

**Risk**: R-13 (Medium) — The ADR-005 "last-writer-wins" retry sequence (truncated write → corrected second write overwrites first) is untested. This verifies the `INSERT` semantics allow overwriting a `cycle_start` row's goal.

**What exists**: `test_uds_cycle_start_goal_truncated_at_char_boundary` tests a single truncated write. The retry-overwrite scenario (writing twice to the same `cycle_id`) is not tested.

**Impact**: If `insert_cycle_event` uses `INSERT OR IGNORE` semantics, the truncated first value would be retained after the corrected retry, silently corrupting the goal. Code review did not reveal `OR IGNORE` semantics (INSERT is used), but no automated test validates this assumption.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_insert_cycle_event_full_column_assertion` (all columns asserted); `test_uds_cycle_start_sets_current_goal_in_registry` |
| AC-02 | PASS | `test_insert_cycle_event_goal_none_writes_null`; `test_uds_cycle_start_no_goal_sets_none` |
| AC-03 | PASS | `test_resume_loads_goal_from_cycle_events`; `test_cycle_start_with_goal_persists_across_restart` (infra-001) |
| AC-04 | PASS | `test_derive_briefing_query_step2_returns_current_goal` |
| AC-05 | PASS | `test_derive_briefing_query_step1_wins_over_goal` |
| AC-06 | PASS | `test_derive_briefing_query_step3_fallback_when_no_goal`; `test_derive_briefing_query_step3_no_session_state` |
| AC-07 | PASS | `test_compact_payload_uses_current_goal_as_query` (verified at code review — `derive_briefing_query` called on CompactPayload path) |
| AC-08 | PASS | `test_subagent_start_goal_present_routes_to_index_briefing` |
| AC-09 | PASS | `test_v15_to_v16_migration_idempotent`; `test_v15_to_v16_migration_adds_goal_column`; `test_v15_pre_existing_rows_have_null_goal` |
| AC-10 | PASS | All 3461 unit+integration tests pass; no existing test modified to accommodate no-goal path |
| AC-11 | PASS | Named test cases map 1:1 to AC-01–AC-06 criterion descriptions per test plan |
| AC-12 | PARTIAL | Goal-present branch confirmed routes to IndexBriefingService; `test_subagent_start_goal_wins_over_nonempty_prompt_snippet` (explicit prompt_snippet precedence guard) missing (Gap 2) |
| AC-13a | PASS | `test_cycle_start_goal_exceeds_max_bytes_rejected` (infra-001); `test_cycle_start_goal_at_exact_max_bytes_accepted` (boundary) |
| AC-13b | PASS | `test_uds_goal_truncation_at_utf8_char_boundary` (multi-byte boundary, no panic); `test_uds_goal_exact_max_bytes_stored_verbatim` |
| AC-14 | PASS | `test_resume_no_cycle_start_row_sets_none` |
| AC-15 | PARTIAL | DB error degradation implemented via `unwrap_or_else` + `tracing::warn!` (ADR-004 code review). `test_resume_db_error_degrades_to_none_with_warn` (named test with warn assertion) missing (Gap 1) |
| AC-16 | PASS | `test_current_schema_version_is_16` in `migration_v15_to_v16.rs`; `migration_v14_to_v15.rs` updated to `>= 15` per pattern #2933; `sqlite_parity.rs` and `sqlite_parity_specialized.rs` use `>= 9` (no literal 15 assertions) |
| AC-17 | PASS | `test_cycle_start_empty_goal_normalized_to_none` (infra-001); `test_cycle_start_whitespace_goal_normalized_to_none` (infra-001); unit tests in `mcp/tools.rs` |
| AC-18 | PASS | `test_format_index_table_starts_with_instruction_header_exactly_once`; `test_briefing_response_starts_with_context_get_instruction` (infra-001); existing `format_index_table` tests updated with `strip_briefing_header` |

---

## Gate 3c Non-Negotiable Scenario Verification

| # | Required Test Name | Status | Location |
|---|-------------------|--------|----------|
| 1 | `test_v15_to_v16_migration_idempotent` | PASS | `crates/unimatrix-store/tests/migration_v15_to_v16.rs` |
| 2 | `test_subagent_start_goal_present_routes_to_index_briefing` | PASS | `crates/unimatrix-server/src/uds/listener.rs` |
| 3 | `test_subagent_start_goal_wins_over_nonempty_prompt_snippet` | **MISSING** | Gap 2 — not implemented |
| 4 | `test_subagent_start_goal_absent_uses_existing_transcript_path` (implemented as `test_subagent_start_goal_absent_uses_existing_path`) | PASS | `crates/unimatrix-server/src/uds/listener.rs` |
| 5 | `test_uds_goal_truncation_at_utf8_char_boundary` | PASS | `crates/unimatrix-server/src/uds/listener.rs` |
| 6 | `test_insert_cycle_event_full_column_assertion` | PASS | `crates/unimatrix-store/tests/migration_v15_to_v16.rs` |
| 7 | `test_resume_db_error_degrades_to_none_with_warn` | **MISSING** | Gap 1 — not implemented |
| 8 | `test_format_index_table_starts_with_instruction_header_exactly_once` | PASS | `crates/unimatrix-server/src/mcp/response/briefing.rs` |
| 9 | `test_uds_truncate_then_overwrite_last_writer_wins` | **MISSING** | Gap 3 — not implemented |

**Score: 6/9 non-negotiable scenarios present as named tests.**

---

## New Integration Tests Added (Stage 3c)

The following infra-001 tests were added during Stage 3c to cover scenarios only observable through the MCP interface:

### harness/client.py
- Added `goal: str | None = None` parameter to `context_cycle()` method.

### suites/test_tools.py (6 new tests)
- `test_cycle_start_goal_accepted` — goal parameter accepted, cycle start succeeds
- `test_cycle_start_goal_exceeds_max_bytes_rejected` — 1025-byte goal rejected with descriptive error (AC-13a)
- `test_cycle_start_goal_at_exact_max_bytes_accepted` — 1024-byte goal accepted at boundary
- `test_cycle_start_empty_goal_treated_as_no_goal` — empty string normalized to None (AC-17)
- `test_cycle_start_whitespace_goal_normalized_to_none` — whitespace-only normalized to None (AC-17)
- `test_briefing_response_starts_with_context_get_instruction` — MCP briefing output starts with CONTEXT_GET_INSTRUCTION (AC-18)

### suites/test_lifecycle.py (2 new tests)
- `test_cycle_start_with_goal_persists_across_restart` — goal survives server restart; session resume loads from DB (AC-03)
- `test_cycle_goal_drives_briefing_query` — goal-driven cycle → briefing response includes CONTEXT_GET_INSTRUCTION (AC-18 end-to-end)

---

## Pre-Existing xfail Markers

No new xfail markers were added by this feature. The following pre-existing xfail tests were observed:
- `test_retrospective_baseline_present` (GH#305) — pre-existing, unrelated to col-025
- `test_dead_knowledge_entries_deprecated_by_tick` (GH#291) — pre-existing, unrelated to col-025
- `test_100_rapid_sequential_stores` — pre-existing, unrelated to col-025

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "testing procedures gate verification integration test triage" — found #553 (worktree isolation), #487 (workspace test without hanging), #750 (pipeline validation tests); none directly applicable to col-025 test execution.
- Stored: nothing novel — the `strip_briefing_header` helper pattern is col-025-specific and the `get_result_text` pitfall (calls `assert_tool_success`) is already documented in assertions.py inline comments. No cross-feature reusable pattern emerged that wasn't already known.
