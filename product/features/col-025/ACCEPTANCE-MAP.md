# col-025 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | `context_cycle(start, goal: "...")` stores the goal text in `cycle_events` on the start event row and in `SessionState.current_goal`. | test | Invoke `handle_cycle_event` with goal param; assert `cycle_events` row `goal` column equals supplied text; assert `state.current_goal = Some(text)`. Include full column-value assertion on all columns (`event_type`, `phase`, `outcome`, `next_phase`, `goal`) to guard against binding transposition (R-08). | PENDING |
| AC-02 | `context_cycle(start)` with no `goal` param stores `NULL` in `cycle_events` and `None` in `SessionState.current_goal`; all downstream behaviour is unchanged. | test | Invoke handler without goal; assert `cycle_events` row `goal IS NULL`; assert `state.current_goal = None`; confirm downstream briefing and injection paths return pre-col-025 behaviour. | PENDING |
| AC-03 | After a server restart, a session associated with a feature cycle that has a stored goal loads `current_goal` from `cycle_events` on resume. | test | Write `cycle_start` row with goal to DB; call `SessionRegister` resume path with that `feature_cycle`; assert `state.current_goal = Some(text)`. | PENDING |
| AC-04 | `context_briefing` called with no `task` param but an active session with a stored goal uses the goal as the retrieval query (step 2 in `derive_briefing_query`). | test | Call `derive_briefing_query(task=None, state.current_goal=Some("goal text"), topic="col-025")`; assert returned query equals goal text. | PENDING |
| AC-05 | `context_briefing` called with an explicit non-empty `task` uses `task` as the query regardless of whether a goal is stored (step 1 wins). | test | Call `derive_briefing_query(task=Some("explicit task"), state.current_goal=Some("goal text"), topic="col-025")`; assert returned query equals `"explicit task"`. | PENDING |
| AC-06 | `context_briefing` called with no `task` and no stored goal falls back to the topic-ID string (step 3), identical to today's behaviour. | test | Call `derive_briefing_query(task=None, state.current_goal=None, topic="col-025")`; assert returned query equals `"col-025"`. | PENDING |
| AC-07 | The `CompactPayload` UDS injection path uses `current_goal` as the query when no task is provided and a goal is stored. | test | Call `IndexBriefingService::index` with `SessionState` where `current_goal=Some("goal text")` and no task param; assert query passed to retrieval equals goal text. Satisfied implicitly by ADR-002 (shared `derive_briefing_query` function) — verify both MCP and UDS CompactPayload paths reach step 2. | PENDING |
| AC-08 | The `SubagentStart` injection path routes to `IndexBriefingService` using `current_goal` as the query when `current_goal` is set, regardless of `prompt_snippet`. | test | Call SubagentStart arm of `dispatch_request` with `state.current_goal=Some("goal text")` and `prompt_snippet="anything"`; assert `IndexBriefingService::index` is invoked with query = `"goal text"`; assert transcript extraction path is NOT taken. | PENDING |
| AC-09 | Schema migration v15→v16 adds `goal TEXT` to `cycle_events` with idempotency guard; existing rows have `goal = NULL`. | test | Migration integration test `migration_v15_to_v16.rs`: (1) apply v16 migration to v15 DB; assert `pragma_table_info(cycle_events)` contains `goal`; assert existing rows `goal IS NULL`; assert `CURRENT_SCHEMA_VERSION = 16`. (2) Re-run migration; assert no error (idempotency). | PENDING |
| AC-10 | All existing `context_cycle`, `context_briefing`, and `context_cycle_review` tests pass without modification (backward compatibility). | test | CI: run full existing test suite on feature branch; zero failures in existing test cases. No existing test file may be modified to accommodate no-goal-path behaviour changes. | PENDING |
| AC-11 | Unit tests cover: goal stored and retrieved on start (AC-01), absent goal (AC-02), resume from DB (AC-03), briefing query derivation priority (AC-04, AC-05, AC-06). | test | Code review: named test cases map 1:1 to AC-01 through AC-06 criterion descriptions. | PENDING |
| AC-12 | `SubagentStart` path: when `current_goal` is `Some`, it wins over a non-empty `prompt_snippet`; the query used is the goal text, not the prompt_snippet text. | test | Call SubagentStart arm with `state.current_goal=Some("goal text")` and `prompt_snippet="non-empty snippet"`; assert `IndexBriefingService::index` is called with query = `"goal text"`; assert `prompt_snippet` is NOT used as the query. (SR-03 inversion guard — non-negotiable.) | PENDING |
| AC-13a | A `goal` value exceeding `MAX_GOAL_BYTES` (1 024 bytes) on the MCP path is rejected with a descriptive structured error; no DB write occurs. | test | Supply 1 025-byte goal to `context_cycle(start)` MCP handler; assert `CallToolResult::error(...)` returned; assert no row written to `cycle_events`. Also test boundary: 1 024-byte goal is accepted. Constant: `MAX_GOAL_BYTES = 1024`. | PENDING |
| AC-13b | A `goal` value exceeding `MAX_GOAL_BYTES` (1 024 bytes) on the UDS path is truncated at the nearest valid UTF-8 character boundary at or below 1 024 bytes and written; the truncated value appears in `cycle_events`. | test | Supply oversized goal via UDS path; assert `cycle_events` row `goal` column byte length ≤ 1 024; assert value is valid UTF-8; assert `tracing::warn!` was emitted. | PENDING |
| AC-14 | Session resume when `cycle_events` has no matching `cycle_start` row (pre-v16 or missing) sets `current_goal = None` and completes registration without error. | test | Call `SessionRegister` resume path with `feature_cycle` that has no `cycle_start` row in DB; assert `state.current_goal = None`; assert registration returns `HookResponse::Ack`. | PENDING |
| AC-15 | Session resume when the DB lookup returns an error sets `current_goal = None`, logs the error, and completes registration without propagating the error. | test | Inject DB error on `get_cycle_start_goal` during session registration; assert `state.current_goal = None`; assert registration returns `HookResponse::Ack`; assert `tracing::warn!` containing `"col-025: goal resume lookup failed"` was emitted. | PENDING |
| AC-16 | All migration test files asserting `schema_version` ≤ 15 are updated to assert version 16. | grep | `grep -r "schema_version.*15\|CURRENT_SCHEMA_VERSION.*15" crates/unimatrix-store/tests/` returns no matches after delivery. Files to audit: `migration_v14_to_v15.rs`, `sqlite_parity.rs`, `sqlite_parity_specialized.rs`. | PENDING |
| AC-17 | A `goal` value that is an empty string or whitespace-only is normalized to `None` at the MCP handler; no blank string is written to `cycle_events` or placed in `SessionState.current_goal`. | test | Supply `""` and `"   "` as goal to `context_cycle(start)` MCP handler; assert no row written with non-NULL goal; assert `state.current_goal = None`. | PENDING |
| AC-18 | All `format_index_table` output (MCP briefing responses and UDS CompactPayload injection) is prefixed with the `CONTEXT_GET_INSTRUCTION` header exactly once, before the first table row. | test | Call `format_index_table` with one or more entries; assert output starts with `CONTEXT_GET_INSTRUCTION` constant text; assert the constant text does not appear again within the table rows. Verify header present in MCP `context_briefing` response path and in UDS `CompactPayload` injection path independently. | PENDING |

---

## Non-Negotiable Test Scenarios (Gate 3c — from RISK-TEST-STRATEGY.md §Coverage Summary)

The following nine scenarios are required at Gate 3c. They correspond to specific risk IDs
and must be present as named test cases in the delivered test suite.

| # | Scenario | Risk ID | AC Reference |
|---|----------|---------|--------------|
| 1 | `migration_v15_to_v16.rs` with idempotency scenario | R-02 | AC-09 |
| 2 | SubagentStart goal-present → `IndexBriefingService` called with goal as query; transcript/RecordEvent path NOT taken | R-04 | AC-08 |
| 3 | SubagentStart goal-present, `prompt_snippet` non-empty → goal still wins; `IndexBriefingService` called | R-04 | AC-12 |
| 4 | SubagentStart goal-absent → existing `ContextSearch`/transcript path runs unchanged (regression guard) | R-12 | AC-10 |
| 5 | UTF-8 char-boundary truncation at `MAX_GOAL_BYTES` boundary (multi-byte character straddling boundary — no panic) | R-07 | AC-13b |
| 6 | Full column-value assertion on `insert_cycle_event` round-trip (all columns, not just `goal`) | R-08 | AC-01 |
| 7 | DB error on resume → `None` + `tracing::warn!` + registration succeeds | R-03 | AC-15 |
| 8 | `format_index_table` output starts with `CONTEXT_GET_INSTRUCTION` constant exactly once | R-11 | AC-18 |
| 9 | UDS truncate-then-overwrite retry: second write overwrites first (last-writer-wins correctness) | R-13 | (integration scenario) |

---

## Additional Required Test Coverage (from RISK-TEST-STRATEGY.md)

| Scenario | Risk ID | Verification Detail |
|----------|---------|---------------------|
| Pre-delivery call-site audit: exactly one `insert_cycle_event` call site | R-01 | `grep -r "insert_cycle_event" crates/` returns exactly one match in `listener.rs` before signature change |
| SubagentStart: `current_goal = None`, `prompt_snippet = ""` → fallback to `RecordEvent` or topic | R-04 | Unit test: assert neither goal nor prompt_snippet drives the query; RecordEvent/topic fallback runs |
| `get_cycle_start_goal`: returns `Ok(None)` for unknown `cycle_id` | R-03 | Unit test on the store helper directly |
| `get_cycle_start_goal`: returns `Ok(None)` for row where `goal IS NULL` | R-03 | Unit test on the store helper directly |
| `get_cycle_start_goal`: returns `Ok(Some(goal))` for valid stored goal | R-03 | Unit test on the store helper directly |
| `current_goal = Some("")` (empty string stored, edge case if normalization skipped on UDS path) | R-04 | Assert SubagentStart non-empty check prevents routing to `IndexBriefingService`; falls through to transcript path |
| `goal` param present on `cycle_phase_end` or `cycle_stop` event — must be ignored | FR-01 | Unit test: goal param on non-start event; assert no `goal` column updated and no error returned |
| Schema version gate returns clear error when old binary connects to v16 DB | R-14 | Code review; existing gate behaviour verified |
| `format_index_table` test helper `strip_briefing_header(s: &str) -> &str` exists for raw-table assertions | R-11 | Code review: shared helper used rather than per-test string removal |
| Goal exactly `MAX_GOAL_BYTES` bytes (1 024) accepted on MCP path without error | R-07 | Boundary test alongside AC-13a |
| Goal exactly `MAX_GOAL_BYTES` bytes (1 024) on UDS path stored verbatim without truncation or warn | R-07 | Boundary test alongside AC-13b |
