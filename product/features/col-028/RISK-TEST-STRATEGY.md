# Risk-Based Test Strategy: col-028

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | D-01 dedup collision: briefing (weight=0) burns the dedup slot, silencing a subsequent context_get (weight=2) access increment | High | High | Critical |
| R-02 | Positional column index drift: analytics.rs INSERT, both scan_query_log_* SELECTs, and row_to_query_log diverge — silent runtime data corruption, not a compile error | High | Med | Critical |
| R-03 | Phase snapshot race: current_phase_for_session called after an await in any of the four handlers, attributing wrong phase | High | Med | Critical |
| R-04 | Phase snapshot duplicated at context_search: two separate get_state calls for UsageContext.current_phase vs QueryLogRecord.phase could diverge under concurrent phase-end event | Med | Med | High |
| R-05 | SR-02 schema version cascade: migration_v15_to_v16.rs and server.rs assertions still claim version 16 after bump to 17 — test suite fails at gate | Med | High | High |
| R-06 | SR-03 UDS compile break: uds/listener.rs:1324 QueryLogRecord::new not updated with phase: None argument — workspace fails to compile | Med | High | High |
| R-07 | context_get weight regression: weight stays at 1 instead of corrected 2 — highest-signal read event underweighted in confidence scoring | Med | Med | High |
| R-08 | context_briefing weight not corrected to 0: briefing continues incrementing access_count, bloating confidence scores for unread entries | Med | Med | High |
| R-09 | confirmed_entries field missing from make_state_with_rework or test helpers — compile errors in all existing SessionState tests | Med | High | High |
| R-10 | Phase not captured in query_log for context_search calls: UsageContext.current_phase populated but query_log row written with phase=NULL | Med | Med | Medium |
| R-11 | Migration idempotency failure: re-running v16→v17 on an already-migrated database fails due to missing pragma_table_info pre-check | Med | Low | Medium |
| R-12 | Pre-existing query_log rows deserialized incorrectly after migration: NULL phase column causes deserializer panic instead of None | Med | Low | Medium |
| R-13 | confirmed_entries cardinality error: multi-target context_lookup incorrectly populates confirmed_entries (response-side vs request-side cardinality) | Low | Med | Medium |
| R-14 | context_lookup weight changed from 2 inadvertently during other edits | Low | Low | Low |
| R-15 | UsageContext.current_phase doc comment not updated (ADR-006): stale comment misleads future implementors on next feature touching UsageContext | Low | Low | Low |
| R-16 | SR-07 future bypass: record_mcp_usage called with AccessSource::Briefing by future refactor, bypassing D-01 guard | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: D-01 Dedup Collision (briefing burns dedup slot)

**Severity**: High
**Likelihood**: High
**Impact**: The highest-signal read event (context_get, weight=2) produces zero access_count increment after any briefing event on the same entry. Confidence scoring permanently underweights entries the agent deliberately retrieved. Pattern #3503 and #3510 confirm this collision is real and requires an explicit early-return guard.

**Test Scenarios**:
1. **(AC-07 positive)** Register a session. Call record_briefing_usage with entry X and weight=0. Assert DedupState.access_counted does NOT contain (agent_id, X). Then call record_mcp_usage with entry X and weight=2. Assert access_count for X increments by 2.
2. **(AC-07 negative — critical)** With the D-01 guard removed (or simulated absent), verify that briefing DOES consume the dedup slot and a subsequent context_get produces access_count += 0. This confirms the guard is load-bearing, not redundant.
3. **(AC-06)** Call record_briefing_usage with weight=0 on entries [X, Y, Z]. Assert access_count for none of X, Y, Z increments. Assert DedupState.access_counted remains empty.
4. Record briefing on entry X (weight=0) twice in the same session. Assert dedup slot is still absent. Then call context_get on X. Assert access_count = 2 (not 0, not 4).

**Coverage Requirement**: Integration test must issue the full briefing-then-get sequence end-to-end through UsageService, not just test record_briefing_usage in isolation. The dedup state must be inspected, not inferred.

---

### R-02: Positional Column Index Drift (SR-01)

**Severity**: High
**Likelihood**: Med
**Impact**: Silent runtime deserialization corruption — a wrong phase value is read back, or a panic occurs from a type mismatch, with no compile-time signal. Any of the four divergent sites (analytics.rs INSERT, scan_query_log_by_sessions SELECT, scan_query_log_by_session SELECT, row_to_query_log index 9) can independently cause this. Pattern #3519 and ADR-007 identify this as an atomic change unit.

**Test Scenarios**:
1. **(AC-17 — primary SR-01 guard)** Write a query_log row via insert_query_log with phase=Some("design"). Read it back via scan_query_log_by_session. Assert the returned QueryLogRecord.phase equals Some("design"). If any of the four sites is out of sync, this test fails with a runtime column-index error.
2. Write a query_log row with phase=None. Read back. Assert phase=None (not an empty string, not a panic).
3. Write multiple rows with different phase values including None. Read back all via scan_query_log_by_sessions. Assert each row's phase matches what was written.
4. Inspect the analytics.rs INSERT column name list vs the SELECT column list: confirm `phase` is the 9th named column in both and that row_to_query_log reads index 9 (code review gate, AC-21).

**Coverage Requirement**: AC-17 round-trip test is mandatory and must use the real analytics drain (not a stub) so the INSERT positional params are exercised. Pattern #3004 (analytics drain causal integration test) applies here.

---

### R-03: Phase Snapshot Race (ADR-002 violation)

**Severity**: High
**Likelihood**: Med
**Impact**: A concurrent context_cycle(phase-start) event updates SessionState.current_phase between the handler start and the phase snapshot, attributing the wrong phase to the access event and query_log row. Undetectable at runtime; corrupts the phase-conditioned frequency table permanently.

**Test Scenarios**:
1. **(AC-12 — code review gate)** Inspect each of the four handler bodies in tools.rs. Confirm current_phase_for_session is the first statement before any .await. This is a delivery gate checklist item, not an automated test.
2. Unit test current_phase_for_session independently: register a session with phase "delivery", call the function, assert Some("delivery"). Register with no phase, assert None. Call with session_id=None, assert None.
3. Verify the function is called exactly once per handler invocation (no duplicate get_state calls within a single handler) — code review gate, NFR-01.
4. For context_search specifically: assert the same phase value is used in both UsageContext.current_phase and QueryLogRecord.phase in the same handler invocation (AC-16 + R-04 coverage).

**Coverage Requirement**: The race condition is non-deterministic and cannot be reliably reproduced in a test. The gate is AC-12 code review plus unit tests for current_phase_for_session correctness.

---

### R-04: Dual get_state at context_search (SR-06)

**Severity**: Med
**Likelihood**: Med
**Impact**: Two separate get_state calls at the context_search site could produce different phase values if a phase-end event arrives between them. One consumer (UsageContext) sees phase "delivery"; the other (query_log) sees None. The discrepancy silently corrupts analytics correlation.

**Test Scenarios**:
1. **(AC-16)** Integration test: open a session with phase "delivery". Issue a context_search. Read the resulting query_log row. Assert query_log.phase = "delivery". Assert the UsageContext passed to UsageService also carries phase = "delivery" (requires inspection of the record or a spy). Both values must originate from a single snapshot.
2. Code review gate: confirm there is exactly one `get_state` call in the context_search handler body (C-04, FR-18).

**Coverage Requirement**: AC-16 integration test is sufficient for the runtime side. The single-call constraint is enforced by code review.

---

### R-05: Schema Version Cascade (SR-02)

**Severity**: Med
**Likelihood**: High
**Impact**: migration_v15_to_v16.rs still asserts `schema_version == 16`; the entire migration test suite fails at gate. server.rs lines 2059 and 2084 retain `assert_eq!(version, 16)` causing integration test failures. Pattern #2933 confirms this is a recurring miss across features.

**Test Scenarios**:
1. **(AC-22)** Before gate: `grep -r 'schema_version.*== 16' crates/` must return zero matches. This grep is a mandatory delivery gate check.
2. **(AC-13)** Unit test `test_current_schema_version_is_17` asserts `CURRENT_SCHEMA_VERSION == 17`.
3. **(AC-19 T-V17-06)** migration_v16_to_v17.rs asserts schema_version counter = 17 after migration.
4. Run `cargo test --workspace` and confirm the renamed test function `test_current_schema_version_is_17` is the only schema-version constant test present (old `_is_16` name must not exist).

**Coverage Requirement**: AC-22 grep check is mandatory before PR merge. Test count in migration_v15_to_v16.rs must be verified as fully updated (not partially patched).

---

### R-06: UDS Compile Break (SR-03)

**Severity**: Med
**Likelihood**: High
**Impact**: If uds/listener.rs:1324 is not updated to pass None as the seventh argument to QueryLogRecord::new, the entire workspace fails to compile. This is a hard gate failure — CI fails immediately.

**Test Scenarios**:
1. **(AC-23)** `cargo build --workspace` (or `cargo check --workspace`) completes without error. This is the definitive test — a compile error is unmissable.
2. Confirm the UDS call site passes `phase: None` (not a phase value) — code review confirms no semantic change is made to the UDS path.

**Coverage Requirement**: Compilation is the test. No additional scenario needed.

---

### R-07: context_get Weight Not Corrected to 2

**Severity**: Med
**Likelihood**: Med
**Impact**: context_get continues underweighting the highest-signal read event. Confidence scores under-represent deliberate full-content retrieval. The D-01 guard (R-01) still works correctly, but the weight correction is absent.

**Test Scenarios**:
1. **(AC-05)** Unit test: insert entry X with access_count=0. Call context_get for entry X in a fresh session (no prior dedup entry). Assert access_count increments to 2 (not 1).
2. Call context_get for the same entry twice. Assert second call produces no increment (dedup filter). access_count remains 2.
3. Inspect the UsageContext literal in the context_get handler: assert `access_weight: 2` is present (code review or static assertion via a test that captures the UsageContext).

**Coverage Requirement**: AC-05 unit test is sufficient. The specific access_count value (2 not 1) distinguishes this from the pre-feature state.

---

### R-08: context_briefing Weight Not Corrected to 0

**Severity**: Med
**Likelihood**: Med
**Impact**: Briefing continues inflating access_count for entries the agent never deliberately read. Combined with the D-01 dedup slot burn, this also blocks the weight-2 context_get increment.

**Test Scenarios**:
1. **(AC-06)** Unit test: insert entries [X, Y] with access_count=0. Call context_briefing returning [X, Y]. Assert access_count for both X and Y remains 0.
2. Confirm access_weight: 0 is present in the briefing UsageContext literal (code review).
3. Verify that the D-01 guard in record_briefing_usage fires for weight=0 (see R-01 scenario 1 — these are the same guard path).

**Coverage Requirement**: AC-06 plus the dedup tests from R-01 together cover this fully.

---

### R-09: confirmed_entries Missing from Test Helpers

**Severity**: Med
**Likelihood**: High
**Impact**: Forgetting to add `confirmed_entries: HashSet::new()` to make_state_with_rework or related helpers causes all existing SessionState tests to fail with a compile error. Pattern #3180 (every new SessionState field requires test helper update) is a recurring obligation.

**Test Scenarios**:
1. **(AC-20)** `cargo test --workspace` passes with no new failures. This is the definitive check — a compile error from missing field initializer is unmissable.
2. Search for all occurrences of `make_state_with_rework` and `SessionState {` struct literals in test code and confirm each has the `confirmed_entries` field (code review gate).

**Coverage Requirement**: Compilation + full test suite pass is sufficient. Pattern #3180 compliance is verified by inspection.

---

### R-10: Phase Not Written to query_log (context_search)

**Severity**: Med
**Likelihood**: Med
**Impact**: UsageContext carries the correct phase in memory but the query_log row is written with phase=NULL, blocking the phase-conditioned frequency table (ass-032) even though the schema is correct.

**Test Scenarios**:
1. **(AC-16)** Integration test: register session with phase "delivery". Issue context_search. Drain the analytics queue (flush). Query query_log for the row. Assert phase = "delivery".
2. Integration test: issue context_search with no active session. Assert query_log row has phase = NULL.
3. Verify that the phase_for_log variable in context_search handler is bound before the spawn_blocking closure and is passed as the final QueryLogRecord::new argument (FR-18, code review gate).

**Coverage Requirement**: AC-16 must use the real analytics drain and a real database, not mocks. Pattern #3004 (analytics drain causal integration test) prescribes this approach.

---

### R-11: Migration Idempotency Failure

**Severity**: Med
**Likelihood**: Low
**Impact**: Running migration on a v17 database (e.g., in a re-deploy or rollback scenario) fails because the pragma_table_info pre-check is missing or incorrect, causing ALTER TABLE to fail on an existing column.

**Test Scenarios**:
1. **(AC-15, T-V17-04)** Create a v17 database (or run v16→v17 once). Run the migration function again. Assert no error is returned. Assert schema_version is still 17.
2. Verify `CREATE INDEX IF NOT EXISTS` is used (not `CREATE INDEX`) — if not, index creation will also fail on re-run.

**Coverage Requirement**: T-V17-04 in migration_v16_to_v17.rs is the definitive test.

---

### R-12: Pre-Existing Row Deserialization After Migration

**Severity**: Med
**Likelihood**: Low
**Impact**: A query_log row written before migration has phase=NULL in the new column. The row_to_query_log deserializer at index 9 uses try_get::<Option<String>, _> which must handle NULL as None. If the wrong type is used, a runtime panic occurs when reading historical rows.

**Test Scenarios**:
1. **(AC-18, T-V17-05)** Insert a query_log row in a v16 database. Run v16→v17 migration. Read back the row via scan_query_log_by_session. Assert phase = None (not a panic, not an error).
2. Verify row_to_query_log uses Option<String> at index 9, not String (a non-nullable type would panic on NULL).

**Coverage Requirement**: T-V17-05 in migration_v16_to_v17.rs is the definitive test.

---

### R-13: confirmed_entries Cardinality Error

**Severity**: Low
**Likelihood**: Med
**Impact**: Multi-target context_lookup incorrectly populates confirmed_entries, inflating the explicit-fetch signal for entries the agent may not have individually intended to retrieve. Thompson Sampling inherits corrupted data.

**Test Scenarios**:
1. **(AC-10 single-target)** Call context_lookup with target_ids=[X]. Assert confirmed_entries contains X.
2. **(AC-10 multi-target)** Call context_lookup with target_ids=[X, Y]. Assert confirmed_entries does NOT contain X or Y.
3. Verify the `target_ids.len() == 1` check is on the request parameter list, not the response (ADR-004 — request-side cardinality).

**Coverage Requirement**: Both AC-10 sub-cases are required. The negative test (multi-target does not populate) is as important as the positive.

---

### R-14: context_lookup Weight Inadvertently Changed

**Severity**: Low
**Likelihood**: Low
**Impact**: context_lookup weight drifts from 2 to something else during edits. The spec says weight is unchanged at 2.

**Test Scenarios**:
1. **(AC-11)** Existing lookup tests pass (no access_count regression). Code review confirms weight=2 is still present in the context_lookup UsageContext literal.

**Coverage Requirement**: Existing test coverage plus code review.

---

### R-15: UsageContext Doc Comment Stale

**Severity**: Low
**Likelihood**: Low
**Impact**: The doc comment on UsageContext.current_phase says "None for all non-store operations" after read-side tools now populate it. Future implementors are misled. This is an ADR-006 deliverable obligation.

**Test Scenarios**:
1. Code review: confirm UsageContext.current_phase doc comment enumerates read-side tools as populating the field and restricts "None" to mutation tools only.

**Coverage Requirement**: Code review only. No automated test possible for doc comment accuracy.

---

### R-16: D-01 Guard Bypassed by Future Refactor

**Severity**: Low
**Likelihood**: Low
**Impact**: A future refactor routes AccessSource::Briefing through record_mcp_usage, bypassing the D-01 guard in record_briefing_usage. The guard is structurally incomplete (ADR-003 SR-07 acknowledgment).

**Test Scenarios**:
1. No active scenario — this is a future-state risk. The guard location is documented in ADR-003 as a structural limitation. AC-07 would catch a regression if the routing ever changes, because the briefing-then-get integration test would fail.

**Coverage Requirement**: Documented risk accepted per ADR-003. AC-07 serves as the canary.

---

## Integration Risks

**IR-01: Two-part delivery independence.** Part 1 (in-memory: session.rs + tools.rs + usage.rs) and Part 2 (schema: migration + analytics.rs + query_log.rs) can be tested independently with a dependency boundary: Part 2 tests require the QueryLogRecord.phase field (Part 2 internal), while Part 1 tests do not touch the store layer. However, the full AC-16 integration test (phase written to query_log) requires both parts to be complete. Delivery order: Part 2 schema changes compile independently; Part 1 handler changes that pass phase to QueryLogRecord::new depend on Part 2's constructor signature change. Part 2 must land first or in the same commit.

**IR-02: Analytics drain async gap.** query_log writes go through the enqueue_analytics channel and are processed by the drain task. A test that writes via context_search and immediately queries query_log will find zero rows unless it flushes the drain. Pattern #3004 prescribes an explicit flush or drain-wait in causal integration tests. AC-16 and AC-17 must follow this pattern.

**IR-03: eval/scenarios/tests.rs helper update.** The `insert_query_log_row` helper in eval/scenarios/tests.rs uses a raw SQL INSERT. If it is not updated to include the phase column binding, all 15+ call sites will produce "table has 9 columns but 8 values were supplied" runtime errors. FR-20 covers this, but it is a large surface (15+ sites) and easy to miss one.

**IR-04: make_query_log struct literal in knowledge_reuse.rs.** FR-21 identifies a second test helper location that constructs QueryLogRecord directly. If missed, this produces a compile error isolated to knowledge_reuse.rs tests rather than the main test suite, and could be overlooked if only running targeted tests.

---

## Edge Cases

**EC-01: Session has no phase set (current_phase = None).** All four handlers must pass None to UsageContext.current_phase without panic. query_log row must be written with phase=NULL (not an empty string).

**EC-02: No session_id in the MCP call.** current_phase_for_session receives session_id=None. Must return None without attempting registry lookup. Verified by AC-01–AC-04 None arm.

**EC-03: context_briefing called with an empty entry list.** D-01 guard fires on weight=0 before any iteration. No dedup slot consumed, no access_count increment. Must not panic.

**EC-04: context_lookup with target_ids=[] (empty).** target_ids.len() == 0, not 1. confirmed_entries must not be updated. This boundary case is not the same as len() > 1.

**EC-05: context_get called for an entry that does not exist.** record_confirmed_entry should still be called on a successful retrieval path. If the handler returns early on not-found, confirmed_entries must not be polluted with a non-existent ID. The specification says "after a successful retrieval" (FR-08).

**EC-06: Phase string contains unusual characters.** Phase is a free-form string (e.g., "design", "delivery"). A phase value containing SQL metacharacters must be handled safely by parameterized binding — the ?9 bind in analytics.rs uses SQLx parameter binding, not string interpolation. No injection risk, but verify the round-trip test uses a non-trivial phase string (e.g., "design/v2") to confirm encoding is clean.

**EC-07: Schema version already at 17 when migration runs.** The `current_version < 17` guard skips the branch entirely. No column addition or index creation attempted. This is the idempotency path tested by T-V17-04.

---

## Security Risks

**SR-SEC-01: Phase is a free-form string from SessionState.** The phase value originates from a prior `context_cycle(start)` call, which takes agent-supplied input. This value flows into query_log as TEXT via SQLx parameterized binding (`?9`). No injection risk — SQLx binds prevent SQL injection. The value is not used in any dynamic SQL construction.

**SR-SEC-02: No external input accepted by this feature beyond existing channels.** The phase capture reads from SessionState, which was already populated by a prior authenticated MCP call. No new untrusted input surface is introduced by this feature.

**SR-SEC-03: confirmed_entries is in-memory only.** The HashSet<u64> contains only entry IDs (u64). No string data from external sources. No serialization/deserialization path. No injection or overflow risk.

**SR-SEC-04: Migration SQL is hardcoded strings.** All migration SQL uses hardcoded literals with no user-supplied data. The pragma_table_info query filters by hardcoded column name 'phase'. No dynamic SQL construction.

**Blast radius assessment**: If any component in this feature is compromised, the worst case is phase attribution corruption in analytics data — incorrect phase labels on query_log rows or incorrect access_count values. No data is deleted, no authentication paths are touched, no secrets are accessed. The feature is analytics instrumentation, not a security boundary.

---

## Failure Modes

**FM-01: D-01 guard absent.** Briefing call silently burns dedup slot. Subsequent context_get on same entry produces access_count += 0. No error surfaced. Detected only by AC-07 integration test failing.

**FM-02: Phase captured after await.** Wrong phase attributed to access events. No error surfaced. Detected only by AC-12 code review gate.

**FM-03: analytics.rs INSERT has 8 binds but query_log table has 9 columns (after migration).** SQLite silently inserts NULL for the missing column. Phase is always NULL in query_log even when set in memory. AC-17 round-trip test detects this: phase is written as Some("design") but read back as None.

**FM-04: row_to_query_log reads index 8 (old) instead of 9 (new).** The wrong column value is deserialized as phase. If column 8 is `source` (a String), reading it as Option<String> may succeed but return the source string in the phase field. AC-17 detects this: phase would read back as Some("mcp") instead of Some("design").

**FM-05: UDS call site not updated.** Compile error at `uds/listener.rs:1324`. Hard gate failure — CI fails to build.

**FM-06: migration_v15_to_v16.rs assertions not updated.** Test suite has a failing test named `test_current_schema_version_is_16`. Gate check: grep for `== 16` assertions returns non-zero.

**FM-07: record_confirmed_entry called on multi-target lookup.** Thompson Sampling cold-inherits inflated confirmed_entries data. No immediate error. AC-10 multi-target negative test detects this.

---

## Scope Risk Traceability

| Scope Risk | AC | Architecture Risk | Resolution |
|-----------|-----|------------------|------------|
| SR-01: Positional column index fragility | AC-17, AC-21 | R-02 | Analytics.rs INSERT, both SELECTs, and row_to_query_log treated as atomic change unit per ADR-007. AC-17 round-trip test is the runtime guard against divergence. |
| SR-02: Schema version cascade (v16 assertions) | AC-13, AC-22 | R-05 | ARCHITECTURE.md enumerates all files with `== 16` assertions. AC-22 mandates pre-gate grep check. migration_v16_to_v17.rs added as new test file. |
| SR-03: UDS compile fix | AC-23 | R-06 | uds/listener.rs:1324 explicitly called out in architecture and specification as compile-fix-only (pass None). Compile success is the test. |
| SR-04: confirmed_entries semantic contract | AC-24 | R-13 | ADR-005 locks the explicit-fetch-only contract. Doc comment required on field. AC-10 validates cardinality boundary. |
| SR-05: briefing weight=1 analytics regression | — | R-08 | Architecture confirms no existing analytics query assumes briefing weight. Regression risk accepted as low per architecture §SR-05. |
| SR-06: Dual get_state at context_search | AC-16 | R-04 | ADR-002 and C-04 mandate single get_state. AC-16 integration test + code review. |
| SR-07: D-01 guard future bypass | — | R-16 | Accepted risk per ADR-003. Guard location documented. AC-07 canary test. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-03) | AC-07 integration test (briefing→get sequence); AC-17 round-trip read-back; AC-12 code review gate |
| High | 6 (R-04–R-09) | AC-16, AC-22 grep, AC-23 compile, AC-05, AC-06, AC-20 compile |
| Medium | 5 (R-10–R-13, IR-01–IR-04) | AC-16 drain-flush integration, T-V17-04, T-V17-05, AC-10 both arms, eval helper update |
| Low | 4 (R-14–R-16, EC edge cases) | AC-11, AC-24 code review, AC-07 canary |

**Minimum test surface to gate on**: AC-07 (D-01 guard), AC-17 (SR-01 round-trip), AC-22 grep check, AC-23 compile, `cargo test --workspace` green. Every other AC flows from these five.

---

## Knowledge Stewardship

- Queried: /uni-knowledge-search for `"lesson-learned failures gate rejection"` — found #3493 (bugfix-383 retro), no directly applicable lessons to col-028 domain.
- Queried: /uni-knowledge-search for `"risk pattern"` category:pattern — found #3426 (formatter regression risk), #2933 (schema version cascade pattern, directly applicable to SR-02).
- Queried: /uni-knowledge-search for `"UsageDedup weight-0 dedup slot"` — found #3503 and #3510, both directly confirm D-01 collision is a known documented pattern; elevates R-01 to Critical.
- Queried: /uni-knowledge-search for `"SQLite migration schema version cascade"` — found #2933 (schema version cascade, pattern) and #2937 (server.rs migration test update), directly applied to R-05 and AC-22.
- Queried: /uni-knowledge-search for `"analytics drain phase-snapshot integration test"` — found #3004 (analytics drain causal test pattern), applied to IR-02 and R-10 coverage requirement.
- Stored: nothing novel to store — all patterns found (#3503, #3510, #2933) are already documented in Unimatrix; col-028 applies them but does not introduce a new cross-feature pattern not already captured.
