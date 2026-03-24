# Risk-Based Test Strategy: col-025 — Feature Goal Signal

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `insert_cycle_event` signature change breaks undetected call sites | High | Low | Medium |
| R-02 | Migration v15→v16 test cascade breaks CI on unmodified assertion files | Med | High | High |
| R-03 | Session resume sets `current_goal = None` silently when DB returns error, masking persistent faults | Med | Med | Medium |
| R-04 | SubagentStart precedence inverted: `current_goal` overrides non-empty `prompt_snippet` | High | Med | High |
| R-05 | `synthesize_from_session` removal of topic-signal synthesis breaks existing `derive_briefing_query` tests that relied on that path | Med | High | High |
| R-06 | `SessionState` struct literals in test helpers not updated for `current_goal` field, causing compile errors | Med | High | High |
| R-07 | UDS byte-limit truncation produces a non-UTF-8-boundary slice, causing a panic | High | Low | Medium |
| R-08 | Goal written to wrong column binding position (`insert_cycle_event` param 8) produces corrupted rows | High | Low | Medium |
| R-09 | `context_cycle(start)` with no `goal` subtly changes downstream behavior in briefing or injection paths | Med | Low | Low |
| R-10 | Session resume query returns first `cycle_start` row of wrong cycle when index collision exists | Low | Low | Low |
| R-11 | Goal text exceeding 2 048 bytes accepted by MCP tool layer (off-by-one in byte check) | Med | Low | Low |
| R-12 | Old binaries connecting to v16 schema fail at runtime with unhelpful error | Med | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: `insert_cycle_event` Signature Change Breaks Undetected Call Sites

**Severity**: High
**Likelihood**: Low
**Impact**: Compile failure or, if the call site exists in a test-only path, silent incorrect behavior in DB rows (NULL goal written where text expected, or vice versa).

**Test Scenarios**:
1. Grep all files in `crates/` for `insert_cycle_event` before delivery — verify exactly one call site exists in `listener.rs`. If more exist, each must receive the `goal` parameter at the correct bind position.
2. Compile the full workspace without `#[allow(unused)]` suppression; no call-site arity errors.
3. Integration test: after calling `context_cycle(start, goal: "test goal")`, read the raw `cycle_events` row and assert the `goal` column equals `"test goal"` (not NULL, not a different column's value).

**Coverage Requirement**: DB round-trip test confirming the correct column binding (param position 8). Architecture open question OQ-01 confirmed one call site; delivery must verify this before touching the signature.

---

### R-02: Migration Test Cascade Breaks CI

**Severity**: Med
**Likelihood**: High (pattern #2933 confirms this is a recurring CI trap)
**Impact**: CI red on unrelated test files asserting `schema_version = 15`; blocks delivery merge.

**Test Scenarios**:
1. Pre-delivery audit: identify every test file asserting `CURRENT_SCHEMA_VERSION` or a literal `15`. Files identified in ARCHITECTURE.md: `migration_v14_to_v15.rs`, `sqlite_parity.rs`, `sqlite_parity_specialized.rs`.
2. New migration test `migration_v15_to_v16.rs`: apply v16 migration to a v15 database, assert `pragma_table_info(cycle_events)` contains `goal` column, assert existing rows have `goal IS NULL`, re-run migration (idempotency), assert no error.
3. Assert `CURRENT_SCHEMA_VERSION = 16` constant in `db.rs` after delivery.

**Coverage Requirement**: Every file asserting a version ≤ 15 must be updated (AC-16). The new `migration_v15_to_v16.rs` test is a non-negotiable addition.

---

### R-03: Resume DB Error Silently Degrades Without Observable Warning

**Severity**: Med
**Likelihood**: Med
**Impact**: Agents operating after a server restart receive topic-ID fallback instead of goal-driven briefing with no visible indication; hard to diagnose in production.

**Test Scenarios**:
1. Inject a DB error on `get_cycle_start_goal` during session registration (test double or deliberate schema corruption in test DB). Assert `state.current_goal = None`, assert session registration returns `HookResponse::Ack`, assert `tracing::warn!` with `"col-025: goal resume lookup failed"` was emitted (AC-15).
2. Resume path with pre-v16 cycle (no matching `cycle_start` row): assert `state.current_goal = None`, session succeeds (AC-14).
3. Resume path with v16 cycle where `goal = NULL` (caller omitted goal): assert `state.current_goal = None`, session succeeds.

**Coverage Requirement**: All three `get_cycle_start_goal` return variants must be covered: `Ok(Some(goal))`, `Ok(None)`, and `Err(...)` (ADR-004 decision). Warn log emission must be asserted, not just tolerated.

---

### R-04: SubagentStart Precedence Inverted — Goal Overrides Non-Empty prompt_snippet

**Severity**: High
**Likelihood**: Med (SR-03; explicit branch in code is not protected by `derive_briefing_query` shared logic)
**Impact**: When a spawning agent provides a specific prompt, the system silently substitutes the feature goal instead, degrading injection quality for all SubagentStart events in cycles with a stored goal.

**Test Scenarios**:
1. `prompt_snippet = "non-empty task"`, `current_goal = Some("feature goal")` → assert query = `"non-empty task"`, NOT `"feature goal"` (AC-12 / SR-03 guard).
2. `prompt_snippet = ""`, `current_goal = Some("feature goal")` → assert query = `"feature goal"` (AC-08).
3. `prompt_snippet = ""`, `current_goal = None` → assert fallback to `RecordEvent` or topic (AC-06 parallel).
4. `prompt_snippet = ""`, `current_goal = Some("")` (empty string goal stored) → assert falls through to topic fallback (goal non-empty check).

**Coverage Requirement**: All four precedence branches must be exercised. The inverted-precedence case (scenario 1) is the critical regression guard — its absence at gate 3c is a known failure mode (lesson #2758).

---

### R-05: Removal of Topic-Signal Synthesis Breaks Existing derive_briefing_query Tests

**Severity**: Med
**Likelihood**: High (ADR-002 explicitly notes "existing `derive_briefing_query` tests... will need updating")
**Impact**: CI failures on tests that set up `topic_signals` and expect `synthesize_from_session` to return the synthesized string. These are not behavioral regressions — they are test-contracts that no longer reflect the new step-2 semantics.

**Test Scenarios**:
1. Identify existing `synthesize_from_session` tests that assert the `"{feature_cycle} {signals}"` format; update them to assert step 2 now returns `current_goal` when `Some`.
2. `derive_briefing_query` called with `task = None`, `current_goal = Some("g")`, populated `topic_signals` → assert step 2 wins with `"g"` (goal, not signal synthesis) (AC-04).
3. `derive_briefing_query` called with `task = None`, `current_goal = None`, populated `topic_signals` → assert step 3 topic-ID string returned (signals no longer affect step 2) (AC-06).

**Coverage Requirement**: Step 2 must be covered with goal `Some` and `None`. The old topic-signal synthesis path must have no remaining test asserting it (removing dead coverage prevents future confusion).

---

### R-06: SessionState Struct Literals Not Updated in Test Helpers

**Severity**: Med
**Likelihood**: High (pattern #3180 confirms `SessionState` field additions require updating `make_state_with_rework` and similar helpers)
**Impact**: Compile failure across any test file that constructs `SessionState { .. }` without using struct update syntax; blocks the entire test suite.

**Test Scenarios**:
1. Pre-delivery: grep `crates/unimatrix-server/src/` for `SessionState {` and `make_session_state` or `make_state_with_rework` to enumerate all construction sites.
2. Verify each site either uses `..Default::default()` syntax or is updated to include `current_goal: None`.
3. Full `cargo test` passes after field addition with no missing-field compile errors.

**Coverage Requirement**: This is a compile-time risk, not a runtime risk. The coverage requirement is zero failing tests after adding the field. At least one test helper must construct `SessionState` with `current_goal: Some("test goal")` to confirm the new field is exercised.

---

### R-07: UDS Byte-Limit Truncation Produces Non-UTF-8-Boundary Slice → Panic

**Severity**: High
**Likelihood**: Low (ADR-005 calls out the requirement for char-boundary-safe truncation)
**Impact**: Panic in the UDS listener on a `CYCLE_START_EVENT` with a multi-byte Unicode goal exceeding 4 096 bytes. The UDS path cannot return an error; it truncates silently. A panic here terminates the server process.

**Test Scenarios**:
1. Supply a goal string that is exactly 4 097 bytes but whose 4 096th byte falls in the middle of a multi-byte UTF-8 character (e.g., a 3-byte CJK character straddling byte 4095–4097). Assert the UDS handler does not panic and the stored goal is valid UTF-8 ≤ 4 096 bytes.
2. Supply a goal that is exactly `MAX_GOAL_BYTES` bytes of valid ASCII. Assert it is stored verbatim (no truncation).
3. MCP path: supply a goal of 2 049 bytes. Assert `CallToolResult::error(...)` is returned and no `cycle_events` row is written (AC-13). Note: spec says 2 048 for MCP, 4 096 for UDS — both limits need coverage.

**Coverage Requirement**: The char-boundary safe truncation path must have a dedicated unit test with a multi-byte character at the boundary. This is the one code path that can cause a panic.

---

### R-08: Goal Written to Wrong Column Binding Position

**Severity**: High
**Likelihood**: Low
**Impact**: The `goal` value populates the wrong column in the row (e.g., `outcome` or `next_phase`), corrupting the event log silently. No error is returned; the database accepts the write.

**Test Scenarios**:
1. DB round-trip test: write a `cycle_start` event with a known goal string; read back the full row and assert each column (`event_type`, `phase`, `outcome`, `next_phase`, `goal`) contains its expected value. This confirms the `goal` bind is at position 8 and no other column is displaced.
2. Write with `goal = None`; assert `goal IS NULL` in the row and no other column is affected.

**Coverage Requirement**: A full column-value assertion on the inserted row (not just the goal column in isolation) is required to detect binding transposition.

---

### R-09: No-Goal Path Subtly Changes Downstream Behavior

**Severity**: Med
**Likelihood**: Low
**Impact**: Callers that never provide a goal experience changed briefing or injection behavior after delivery, violating NFR-02 (backward compatibility).

**Test Scenarios**:
1. Run the full existing `context_cycle`, `context_briefing`, and `context_cycle_review` test suite on the feature branch without modification — all must pass (AC-10, NFR-02).
2. End-to-end test: start a cycle with no goal, call `context_briefing` with no task, assert the query used is the topic-ID string (step 3), identical to pre-col-025 behavior (AC-06).

**Coverage Requirement**: AC-10 is the primary gate for this risk. No existing test may be modified to accommodate no-goal path behavior changes.

---

### R-10: Resume Query Returns Wrong Row When Multiple cycle_start Rows Exist

**Severity**: Low
**Likelihood**: Low (ADR-001 notes this is a defensive concern; normal lifecycle has exactly one `cycle_start` per `cycle_id`)
**Impact**: Goal from a previous cycle's start event contaminates the resumed session.

**Test Scenarios**:
1. Insert two `cycle_start` rows for the same `cycle_id` with different goals (simulate a corrupted state). Assert `get_cycle_start_goal` returns the first row's goal (LIMIT 1 semantics).
2. Verify `LIMIT 1` is present in the query at code review.

**Coverage Requirement**: One defensive test for the multi-row edge case. This is a low-priority scenario that guards against data corruption rather than a realistic usage path.

---

### R-11: Goal Byte-Length Off-By-One Accepts 2 049-Byte Input

**Severity**: Med
**Likelihood**: Low
**Impact**: Goals marginally over the limit are stored and loaded into `SessionState`, undermining the guard.

**Test Scenarios**:
1. Supply exactly 2 048 bytes — assert accepted (boundary passes).
2. Supply exactly 2 049 bytes — assert rejected with structured error (AC-13).
3. Supply 0 bytes (empty string goal) — assert behavior equivalent to `goal = None` (no empty-string goal stored as if it were meaningful).

**Coverage Requirement**: The boundary conditions at `MAX_GOAL_BYTES` (both sides) and empty-string input must be covered.

---

### R-12: Old Binary Connecting to v16 Schema

**Severity**: Med
**Likelihood**: Low (standard constraint; old binaries blocked by schema version gate)
**Impact**: Unhelpful error message when a pre-col-025 binary attempts to open the v16 database.

**Test Scenarios**:
1. Verify the schema version gate in `db.rs` returns a clear `DatabaseVersionMismatch` or equivalent error (not a generic SQLite error) when `CURRENT_SCHEMA_VERSION < stored_version`.
2. This is primarily a code-review and manual test concern; no automated test regression risk.

**Coverage Requirement**: Existing schema version gate behavior covers this. No new test needed unless the gate logic itself changed.

---

## Integration Risks

**Goal value flow across MCP → ImplantEvent → UDS listener**: The goal travels from `CycleParams.goal` (MCP wire) into an `ImplantEvent` payload, then is extracted by `handle_cycle_event` in the UDS listener. A serialization mismatch (e.g., field name change, optional field missing from payload struct) would cause the UDS listener to receive `None` even when the caller supplied a goal. This path must be covered by an integration test that starts from the MCP tool call and asserts `state.current_goal` is set correctly after the full round-trip.

**`derive_briefing_query` shared between MCP and UDS paths**: ADR-002 resolves SR-06 by architecture (single function), but if the MCP handler and UDS handler pass different `SessionState` representations to `derive_briefing_query`, goal will be present on one path and absent on the other. Each path needs at least one integration test exercising step 2 independently (AC-04 for MCP, AC-07 for UDS CompactPayload).

**`get_cycle_start_goal` vs. `SessionState.current_goal` consistency**: If a session is registered before the `cycle_start` event is written (race condition between MCP start and session registration on resume), the resume path might find no `cycle_start` row. The architecture resolves this by requiring session resume to occur only after `CYCLE_START_EVENT` has been processed, but the test suite should include a resume test where the `cycle_events` row exists before `SessionRegister` fires.

---

## Edge Cases

| Edge Case | Risk ID | Test Required |
|-----------|---------|---------------|
| `goal` field present in `CycleParams` but `action != "start"` (phase_end or stop) | FR-01 | Assert goal param is ignored silently; no DB write |
| `goal = ""` (empty string, not null) | R-11 | Assert treated as `None`; step 3 topic-ID fallback used |
| Goal containing only whitespace | R-11 | Assert treated as `None` or stored verbatim (spec must clarify; NFR-05 says verbatim) |
| UTF-8 multi-byte goal at exact byte boundary | R-07 | Char-boundary-safe truncation test (UDS path) |
| `cycle_id` not yet present in `cycle_events` at resume time | R-03 | `get_cycle_start_goal` returns `Ok(None)`, session proceeds |
| `goal` column absent from `cycle_events` (pre-v16 DB in test) | R-03 / R-08 | `get_cycle_start_goal` returns `None` on SQL error (defensive) |
| `context_cycle(stop)` or `context_cycle(phase_end)` carrying a `goal` param from a buggy client | FR-01 | Assert goal is extracted but not written to stop/phase-end rows |
| Concurrent `SessionRegister` for the same `session_id` (race) | R-03 | `set_current_goal` must be idempotent under concurrent calls |

---

## Security Risks

**Untrusted input surface**: The `goal` parameter on `context_cycle(start)` is caller-provided text that flows into:
1. A SQLite `TEXT` column (via parameterized bind — injection risk is mitigated).
2. `SessionState.current_goal` in-memory (String clone — no execution surface).
3. `derive_briefing_query` output used as a vector search query — the goal text becomes the embedding input.

**Parameterized SQL binding** (R-08 above): The bind is positional. A transposition error would corrupt the wrong column, not enable injection. The architecture uses rusqlite positional binds throughout; this pattern is consistent with the codebase and safe as long as bind positions are audited.

**Embedding pipeline injection via goal text**: A crafted goal string (e.g., adversarial text targeting the embedding model) could influence retrieval ranking for all agents in the feature cycle. The blast radius is limited to retrieval quality within that cycle. No external data exfiltration path exists. Mitigation: the 2 048-byte MCP limit and 4 096-byte UDS limit constrain the input surface.

**Memory amplification (SR-02 resolved by ADR-005)**: With `MAX_GOAL_BYTES = 4 096`, the worst-case per-session memory overhead is 4 KB. Across a bounded number of concurrent sessions this is negligible. The multi-megabyte scenario is fully mitigated.

**UDS path (fire-and-forget)**: The UDS listener cannot return a rejection error for oversized goals; it truncates with a warn log. This is a known limitation documented in ADR-005. The truncation must be char-boundary safe (R-07) to avoid panic.

---

## Failure Modes

| Failure | Expected Behavior | Detectable By |
|---------|------------------|---------------|
| DB error during `get_cycle_start_goal` on resume | `current_goal = None`, `tracing::warn!` emitted, session registration succeeds | Log monitoring; AC-15 test |
| No `cycle_start` row for cycle_id (pre-v16 or deleted) | `current_goal = None`, session registration succeeds | AC-14 test |
| Goal exceeds 2 048 bytes (MCP path) | `CallToolResult::error(...)` returned; no DB write | AC-13 test |
| Goal exceeds 4 096 bytes (UDS path) | Truncated to `MAX_GOAL_BYTES` at char boundary, `tracing::warn!` | R-07 test |
| `insert_cycle_event` called with wrong parameter arity | Compile error (static) | Cargo build |
| Migration applied to already-v16 database | No-op (idempotency guard); no error | AC-09 idempotency scenario |
| Old binary connecting to v16 database | Schema version gate returns error; clear log message | Existing gate behavior |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: Schema migration test cascade | R-02 | Architecture explicitly identifies three test files to audit (ARCHITECTURE.md §Migration Test Cascade). NFR-06 and AC-16 mandate updates. |
| SR-02: Unbounded goal text | R-07, R-11 | ADR-005 establishes `MAX_GOAL_BYTES = 4096` (UDS) and 2 048 (MCP). Tool-layer rejection for MCP; char-boundary-safe truncation for UDS. |
| SR-03: SubagentStart precedence not tested in isolation | R-04 | ADR-003 mandates explicit branch with three dedicated tests (AC-08, AC-12, fallback). Inversion scenario (R-04) is covered by AC-12. |
| SR-04: sessions.keywords column boundary | — | Accepted. ARCHITECTURE.md §Columns explicitly out of scope names the column. No architecture-level risk materialized. |
| SR-05: Session resume DB failure contract | R-03 | ADR-004 specifies `unwrap_or_else` degradation to `None` with `tracing::warn!`. AC-14 and AC-15 cover the two failure sub-cases. |
| SR-06: derive_briefing_query shared path divergence | — | Resolved by architecture (ADR-002). Single shared function eliminates divergence. Separate integration tests for MCP (AC-04) and UDS (AC-07) verify the shared function is reached on both paths. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical (High×High) | 0 | — |
| High (High×Med or Med×High) | 5 (R-02, R-04, R-05, R-06, R-07*) | ≥ 3 scenarios each |
| Medium (Med×Med or High×Low) | 5 (R-01, R-03, R-08, R-09, R-11) | ≥ 2 scenarios each |
| Low (Low×Any or Med×Low) | 2 (R-10, R-12) | ≥ 1 scenario each |

*R-07 is High×Low but elevated to the High tier because a panic in the UDS listener terminates the server process.

**Non-negotiable tests** (gate-3c check, per lesson #2758):
- `migration_v15_to_v16.rs` with idempotency scenario (R-02 / AC-09)
- SubagentStart inversion guard: `prompt_snippet` non-empty + goal set → `prompt_snippet` wins (R-04 / AC-12)
- UTF-8 char-boundary truncation at 4 096-byte boundary (R-07)
- Full column-value assertion on `insert_cycle_event` round-trip (R-08)
- DB error on resume → `None` + warn log + registration succeeds (R-03 / AC-15)

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found #1203 (cascading rework), #1204 (test plan cross-reference), #2758 (gate-3c non-negotiable test names), #2800 (circuit breaker testability).
- Queried: `/uni-knowledge-search` for "SQLite migration schema version cascade" — found #2933 (schema version cascade pattern, directly informs R-02), #378 (migration tests on old-schema DBs), #681/#370 (create-new-then-swap, not applicable here).
- Queried: `/uni-knowledge-search` for "risk pattern SessionState session resume" — found #3180 (SessionState field additions require helper updates, directly informs R-06), #3301 (graceful degradation, informs R-03), #3027 (phase snapshot / race condition pattern).
- Queried: `/uni-knowledge-search` for "SubagentStart hook session_id lookup" — found #3297 (session_id routing gotcha), #3230 (SubagentStart routing pattern), #3251 (hookSpecificOutput envelope).
- Stored: nothing novel to store — R-02 (schema cascade) already pattern #2933; R-06 (SessionState helper updates) already pattern #3180; no new cross-feature pattern visible from this single feature.
