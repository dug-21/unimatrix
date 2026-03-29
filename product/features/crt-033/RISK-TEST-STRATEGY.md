# Risk-Based Test Strategy: crt-033

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Schema cascade miss — one or more of the seven v17→v18 touchpoints omitted | High | High | Critical |
| R-02 | Synchronous write on write_pool_server() causes pool starvation under concurrent first-calls | High | Med | High |
| R-03 | evidence_limit truncation applied at storage time instead of render time — stored JSON loses evidence | High | Med | High |
| R-04 | force=true + purged signals path falls through to ERROR_NO_OBSERVATION_DATA when a stored record exists | High | Med | High |
| R-05 | Memoization hit path still executes observation load or computation steps (AC-04 false pass) | High | Low | High |
| R-06 | serde deserialization fails on stored summary_json for records written by an older schema_version | Med | Med | High |
| R-07 | pending_cycle_reviews query based on cycle_events misidentifies or excludes valid pending cycles | Med | Med | Medium |
| R-08 | Version advisory not included when schema_version differs; or handler silently recomputes | Med | Med | Medium |
| R-09 | store_cycle_review called from spawn_blocking (ADR-001 violation) causes pool starvation | High | Low | Medium |
| R-10 | INSERT OR REPLACE on concurrent first-call for the same cycle corrupts the stored record | Low | Low | Low |
| R-11 | summary_json exceeds 4MB ceiling; store layer panics instead of returning an error | Med | Low | Low |
| R-12 | pending_cycle_reviews uses write_pool_server() instead of read_pool() (entry #3619) | Med | Low | Low |
| R-13 | SUMMARY_SCHEMA_VERSION defined in the wrong location (tools.rs or unimatrix-observe) breaking C-04 | Med | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Schema cascade miss — one or more of the seven v17→v18 touchpoints omitted

**Severity**: High
**Likelihood**: High
**Impact**: Fresh databases do not have the `cycle_review_index` table; existing databases fail to migrate; column-count or parity tests fail at CI gate; the `cycle_review_index` write at step 8a raises a "no such table" error at runtime. Historical evidence: entry #3539 documents this as a recurring gate failure pattern.

**Test Scenarios**:
1. Open a freshly created database (no prior state); assert `SELECT name FROM sqlite_master WHERE name='cycle_review_index'` returns one row and `schema_version` counter = 18.
2. Build a v17-shaped database (using the v17 DDL snapshot), open it with `SqlxStore`, assert the migration runs without error and `cycle_review_index` exists with all five columns present and no pre-existing table or row is modified.
3. Run the migration twice on the same database (idempotency); assert no error and `schema_version` = 18 (NFR-06).
4. Run `grep -r 'schema_version.*== 17' crates/` and assert zero matches (AC-02b cascade grep check from entry #3539).
5. Assert `CURRENT_SCHEMA_VERSION` constant in `migration.rs` == 18 in a unit test.
6. Assert `sqlite_parity` tests pass: table-count and named-table assertions include `cycle_review_index`.

**Coverage Requirement**: All seven cascade touchpoints must be exercised. The migration integration test (AC-02 / AC-13) and the cascade grep check are both required; neither alone is sufficient.

---

### R-02: Synchronous write on write_pool_server() causes pool starvation under concurrent first-calls

**Severity**: High
**Likelihood**: Med
**Impact**: Two or more concurrent first-call requests for different cycles both enter the full computation pipeline, then both attempt `INSERT OR REPLACE`. With `max_connections=1`, the second writer blocks until the first completes. If the computation path itself holds a write connection (it should not), deadlock is possible — see entries #2266 and #2249. If the handler incorrectly calls `store_cycle_review` from `spawn_blocking`, pool starvation occurs (ADR-001 explicitly prohibits this, citing entry #2266).

**Test Scenarios**:
1. Spawn two concurrent async tasks each invoking the handler for different cycle IDs simultaneously; assert both complete without error and both rows appear in `cycle_review_index`.
2. Confirm `store_cycle_review` is awaited directly in the handler's async context — not wrapped in `spawn_blocking` or `block_in_place`. Static code review / grep for `spawn_blocking.*store_cycle_review`.
3. Assert the computation pipeline (steps 3–8) does not acquire the write pool; only step 8a acquires it. Review that `list_all_metrics`, `detect_hotspots`, `store_metrics` use appropriate pools.

**Coverage Requirement**: At least one integration test exercising concurrent first-calls. Static check for spawn_blocking misuse.

---

### R-03: evidence_limit truncation applied at storage time instead of render time

**Severity**: High
**Likelihood**: Med
**Impact**: Stored `summary_json` contains truncated hotspot evidence. On a memoization hit, callers that did not pass `evidence_limit` receive truncated evidence with no indication that evidence is missing. This silently degrades the value of the stored record and violates C-03 and AC-08.

**Test Scenarios**:
1. Seed a cycle with 10 hotspots each having 5 evidence items. Call handler with `evidence_limit=2`. After the call, read `summary_json` directly from `cycle_review_index`; deserialize; assert each hotspot in the raw JSON has 5 evidence items (not 2).
2. Call the handler again (memoization hit) with `evidence_limit=2`; assert the returned MCP response hotspots each have 2 evidence items.
3. Call the handler again (memoization hit) without `evidence_limit`; assert hotspots have the full 5 evidence items.

**Coverage Requirement**: AC-08 must be an integration test that reads raw `summary_json` from the table (not just the MCP response).

---

### R-04: force=true + purged signals path falls through to ERROR_NO_OBSERVATION_DATA when a stored record exists

**Severity**: High
**Likelihood**: Med
**Impact**: An agent calls `force=true` after signals are purged and receives an error instead of the stored record. The stored record's existence is the whole point of crt-033's graceful degradation (AC-06, FR-05). This risk is elevated by the SR-07 discriminator complexity: the handler must distinguish "empty because purged" from "empty because never existed."

**Test Scenarios**:
1. Insert a `cycle_review_index` row directly (bypassing handler). Ensure no observations exist for that cycle. Call handler with `force=true`. Assert: response is `Ok` (not error), contains the explanatory note `"Raw signals have been purged"`, and `raw_signals_available` is reported as false.
2. Same scenario but with no `cycle_review_index` row: assert response is `ERROR_NO_OBSERVATION_DATA` (AC-07, FR-06).
3. Seed observations, call handler with `force=true`, purge observations between calls, call again with `force=true`: assert the second call returns the stored record with note (not error).
4. For the SR-07 discriminator: when `force=true` and observations are empty, assert the handler queries `cycle_events` to distinguish purged vs never-existed before checking `cycle_review_index`.

**Coverage Requirement**: AC-06, AC-07, and AC-15 are all required. The three sub-cases (stored record exists, stored record absent, concurrent purge between calls) must each have a dedicated test.

---

### R-05: Memoization hit path still executes observation load or computation steps

**Severity**: High
**Likelihood**: Low
**Impact**: The performance guarantee of memoization is violated. Worse: if computation is re-run but the stored record is still returned, the result diverges from what would have been returned by fresh computation (inconsistency). If the stored record is overwritten on every hit, idempotency is broken.

**Test Scenarios**:
1. Insert a `cycle_review_index` row directly (bypassing handler). Call handler with `force=false`. Assert no observation-load queries are executed (mock or spy on the store's observation-read methods, or assert `cycle_events`/`observations` tables are not queried — verify via query counter or store mock).
2. Assert that the `computed_at` timestamp in the stored row is unchanged after a memoization hit (the row was not overwritten).
3. Assert the returned report's `feature_cycle` matches the stored record's, and no `store_cycle_review` call occurred on the hit path.

**Coverage Requirement**: AC-04 and AC-14 require isolation of the hit path. A mock or patched store is the correct verification mechanism.

---

### R-06: serde deserialization fails on stored summary_json for records written by an older schema_version

**Severity**: Med
**Likelihood**: Med
**Impact**: A stored record written at `SUMMARY_SCHEMA_VERSION=1` cannot be deserialized after a field change to `RetrospectiveReport`. Instead of the version advisory + stored record (the specified behavior), the handler crashes or returns an unrecoverable error. ADR-003 specifies a fallthrough to full recomputation on deserialization failure — this fallthrough must not silently lose the advisory or panic.

**Test Scenarios**:
1. Round-trip test: serialize a fully-populated `RetrospectiveReport` instance to JSON, deserialize back, assert field-level equality including nested `hotspots`, `evidence`, `phase_narrative` (AC-16).
2. Backward-compat test: deserialize a JSON blob that lacks optional fields (simulating an older stored record); assert `#[serde(default)]` fields are populated with defaults rather than erroring.
3. Deliberately corrupt `summary_json` in a `cycle_review_index` row (invalid JSON). Call handler with `force=false`. Assert: handler does NOT panic; falls through to full recomputation with a tracing warning; the response is a freshly computed report (ADR-003 defense-in-depth).
4. Insert a row with `schema_version=0`. Call handler. Assert advisory text contains `"use force=true to recompute"` and the stored record is returned (not an error) — AC-04b.

**Coverage Requirement**: AC-16 round-trip test plus the corrupted-JSON fallthrough test are both required. The corrupted-JSON path is specifically called out in ADR-003 and has no AC — it must be added as a test even though it lacks an explicit AC.

---

### R-07: pending_cycle_reviews query misidentifies or excludes valid pending cycles

**Severity**: Med
**Likelihood**: Med
**Impact**: Operators see an incorrect backlog: cycles needing review are absent from the list (false negatives) or cycles that never need review appear (false positives). The specification changed the query source from `query_log.feature_cycle` (SCOPE, which does not exist) to `cycle_events` with `event_type = 'cycle_start'` (OQ-02 resolution). This substitution is the primary risk surface: a cycle that has `cycle_start` but no Unimatrix query activity will now appear as pending even if there is nothing to review.

**Test Scenarios**:
1. Seed two cycles in `cycle_events` with `event_type='cycle_start'` within K-window. Write `cycle_review_index` row for one. Call `context_status`. Assert `pending_cycle_reviews` contains exactly the un-reviewed cycle (AC-09).
2. Write `cycle_review_index` rows for both cycles. Call `context_status`. Assert `pending_cycle_reviews` is empty (AC-10).
3. Seed a cycle with only `cycle_end` event (no `cycle_start`). Assert it does not appear in `pending_cycle_reviews`.
4. Seed a cycle whose `cycle_start` timestamp is outside the K-window (older than 90 days). Assert it does not appear in `pending_cycle_reviews`.
5. Seed a pre-cycle_events cycle (row in `observation_metrics` only, no `cycle_events` row). Assert it does not appear in `pending_cycle_reviews`.
6. Seed a `cycle_start` event with NULL `cycle_id`. Assert no panic and the NULL row is excluded from the list.

**Coverage Requirement**: AC-09 and AC-10 cover the happy path. Scenarios 3–6 cover exclusion correctness and are not explicitly covered by any AC — they must be added.

---

### R-08: Version advisory absent when schema_version differs; or handler silently recomputes

**Severity**: Med
**Likelihood**: Med
**Impact**: Without the advisory, callers have no signal that their stored result is stale. If the handler silently recomputes on version mismatch, the idempotency guarantee is broken (C-05 violation). Callers relying on the deterministic hit path would receive freshly computed results without requesting them.

**Test Scenarios**:
1. Insert a `cycle_review_index` row with `schema_version=0` (mismatch). Call handler with `force=false`. Assert response text contains advisory string with both the stored version (0) and current version (SUMMARY_SCHEMA_VERSION). Assert no observation-load queries executed (AC-04b).
2. Insert a row with `schema_version=SUMMARY_SCHEMA_VERSION` (match). Call handler with `force=false`. Assert advisory string is absent from response.
3. Insert a row with `schema_version=999` (future, higher than current). Assert advisory string is present and stored record is returned as-is.

**Coverage Requirement**: AC-04b. Scenario 3 (future version) is not in any AC and must be added.

---

### R-09: store_cycle_review called from spawn_blocking (ADR-001 violation)

**Severity**: High
**Likelihood**: Low
**Impact**: Calling an async sqlx query from `spawn_blocking` requires `block_in_place`, which risks pool starvation on the max-1 write pool. Entries #2266 and #2249 document this as a historical deadlock cause. ADR-001 explicitly prohibits this pattern.

**Test Scenarios**:
1. Static check: `grep -n 'spawn_blocking' crates/unimatrix-server/src/mcp/tools.rs` — assert no occurrence within the memoization or store step functions.
2. Integration test: call handler once in a normal async test context; assert it completes within 2 seconds (a pool starvation deadlock would timeout or hang).

**Coverage Requirement**: Static grep check is mandatory. The integration latency test (NFR-02) serves as the runtime guard.

---

### R-10: INSERT OR REPLACE on concurrent first-call for the same cycle corrupts the stored record

**Severity**: Low
**Likelihood**: Low
**Impact**: Two requests for the same cycle arrive simultaneously, both find no stored record, both compute, both attempt `INSERT OR REPLACE`. Last-writer-wins is safe for data integrity. The only waste is duplicate computation. Spec (OQ-03) explicitly accepts this race. No data corruption is possible.

**Test Scenarios**:
1. Spawn two concurrent tasks for the same cycle. Assert both complete successfully and exactly one row exists in `cycle_review_index` afterward (not two, not zero).
2. Assert both returned reports have `feature_cycle` matching the input (not a cross-contaminated result).

**Coverage Requirement**: A concurrent integration test is sufficient; this is a smoke-level check given the accepted race.

---

### R-11: summary_json exceeds 4MB ceiling; store layer panics instead of returning an error

**Severity**: Med
**Likelihood**: Low
**Impact**: A pathological cycle with very large hotspot evidence (~20 hotspots × ~30 evidence items × large text) could exceed 4MB. NFR-03 requires the store layer return an error, not panic. Without a size check, a very large `TEXT` insert succeeds in SQLite (no enforcement), but the JSON serialization could OOM on very large cycles.

**Test Scenarios**:
1. Construct a `CycleReviewRecord` with `summary_json` of exactly 4MB + 1 byte. Call `store_cycle_review`. Assert the return is `Err(...)`, not `Ok`, and the server does not panic.
2. Construct a record with `summary_json` of exactly 4MB. Assert `store_cycle_review` returns `Ok`.
3. Call the handler with a cycle whose `RetrospectiveReport` serializes to > 4MB. Assert the MCP response is a tool error (not a server crash) and the error message is human-readable.

**Coverage Requirement**: NFR-03. The boundary test at exactly 4MB and 4MB+1 is required.

---

### R-12: pending_cycle_reviews uses write_pool_server() instead of read_pool()

**Severity**: Med
**Likelihood**: Low
**Impact**: Using the write pool for a read-only aggregate adds unnecessary contention. Entry #3619 documents this as a past gate failure for `context_status` aggregates. ADR-004 specifies `read_pool()`.

**Test Scenarios**:
1. Static check: `grep -n 'write_pool_server\|read_pool' crates/unimatrix-store/src/cycle_review_index.rs` — assert `pending_cycle_reviews` implementation uses `read_pool()` and `get_cycle_review` uses `read_pool()`. Only `store_cycle_review` uses `write_pool_server()`.
2. Integration test: call `context_status` while a long-running write is in progress; assert `pending_cycle_reviews` completes without blocking.

**Coverage Requirement**: Static check is the primary gate. Integration test is advisory.

---

### R-13: SUMMARY_SCHEMA_VERSION defined in the wrong location

**Severity**: Med
**Likelihood**: Low
**Impact**: If defined as a literal in `tools.rs` or in `unimatrix-observe`, future bumps require changes in unexpected places and the single-definition rule (C-04, FR-12, AC-17) is silently violated.

**Test Scenarios**:
1. CI check: `grep -r 'SUMMARY_SCHEMA_VERSION' crates/` — assert exactly one definition, located in `crates/unimatrix-store/src/cycle_review_index.rs`. All other occurrences are imports/uses, not definitions.
2. AC-17 verification: `grep -r 'SUMMARY_SCHEMA_VERSION.*=.*[0-9]' crates/unimatrix-server/` returns zero matches (no inline literal in the handler).

**Coverage Requirement**: AC-17 CI check is the gate.

---

## Integration Risks

### I-01: OQ-02 schema substitution (query_log.feature_cycle → cycle_events)

The SCOPE specified `query_log.feature_cycle` for `pending_cycle_reviews`, but the spec confirms this column does not exist. The specification substitutes `cycle_events` with `event_type='cycle_start'`. This changes the semantics of "has raw signals": from "Unimatrix was queried during this cycle" to "a cycle_start event was recorded." A cycle with a `cycle_start` but zero Unimatrix queries will now appear as pending even if there is nothing to review. Testers must verify the `event_type = 'cycle_start'` filter is correctly applied and that the result set matches operator expectations (not just SQL correctness).

### I-02: GH #409 gate contract

crt-033 writes `cycle_review_index`; GH #409 reads it. The gate contract is: `SELECT COUNT(*) FROM cycle_review_index WHERE feature_cycle = ?` before purge. crt-033 cannot test #409's reading behavior, but it must verify that:
- A row written by step 8a is immediately visible to a subsequent SELECT in the same process (ADR-001 synchronous write guarantee).
- The row is written before the handler returns — no `await` fence between step 8a and the handler's return path.

### I-03: force=true skips step 2.5 entirely

The architecture specifies that `force=true` skips the `get_cycle_review` call at step 2.5. The discriminator logic for purged-signals runs on the empty-attributed-observations path (step 4), not step 2.5. This two-path control flow (skip vs fallback) must be tested end-to-end: `force=true` with live signals must not consult `cycle_review_index` at step 2.5 at all.

### I-04: StatusReport struct extension — Default + JSON + summary formatters

Three separate code sites must be updated (FR-09, FR-11): `StatusReport::default()`, `StatusReportJson`, and `From<&StatusReport>`. If any is missed, the field either panics on access, is absent from JSON output, or shows an incorrect value. Each must be tested independently.

---

## Edge Cases

| Edge Case | Risk ID | Scenario |
|-----------|---------|----------|
| `feature_cycle` is an empty string | R-07 | `get_cycle_review("")` should return `None` gracefully, not match a garbage row. `store_cycle_review` with empty string should be rejected at validation layer or produce a harmless row. |
| `feature_cycle` is very long (>255 chars) | R-07 | SQLite TEXT has no length limit, but existing `validate_retrospective_params` checks length. Confirm `force` field does not bypass this validation. |
| `force=None` vs `force=Some(false)` equivalence | R-05, R-08 | Both must trigger memoization check. Deserializing `{"feature_cycle":"x"}` (absent `force`) must yield `force=None`, treated as false (AC-12). |
| Stored record with empty `summary_json` | R-06 | An empty string is not valid JSON. `serde_json::from_str("")` returns `Err`. Handler must not panic; ADR-003 fallthrough applies. |
| K-window boundary (cycle exactly at cutoff timestamp) | R-07 | Cycle with `timestamp = k_window_cutoff` — inclusive vs exclusive boundary. SQL uses `>=`; confirm behavior at the exact boundary second. |
| `raw_signals_available` mapping: SQLite INTEGER 0/1 → Rust bool | R-04 | sqlx maps `INTEGER` to `i32`/`i64`, not `bool`, by default. Confirm the `CycleReviewRecord` field type and the sqlx column mapping are consistent. A mismatch causes a runtime deserialization error, not a compile error. |
| Multiple `cycle_start` events for same cycle | R-07 | `SELECT DISTINCT` in the pending query prevents duplicates, but `DISTINCT` on `cycle_id` may not de-duplicate if multiple `cycle_start` events exist. Confirm `DISTINCT` applies to `cycle_id`, not to the row. |

---

## Security Risks

**Untrusted inputs accepted by this feature:**
- `feature_cycle` string in `RetrospectiveParams` (MCP caller-supplied)
- `force: Option<bool>` in `RetrospectiveParams`

**`feature_cycle` injection risk:**
`feature_cycle` is passed to `get_cycle_review(feature_cycle)` and used in a parameterized SQL query (`WHERE feature_cycle = ?`). SQLite parameterized queries prevent SQL injection. The existing `validate_retrospective_params` length/emptiness check limits input size. No additional injection surface exists if sqlx bind parameters are used correctly.

**`force` field:**
Boolean value. No injection surface. Cannot escalate privilege or bypass authentication.

**`summary_json` storage:**
The stored JSON is written from a `RetrospectiveReport` that was computed server-side from trusted internal data (not from caller-supplied content). The only caller-supplied data that flows into `summary_json` indirectly is `feature_cycle` (used to load observations). Observation data originates from prior agent actions on the Unimatrix knowledge base — already-trusted content. No deserialization of untrusted external JSON occurs at write time.

**`summary_json` read path:**
On a memoization hit, the stored `summary_json` is deserialized server-side. If a malicious actor can write arbitrary content to `cycle_review_index` (database-level access), they could craft a JSON payload that exercises unusual serde behavior. This requires database write access, which is outside the MCP threat model. The ADR-003 fallthrough (corrupted JSON → recompute, not panic) limits blast radius to a degraded response, not a crash.

**Blast radius if `cycle_review_index` is compromised:** A poisoned stored record would return stale or incorrect retrospective data to agents that rely on memoization hit path. This is a data-integrity risk (wrong advice to agents) not a code execution risk. The `force=true` escape hatch allows recovery.

---

## Failure Modes

| Failure | Expected Behavior | Test Scenario |
|---------|------------------|---------------|
| `store_cycle_review` write fails (DB error) | Handler returns a tool error; the MCP response must not be a server crash. No partial state written. | Inject a store error at step 8a; assert `Err` propagates as tool error, not panic. |
| `get_cycle_review` read fails (DB error) | Handler falls through to full computation (treat as a miss). An unexpected error from a read should not abort a first-call that could otherwise succeed. | Inject a read error at step 2.5; assert handler continues to full computation. |
| `pending_cycle_reviews` query fails | `StatusReport.pending_cycle_reviews` returns empty (graceful degradation), not an error. `context_status` must not fail because of Phase 7b. | Inject a DB error in `pending_cycle_reviews`; assert `context_status` completes with an empty list (or a logged warning). |
| Deserialization of stored `summary_json` fails | Handler falls through to full computation with a tracing warning (ADR-003). Returns a freshly computed report. | Insert invalid JSON in `summary_json`; call handler with `force=false`; assert response is a valid computed report and no panic. |
| Migration fails midway (incomplete transaction) | Database remains at v17; no partial state; next startup re-attempts migration. Idempotent `CREATE TABLE IF NOT EXISTS` is safe. | Simulate aborted migration (e.g., SIGKILL mid-migration test); reopen DB; assert migration completes on next open. |
| `summary_json` > 4MB ceiling | `store_cycle_review` returns `Err`; handler returns tool error, not panic (NFR-03). | See R-11 test scenarios. |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: RetrospectiveReport may not be fully Serialize + Deserialize | R-06 | Resolved by serde audit (all 23 types confirmed). AC-16 compile-time check + round-trip test required. |
| SR-02: summary_json blob size unbounded at write time | R-11 | Architecture caps at 4MB (NFR-03); store layer must enforce with an error return. Test scenarios for R-11 verify the ceiling. |
| SR-03: SUMMARY_SCHEMA_VERSION unified constant conflates rule-staleness with structural incompatibility | R-08, R-13 | Accepted trade-off (ADR-002). Advisory message covers both causes. Test scenario R-08-3 (future version) and R-13 static check verify version discipline. |
| SR-04: pending_cycle_reviews K-window depends on unmerged #409 constant | R-07 | Architecture pins 90-day default as `PENDING_REVIEWS_K_WINDOW_SECS` in `services/status.rs` (ADR-004). NFR-05 names the constant. Reconciliation required at #409 merge. |
| SR-05: Schema v17→v18 cascade — historically caused gate failures (entry #3539) | R-01 | Architecture enumerates all seven touchpoints. R-01 has six test scenarios covering all touchpoints. CI cascade grep check is mandatory gate. |
| SR-06: Synchronous write latency on shared write_pool | R-02, R-09 | ADR-001 confirms single INSERT OR REPLACE is ~1ms; computation path dominates. R-02 and R-09 test pool contention and spawn_blocking prohibition. |
| SR-07: force=true + purged signals — handler cannot distinguish "purged" from "never had signals" | R-04 | Architecture adds a `cycle_events` COUNT discriminator. R-04 has four test scenarios including the ambiguous-empty case. OQ-01 accepted: ERROR_NO_OBSERVATION_DATA when no stored record exists. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 6 scenarios |
| High | 5 (R-02, R-03, R-04, R-05, R-09) | 14 scenarios |
| Medium | 5 (R-06, R-07, R-08, R-12, R-13) | 14 scenarios |
| Low | 2 (R-10, R-11) | 5 scenarios |

**Total**: 13 risks, 39 test scenarios.

Risks requiring new tests beyond the explicit ACs: R-06 scenario 3 (corrupted-JSON fallthrough), R-07 scenarios 3–6 (exclusion correctness), R-08 scenario 3 (future schema_version), R-09 scenario 1 (spawn_blocking static check), R-11 scenarios 1–3 (4MB ceiling), R-12 scenario 1 (pool selection static check).

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned failures/gate rejections — found entries #3539 (schema cascade checklist, directly informs R-01 severity=High/likelihood=High elevation), #2266 and #2249 (pool starvation/deadlock from spawn_blocking, directly informs R-09), #3619 (read_pool for status aggregates, directly informs R-12), #2125 (analytics drain unsuitable for immediate-visibility writes, confirms ADR-001 decision).
- Queried: `/uni-knowledge-search` for serde round-trip patterns — found entry #885 (serde-heavy types need explicit test coverage, applied to R-06 scenario 1 requirement).
- Stored: nothing novel to store — the schema-cascade-causes-gate-failures pattern is already captured in entry #3539, and the spawn_blocking/pool-starvation pattern is captured in entries #2266 and #2249. No new cross-feature pattern visible from this feature alone.
