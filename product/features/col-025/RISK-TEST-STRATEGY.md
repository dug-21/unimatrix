# Risk-Based Test Strategy: col-025 — Feature Goal Signal

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `insert_cycle_event` signature change breaks undetected call sites | High | Low | Medium |
| R-02 | Migration v15→v16 test cascade breaks CI on unmodified assertion files | Med | High | High |
| R-03 | Session resume sets `current_goal = None` silently when DB returns error, masking persistent faults | Med | Med | Medium |
| R-04 | SubagentStart goal-present branch incorrectly falls through to transcript path instead of `IndexBriefingService` | High | Med | High |
| R-05 | `synthesize_from_session` removal of topic-signal synthesis breaks existing `derive_briefing_query` tests | Med | High | High |
| R-06 | `SessionState` struct literals in test helpers not updated for `current_goal` field, causing compile errors | Med | High | High |
| R-07 | UDS byte-limit truncation produces a non-UTF-8-boundary slice, causing a panic | High | Low | Medium |
| R-08 | Goal written to wrong column binding position in `insert_cycle_event`, corrupting event rows silently | High | Low | Medium |
| R-09 | No-goal path subtly changes downstream behavior, violating backward compatibility | Med | Low | Low |
| R-10 | Resume query returns wrong row when multiple `cycle_start` rows exist for same `cycle_id` | Low | Low | Low |
| R-11 | `CONTEXT_GET_INSTRUCTION` header breaks existing `format_index_table` tests asserting table content | Med | High | High |
| R-12 | SubagentStart goal-present branch calls `IndexBriefingService` instead of expected `ContextSearch` — new integration surface untested | High | Med | High |
| R-13 | UDS truncate-then-overwrite retry sequence produces incorrect final goal state | Med | Med | Medium |
| R-14 | Old binary connecting to v16 schema fails at runtime with unhelpful error | Med | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: `insert_cycle_event` Signature Change Breaks Undetected Call Sites

**Severity**: High
**Likelihood**: Low
**Impact**: Compile failure or, if the call site exists in a test-only path, silent incorrect behavior in DB rows (NULL goal written where text expected, or vice versa).

**Test Scenarios**:
1. Pre-delivery: grep `crates/` for `insert_cycle_event` — verify exactly one call site exists in `listener.rs`. If more exist, each must receive the `goal` parameter at the correct bind position.
2. Compile the full workspace; no call-site arity errors.
3. Integration test: after calling `context_cycle(start, goal: "test goal")`, read the raw `cycle_events` row and assert the `goal` column equals `"test goal"` (not NULL, not a different column's value).

**Coverage Requirement**: DB round-trip test confirming correct column binding. Architecture open question OQ-01 confirmed one call site; delivery must verify this before touching the signature.

---

### R-02: Migration Test Cascade Breaks CI

**Severity**: Med
**Likelihood**: High (pattern #2933 confirms this is a recurring CI trap)
**Impact**: CI red on unrelated test files asserting `schema_version = 15`; blocks delivery merge.

**Test Scenarios**:
1. Pre-delivery audit: identify every test file asserting `CURRENT_SCHEMA_VERSION` or a literal `15`. Files identified in ARCHITECTURE.md: `migration_v14_to_v15.rs`, `sqlite_parity.rs`, `sqlite_parity_specialized.rs`.
2. New migration test `migration_v15_to_v16.rs`: apply v16 migration to a v15 database, assert `pragma_table_info(cycle_events)` contains `goal` column, assert existing rows have `goal IS NULL`, re-run migration (idempotency), assert no error.
3. Assert `CURRENT_SCHEMA_VERSION = 16` in `db.rs` after delivery.

**Coverage Requirement**: Every file asserting a version ≤ 15 must be updated (AC-16). The new `migration_v15_to_v16.rs` test is non-negotiable.

---

### R-03: Resume DB Error Silently Degrades Without Observable Warning

**Severity**: Med
**Likelihood**: Med
**Impact**: Agents operating after a server restart receive topic-ID fallback instead of goal-driven briefing with no visible indication; hard to diagnose in production.

**Test Scenarios**:
1. Inject a DB error on `get_cycle_start_goal` during session registration. Assert `state.current_goal = None`, assert session registration returns `HookResponse::Ack`, assert `tracing::warn!` with the lookup-failed message was emitted (AC-15).
2. Resume with pre-v16 cycle (no matching `cycle_start` row): assert `state.current_goal = None`, session succeeds (AC-14).
3. Resume with v16 cycle where `goal = NULL` (caller omitted goal): assert `state.current_goal = None`, session succeeds.
4. Resume with valid `cycle_start` row with non-NULL goal: assert `state.current_goal = Some(goal_text)` (AC-03).

**Coverage Requirement**: All four `get_cycle_start_goal` return variants must be covered: `Ok(Some(goal))`, `Ok(None)` from missing row, `Ok(None)` from NULL column, and `Err(...)`. Warn log emission must be asserted, not just tolerated.

---

### R-04: SubagentStart Goal-Present Branch Falls Through to Transcript Path

**Severity**: High
**Likelihood**: Med (SR-03; explicit branch not protected by `derive_briefing_query` shared logic)
**Impact**: When a goal is set, the SubagentStart handler silently falls through to the transcript-extraction path instead of routing to `IndexBriefingService`, defeating the entire injection improvement. All spawned agents receive degraded injection regardless of goal being set.

**Test Scenarios**:
1. `current_goal = Some("feature goal")`, `prompt_snippet = "anything"` → assert `IndexBriefingService::index` is called with query = `"feature goal"`, assert transcript-extraction path is NOT taken (AC-08 / AC-12 / ADR-003).
2. `current_goal = Some("feature goal")`, `prompt_snippet = ""` → assert `IndexBriefingService::index` is called with query = `"feature goal"` (goal wins; ADR-003 §Decision step 2).
3. `current_goal = None`, `prompt_snippet = "non-empty snippet"` → assert existing transcript/RecordEvent path runs (goal-absent branch unchanged).
4. `current_goal = None`, `prompt_snippet = ""` → assert fallback to `RecordEvent` or topic (unchanged fallback).
5. `current_goal = Some("")` (empty string stored, edge case) → assert non-empty check prevents routing to `IndexBriefingService`; falls through to transcript path.

**Coverage Requirement**: All five precedence branches must be exercised. The goal-present-routes-to-IndexBriefingService case (scenario 1) is the critical correctness guard — its absence at gate-3c is a known failure mode (lesson #2758). Both goal-present and goal-absent branches must be tested independently to prevent conditional inversion at the branch point.

---

### R-05: Removal of Topic-Signal Synthesis Breaks Existing `derive_briefing_query` Tests

**Severity**: Med
**Likelihood**: High (ADR-002 explicitly notes "existing `derive_briefing_query` tests... will need updating")
**Impact**: CI failures on tests that set up `topic_signals` and expect `synthesize_from_session` to return the synthesized string. These are not behavioral regressions — they are test-contracts that no longer reflect the new step-2 semantics.

**Test Scenarios**:
1. Identify existing `synthesize_from_session` tests that assert the `"{feature_cycle} {signals}"` format; update them to assert step 2 now returns `current_goal` when `Some`.
2. `derive_briefing_query` called with `task = None`, `current_goal = Some("g")`, populated `topic_signals` → assert step 2 wins with `"g"` (goal, not signal synthesis) (AC-04).
3. `derive_briefing_query` called with `task = None`, `current_goal = None`, populated `topic_signals` → assert step 3 topic-ID string returned; signals no longer affect step 2 (AC-06).

**Coverage Requirement**: Step 2 must be covered with goal `Some` and `None`. All tests asserting the old topic-signal synthesis format must be removed or updated — no dead assertions.

---

### R-06: `SessionState` Struct Literals Not Updated in Test Helpers

**Severity**: Med
**Likelihood**: High (pattern #3180 confirms `SessionState` field additions require updating `make_session_state` and similar helpers)
**Impact**: Compile failure across any test file constructing `SessionState { .. }` without struct update syntax; blocks the entire test suite.

**Test Scenarios**:
1. Pre-delivery: grep `crates/unimatrix-server/src/` for `SessionState {` and `make_session_state` to enumerate all construction sites.
2. Verify each site either uses `..Default::default()` syntax or is updated to include `current_goal: None`.
3. Full `cargo test` passes after field addition with no missing-field compile errors.
4. At least one test helper constructs `SessionState` with `current_goal: Some("test goal")` to confirm the field is exercised.

**Coverage Requirement**: Compile-time risk. The coverage requirement is zero compile failures after adding the field.

---

### R-07: UDS Byte-Limit Truncation Produces Non-UTF-8-Boundary Slice → Panic

**Severity**: High
**Likelihood**: Low (ADR-005 calls out the requirement for char-boundary-safe truncation)
**Impact**: Panic in the UDS listener on a `CYCLE_START_EVENT` with a multi-byte Unicode goal exceeding `MAX_GOAL_BYTES`. The UDS path cannot return an error; it truncates silently. A panic here terminates the server process.

**Test Scenarios**:
1. Supply a goal string that is exactly `MAX_GOAL_BYTES + 1` bytes but whose final byte falls in the middle of a multi-byte UTF-8 character (e.g., a 3-byte CJK character straddling the boundary). Assert the UDS handler does not panic and the stored goal is valid UTF-8 ≤ `MAX_GOAL_BYTES` bytes.
2. Supply a goal that is exactly `MAX_GOAL_BYTES` bytes of valid ASCII. Assert it is stored verbatim (no truncation, no warn log).
3. Supply a goal of `MAX_GOAL_BYTES + 100` bytes of ASCII. Assert truncation to exactly `MAX_GOAL_BYTES` bytes and `tracing::warn!` is emitted.

**Coverage Requirement**: The char-boundary-safe truncation path must have a dedicated unit test with a multi-byte character at the boundary. This is the one path that can cause a server-terminating panic.

---

### R-08: Goal Written to Wrong Column Binding Position

**Severity**: High
**Likelihood**: Low
**Impact**: The `goal` value populates the wrong column in the row (e.g., `outcome` or `next_phase`), corrupting the event log silently. No error is returned; the database accepts the write.

**Test Scenarios**:
1. DB round-trip: write a `cycle_start` event with a known goal string; read back the full row and assert each column (`event_type`, `phase`, `outcome`, `next_phase`, `goal`) contains its expected value. Confirms the `goal` bind is at the correct position and no other column is displaced.
2. Write with `goal = None`; assert `goal IS NULL` in the row and no other column is affected.

**Coverage Requirement**: Full column-value assertion on the inserted row (not just the goal column in isolation) is required to detect binding transposition.

---

### R-09: No-Goal Path Subtly Changes Downstream Behavior

**Severity**: Med
**Likelihood**: Low
**Impact**: Callers that never provide a goal experience changed briefing or injection behavior after delivery, violating NFR-02 (backward compatibility).

**Test Scenarios**:
1. Run the full existing `context_cycle`, `context_briefing`, and `context_cycle_review` test suite on the feature branch without modification — all must pass (AC-10, NFR-02).
2. End-to-end: start a cycle with no goal, call `context_briefing` with no task, assert the query used is the topic-ID string (step 3), identical to pre-col-025 behavior (AC-06).

**Coverage Requirement**: AC-10 is the primary gate for this risk. No existing test may be modified to accommodate no-goal path behavior changes.

---

### R-10: Resume Query Returns Wrong Row When Multiple `cycle_start` Rows Exist

**Severity**: Low
**Likelihood**: Low (ADR-001 notes this is a defensive concern; normal lifecycle has exactly one `cycle_start` per `cycle_id`)
**Impact**: Goal from a previous or duplicate cycle start row contaminates the resumed session.

**Test Scenarios**:
1. Insert two `cycle_start` rows for the same `cycle_id` with different goals (simulated corrupted state). Assert `get_cycle_start_goal` returns the first row's goal (`LIMIT 1` semantics).
2. Verify `LIMIT 1` is present in the query at code review.

**Coverage Requirement**: One defensive test for the multi-row edge case.

---

### R-11: `CONTEXT_GET_INSTRUCTION` Header Breaks Existing `format_index_table` Tests

**Severity**: Med
**Likelihood**: High (ADR-006 explicitly warns: "Existing tests that assert `format_index_table` output will need updating")
**Impact**: Any test asserting `format_index_table` output by row-count, line position, or full-string match will fail because the instruction header adds one line before the table. Tests that do `assert_eq!(output, expected_table)` or `assert!(output.starts_with("| ID |"))` will fail silently or with misleading diffs.

**Test Scenarios**:
1. Identify all test assertions on `format_index_table` output. Update each to either: (a) strip the first non-empty line before asserting table rows, or (b) use a test helper that normalizes the header away for row-content assertions.
2. New test: call `format_index_table` with one or more entries, assert output starts with `CONTEXT_GET_INSTRUCTION` constant text, assert the constant does not appear again within the table rows (once-only, AC-18).
3. Verify header appears in MCP `context_briefing` response output (AC-18 via `handle_briefing` path).
4. Verify header appears in UDS `CompactPayload` injection output (AC-18 via `handle_compact_payload` path).

**Coverage Requirement**: Every existing test touching `format_index_table` output must be audited and updated. The once-only header assertion (scenario 2) is required. Recommend a shared `strip_briefing_header(s: &str) -> &str` test helper rather than per-test magic string removal.

---

### R-12: SubagentStart Goal-Present Branch — New `IndexBriefingService` Integration Surface Untested

**Severity**: High
**Likelihood**: Med
**Impact**: The SubagentStart goal-present branch is architecturally distinct from both the old `ContextSearch` path and the `CompactPayload` path. It calls `IndexBriefingService::index(query: &goal, session_state, k: 20)` — a new call site for this path. If `IndexBriefingService::index` is not reachable from the SubagentStart arm (e.g., the service is not wired into `dispatch_request`), or if `session_registry.get_state(session_id)` fails at SubagentStart time (OQ-03: timing of hook fire relative to session registration), the branch silently produces no injection.

**Test Scenarios**:
1. Integration test: with an active session (`current_goal = Some("goal text")`), fire a `SubagentStart` hook event, assert `IndexBriefingService::index` is invoked and the response contains a ranked-index payload (not a `ContextSearch` response or empty payload).
2. Verify `session_registry.get_state(session_id)` is reachable in the SubagentStart arm before a `CYCLE_START_EVENT` has been processed for the same session — or document and test the ordering requirement (OQ-03 resolution).
3. Goal-absent SubagentStart: assert the old `ContextSearch` path still runs unchanged (regression guard for the goal-absent branch).

**Coverage Requirement**: Both branches of the SubagentStart fork (goal-present → `IndexBriefingService`; goal-absent → `ContextSearch`/transcript) must be exercised by integration tests. A unit test alone is insufficient — the wiring from `dispatch_request` to `IndexBriefingService` must be verified end-to-end.

---

### R-13: UDS Truncate-Then-Overwrite Retry Sequence Produces Incorrect Final Goal State

**Severity**: Med
**Likelihood**: Med
**Impact**: The ADR-005 design relies on "last-writer-wins" to correct a truncated UDS write: the MCP path rejects the oversized goal, the agent corrects it, retries via MCP, which fires a new UDS write that overwrites the truncated value. If the second UDS write does not overwrite the first (e.g., because the insert is not an UPSERT or the `cycle_events` row is immutable after first write), the session is left with a truncated goal even after the corrected retry.

**Test Scenarios**:
1. Write a truncated goal to `cycle_events` via the UDS path (simulate oversized input). Then write a full corrected goal for the same `cycle_id` via a second UDS write. Assert `get_cycle_start_goal` returns the corrected (second) goal value — not the truncated first value.
2. Verify the SQL `INSERT` or `UPSERT` semantics for `insert_cycle_event` support overwriting an existing `cycle_start` row's `goal` column on the same `cycle_id`. If the insert uses `INSERT OR IGNORE` semantics, this scenario will silently retain the truncated value.
3. Assert `SessionState.current_goal` is updated to the corrected value after the second UDS write (in-memory consistency with DB).

**Coverage Requirement**: The retry-overwrite sequence must be tested as an integrated scenario, not just the individual UDS-truncate and UDS-write steps in isolation. The DB write semantics (INSERT vs. INSERT OR REPLACE vs. UPSERT) must be verified at code review.

---

### R-14: Old Binary Connecting to v16 Schema

**Severity**: Med
**Likelihood**: Low (standard constraint; old binaries blocked by schema version gate)
**Impact**: Unhelpful error message when a pre-col-025 binary attempts to open the v16 database.

**Test Scenarios**:
1. Verify the schema version gate in `db.rs` returns a clear `DatabaseVersionMismatch` or equivalent error (not a generic SQLite error) when `CURRENT_SCHEMA_VERSION < stored_version`.
2. This is primarily a code-review concern; no new automated test needed unless the gate logic itself changed.

**Coverage Requirement**: Existing schema version gate behavior covers this.

---

## Integration Risks

**Goal value flow across MCP → ImplantEvent → UDS listener**: The goal travels from `CycleParams.goal` (MCP wire) into an `ImplantEvent` payload, then is extracted by `handle_cycle_event` in the UDS listener. A serialization mismatch (field name change, optional field dropped from payload struct) would cause the UDS listener to receive `None` even when the caller supplied a goal. This path must be covered by an integration test starting from the MCP tool call that asserts `state.current_goal` is set correctly after the full round-trip.

**`derive_briefing_query` shared between MCP and UDS paths**: ADR-002 resolves SR-06 by architecture (single function), but if the MCP handler and UDS handler pass different `SessionState` representations to `derive_briefing_query`, goal will be present on one path and absent on the other. Each path needs at least one integration test exercising step 2 independently: AC-04 for MCP, AC-07 for UDS CompactPayload.

**SubagentStart goal-present branch wiring (new in revised design)**: The goal-present branch of SubagentStart calls `IndexBriefingService::index` — a function not previously called from this code path. If `IndexBriefingService` is not available in `dispatch_request`'s calling context (not passed in, not available via service locator), the branch will fail at the wiring point. R-12 covers this; the integration test must confirm the service is reachable from the SubagentStart arm.

**`get_cycle_start_goal` vs. `SessionState.current_goal` consistency on resume**: If session registration fires before the `CYCLE_START_EVENT` write has been committed (race between MCP start and session registration on resume), the resume path may find no row. The architecture resolves this by requiring session resume to occur only after `CYCLE_START_EVENT` has been processed. The test suite must include a resume test where the `cycle_events` row is confirmed to exist before `SessionRegister` fires.

---

## Edge Cases

| Edge Case | Risk ID | Test Required |
|-----------|---------|---------------|
| `goal` field present in `CycleParams` but `action != "start"` (phase_end or stop) | FR-01 | Assert goal param is ignored silently; no DB write of goal on those rows |
| `goal = ""` (empty string, not null) on MCP path | R-06 | Assert normalized to `None`; step 3 topic-ID fallback used (FR-11 / AC-17) |
| `goal` consisting entirely of whitespace on MCP path | R-06 | Assert normalized to `None`; blank strings must not reach storage (AC-17) |
| Whitespace-only goal on UDS path | R-07 | UDS does not normalize whitespace (only truncates); stored verbatim if within byte limit |
| UTF-8 multi-byte goal at exact `MAX_GOAL_BYTES` boundary | R-07 | Char-boundary-safe truncation unit test |
| Goal exactly `MAX_GOAL_BYTES` bytes — accepted, not truncated | R-07 | Boundary passes on both MCP and UDS paths |
| `cycle_id` not yet present in `cycle_events` at resume time | R-03 | `get_cycle_start_goal` returns `Ok(None)`, session proceeds |
| `goal` column absent from `cycle_events` (pre-v16 DB in test) | R-03 | `get_cycle_start_goal` returns `None` on SQL error (defensive per ADR-004) |
| `context_cycle(stop)` or `context_cycle(phase_end)` carrying `goal` from a buggy client | FR-01 | Goal is extracted but not written to stop/phase-end rows |
| Concurrent `SessionRegister` for the same `session_id` | R-03 | `set_current_goal` must be idempotent under concurrent calls |
| `current_goal = Some("")` (empty string stored — edge case if normalization skipped) | R-04 | SubagentStart non-empty check prevents routing to `IndexBriefingService`; falls through |
| Second UDS write overwrites first (corrected retry after MCP rejection) | R-13 | Retry-overwrite sequence test; verify last-writer-wins semantics |

---

## Security Risks

**Untrusted input surface**: The `goal` parameter on `context_cycle(start)` is caller-provided text that flows into:
1. A SQLite `TEXT` column via parameterized bind — injection risk mitigated by rusqlite positional binds.
2. `SessionState.current_goal` in-memory (`String` clone — no execution surface).
3. `derive_briefing_query` output used as a vector search query — the goal text becomes the embedding input on both the MCP briefing path and, via `IndexBriefingService`, the SubagentStart injection path.

**Parameterized SQL binding** (R-08 above): The bind is positional. A transposition error corrupts the wrong column, not enables injection. The architecture uses rusqlite positional binds throughout; safe as long as bind positions are audited at code review.

**Embedding pipeline influence via goal text**: A crafted goal string (adversarial text targeting the embedding model) could skew retrieval ranking for all agents in the feature cycle. The blast radius is limited to retrieval quality within that cycle — no external data exfiltration path exists. Mitigation: `MAX_GOAL_BYTES = 1024` constrains the input surface on both MCP and UDS paths.

**UDS truncation cannot be prevented by callers**: The hook path is fire-and-forget; if a UDS-originated goal exceeds `MAX_GOAL_BYTES`, it is truncated with a warn log but the session proceeds. A malicious hook payload cannot exceed `MAX_GOAL_BYTES` in the stored value; the truncation is a bound. However, the truncated value could be semantically misleading (mid-sentence). This is an accepted limitation per ADR-005.

**Memory amplification resolved (ADR-005)**: With `MAX_GOAL_BYTES = 1024`, the worst-case per-session in-memory clone is 1 KB. The multi-megabyte scenario is fully mitigated. No further security concern on this surface.

---

## Failure Modes

| Failure | Expected Behavior | Detectable By |
|---------|------------------|---------------|
| DB error during `get_cycle_start_goal` on resume | `current_goal = None`, `tracing::warn!` emitted, session registration succeeds (HookResponse::Ack returned) | Log monitoring; AC-15 test |
| No `cycle_start` row for cycle_id (pre-v16 or missing) | `current_goal = None`, session registration succeeds | AC-14 test |
| Goal exceeds `MAX_GOAL_BYTES` bytes (MCP path) | `CallToolResult::error(...)` returned with actionable message; no DB write | AC-13a test |
| Goal exceeds `MAX_GOAL_BYTES` bytes (UDS path) | Truncated at char boundary, `tracing::warn!` emitted, truncated value written | R-07 test |
| Corrected retry after MCP rejection fires second UDS write | Second UDS write overwrites truncated first value; final state = corrected goal | R-13 test |
| `insert_cycle_event` called with wrong parameter arity | Compile error (static); caught at build | Cargo build |
| Migration applied to already-v16 database | No-op (idempotency guard via `pragma_table_info`); no error | AC-09 idempotency scenario |
| `format_index_table` called — existing test asserts raw output | Test failure due to `CONTEXT_GET_INSTRUCTION` header line prepended | R-11 test audit |
| SubagentStart fires with goal-present session; `IndexBriefingService` not wired | Silent no-injection or panic; agents receive no context | R-12 integration test |
| Old binary connecting to v16 database | Schema version gate returns clear error; clear log message | Existing gate behavior |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: Schema migration test cascade | R-02 | ARCHITECTURE.md §Migration Test Cascade identifies three files to audit. NFR-06 and AC-16 mandate updates. |
| SR-02: Unbounded goal text | R-07, R-13 | ADR-005 settles `MAX_GOAL_BYTES = 1024` (single constant, same limit for MCP and UDS). MCP hard-rejects; UDS truncates at char boundary. R-13 covers the retry-overwrite correctness of the UDS truncation path. |
| SR-03: SubagentStart precedence not tested in isolation | R-04, R-12 | ADR-003 mandates explicit goal-present branch routing to `IndexBriefingService`. R-04 covers branch-point correctness (all five precedence cases). R-12 covers the new IndexBriefingService wiring on this path. |
| SR-04: sessions.keywords column boundary | — | Accepted. ARCHITECTURE.md §Columns explicitly out of scope names the column. No architecture-level risk materialized. |
| SR-05: Session resume DB failure contract | R-03 | ADR-004 specifies `unwrap_or_else` degradation to `None` with `tracing::warn!`. AC-14 and AC-15 cover the failure sub-cases. |
| SR-06: `derive_briefing_query` shared path divergence | — | Resolved by architecture (ADR-002). Single shared function eliminates divergence. AC-04 (MCP path) and AC-07 (UDS CompactPayload path) verify the function is reached on both paths. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical (High×High) | 0 | — |
| High (High×Med or Med×High) | 5 (R-02, R-04, R-05, R-06, R-11, R-12*) | ≥ 3 scenarios each |
| Medium (High×Low elevated or Med×Med) | 5 (R-01, R-03, R-07†, R-08, R-13) | ≥ 2 scenarios each |
| Low (Low×Any or Med×Low) | 3 (R-09, R-10, R-14) | ≥ 1 scenario each |

*R-12 is High×Med: new integration surface introduced by ADR-003 revision.
†R-07 is High×Low but elevated to Medium priority because a UTF-8 boundary panic in the UDS listener terminates the server process.

**Non-negotiable test scenarios** (gate-3c check, per lesson #2758):

1. `migration_v15_to_v16.rs` with idempotency scenario (R-02 / AC-09).
2. SubagentStart goal-present → `IndexBriefingService` called with goal as query; transcript path NOT taken (R-04 / AC-08 / ADR-003).
3. SubagentStart goal-present, `prompt_snippet` non-empty → goal still wins; `IndexBriefingService` called (R-04 / AC-12 / ADR-003).
4. SubagentStart goal-absent → existing `ContextSearch`/transcript path runs unchanged (R-12 regression guard).
5. UTF-8 char-boundary truncation at `MAX_GOAL_BYTES` boundary (R-07).
6. Full column-value assertion on `insert_cycle_event` round-trip (R-08).
7. DB error on resume → `None` + warn log + registration succeeds (R-03 / AC-15).
8. `format_index_table` output starts with `CONTEXT_GET_INSTRUCTION` constant exactly once (R-11 / AC-18).
9. UDS truncate-then-overwrite retry: second write overwrites first (R-13).

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found #1203 (cascading rework), #1204 (test plan cross-reference), #2758 (gate-3c non-negotiable test names), #2800 (circuit breaker testability).
- Queried: `/uni-knowledge-search` for "SQLite migration schema version cascade" — found #2933 (schema version cascade pattern, directly informs R-02), #378 (migration tests on old-schema DBs).
- Queried: `/uni-knowledge-search` for "risk pattern SessionState session resume" — found #3180 (SessionState field additions require helper updates, informs R-06), #3301 (graceful degradation, informs R-03), #3027 (phase snapshot / race condition pattern).
- Queried: `/uni-knowledge-search` for "SubagentStart IndexBriefingService hook injection" — found #3398 (ADR-003 col-025: SubagentStart goal-first, IndexBriefingService routing), #3230 (SubagentStart routing pattern), #3297 (session_id routing gotcha).
- Stored: nothing novel to store — R-02 (schema cascade) already pattern #2933; R-06 (SessionState helper updates) already pattern #3180; R-11 (format_index_table header breakage pattern) is col-025-specific; no cross-feature pattern visible yet.
