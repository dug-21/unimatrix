# Risk-Based Test Strategy: crt-036 — Intelligence-Driven Retention Framework

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Legacy 60-day DELETE survives in one or both sites after delivery | High | Med | Critical |
| R-02 | gc_cycle_activity() deletes sessions before injection_log, leaving orphans | High | Med | Critical |
| R-03 | mark_signals_purged() uses INSERT OR REPLACE instead of targeted UPDATE, clobbering summary_json | High | Low | High |
| R-04 | Per-cycle transaction holds write pool across all cycles (not released between cycles) | High | Med | Critical |
| R-05 | crt-033 gate bypassed: cycle without cycle_review_index row is pruned | High | Low | High |
| R-06 | list_purgeable_cycles() returns a cycle that gc_cycle_activity() then partially prunes (transaction rollback leaves orphaned state) | Med | Low | Medium |
| R-07 | Unattributed prune deletes observations for Active sessions, disrupting in-flight retrospective | Med | Med | High |
| R-08 | max_cycles_per_tick cap not applied: all purgeable cycles processed in one tick | Med | Med | High |
| R-09 | Two-hop subquery uses full-table scan (no index) on 152 MB observations table | Med | Med | High |
| R-10 | RetentionConfig validate() missing for max_cycles_per_tick = 0; server starts with invalid cap | Med | Med | High |
| R-11 | PhaseFreqTable mismatch warning fires on wrong condition (inverted comparison direction) | Low | Med | Medium |
| R-12 | gc_audit_log uses wrong timestamp unit (millis vs seconds); no rows deleted or all rows deleted | Med | Med | High |
| R-13 | raw_signals_available stays 1 after crash between gc_cycle_activity commit and mark_signals_purged | Low | Low | Low |
| R-14 | Protected tables (entries, GRAPH_EDGES, cycle_events, observation_phase_metrics) touched by GC | High | Low | High |
| R-15 | RetentionConfig absent from config.toml silently applies wrong defaults (not 50/180/10) | Med | Low | Medium |
| R-16 | Oldest-K query for PhaseFreqTable guard returns wrong boundary when fewer than K cycles exist | Low | Med | Low |

---

## Risk-to-Scenario Mapping

### R-01: Legacy 60-Day DELETE Survives After Delivery

**Severity**: High
**Likelihood**: Med
**Impact**: Both the old time-based policy and the new cycle-based GC run concurrently. Observations for open or recently-reviewed cycles are deleted by wall-clock age regardless of learning value. The fundamental correctness guarantee of crt-036 is violated. Historical evidence: entry #3579 (wave delivers code but omits mandatory removals) and the scope's explicit call-out of two independent sites (status.rs:1380 and tools.rs:1638).

**Test Scenarios**:
1. Grep assertion: `DELETE FROM observations WHERE ts_millis` must not appear anywhere in `status.rs` after the change (AC-01a).
2. Grep assertion: `DELETE FROM observations WHERE ts_millis` must not appear anywhere in `tools.rs` after the change (AC-01b).
3. Each grep assertion is independently verified — both files checked, not just one.

**Coverage Requirement**: Two independent grep-based assertions, one per file, both must pass before the feature is accepted at Gate 3c. A single combined grep is not sufficient — it would pass if one site is removed but the other remains.

---

### R-02: Cascade Delete Order Violation (sessions Before injection_log)

**Severity**: High
**Likelihood**: Med
**Impact**: Deleting sessions first removes the anchor rows that the injection_log DELETE subquery (`WHERE session_id IN (SELECT session_id FROM sessions WHERE feature_cycle = ?)`) depends on. The injection_log DELETE finds zero matching sessions, leaves all injection_log rows in place, and the transaction commits with orphaned injection_log rows. The GC pass silently under-prunes. Subsequent ticks see the cycle as already pruned (sessions gone) but injection_log accumulates unbounded. Entry #2269 (manual BEGIN/COMMIT connection identity) reinforces that atomic transaction order is non-trivial.

**Test Scenarios**:
1. Insert a cycle with sessions, injection_log rows, observations, and query_log rows. Run GC. Assert injection_log rows for those sessions are absent after GC.
2. Mutation test: temporarily invert the delete order (sessions before injection_log), run the same test, assert it fails with orphaned injection_log rows present. Restore correct order and verify test passes. (AC-08 order-of-operations check.)
3. Assert no injection_log rows exist with session_id values absent from sessions after GC.

**Coverage Requirement**: Integration test must include the order-inversion mutation to confirm the test detects the ordering constraint, not just the end state.

---

### R-03: mark_signals_purged() Overwrites summary_json via INSERT OR REPLACE

**Severity**: High
**Likelihood**: Low
**Impact**: `store_cycle_review()` uses INSERT OR REPLACE, which replaces the entire row. If `mark_signals_purged()` is accidentally routed through this path, the stored retrospective report (`summary_json`) is silently replaced with a default/empty value. The cycle review is unrecoverable once raw signals are also gone. This is the SR-05 risk that the architecture explicitly resolves, but the implementation must be verified to use the targeted UPDATE. Entry #3793 (crt-033 ADR-001) reinforces the write_pool_server direct-write constraint.

**Test Scenarios**:
1. After GC prunes a cycle: read `summary_json` from `cycle_review_index` for the pruned cycle and assert it is byte-for-byte identical to the pre-GC value (AC-05 additional guard).
2. Read `raw_signals_available` for the pruned cycle and assert it is 0.
3. Read `raw_signals_available` for a retained cycle and assert it is unchanged (1 or its original value).
4. Code review assertion: `mark_signals_purged()` must contain `UPDATE cycle_review_index SET raw_signals_available` and must NOT contain `store_cycle_review` or `INSERT OR REPLACE`.

**Coverage Requirement**: summary_json content preservation check is mandatory alongside the flag value check. Checking only the flag allows INSERT OR REPLACE to pass if it happens to write the same JSON.

---

### R-04: Write Pool Held Across All Cycles (Single Spanning Transaction)

**Severity**: High
**Likelihood**: Med
**Impact**: `write_pool_server()` has max_connections=1. A spanning transaction across 10+ purgeable cycles on a 152 MB observations table blocks all concurrent writers (drain task, audit writes, session inserts) for the duration. This is the SR-01/SR-02 deadlock pattern documented in entry #2249 (write_pool + drain task + synchronous write = deadlock). The architecture decision (ADR-001) explicitly resolves this with per-cycle transactions, but if the implementation wraps the cycle loop in a single `pool.begin()`, the risk materializes.

**Test Scenarios**:
1. Structural assertion: `gc_cycle_activity()` must call `pool.begin()` inside the cycle loop body, not outside it. The transaction handle must not be passed between cycles.
2. Integration test with concurrent writer: while GC processes multiple purgeable cycles, assert that an independent write operation (e.g., session insert) completes without timeout. No deadlock observed.
3. Assert `pool.begin()` and `tx.commit()` are used (not raw `BEGIN`/`COMMIT` SQL strings per entry #2159).

**Coverage Requirement**: Structural code review plus concurrent-write integration test. The concurrent-write test is the only way to detect held-pool issues at runtime.

---

### R-05: crt-033 Gate Bypassed

**Severity**: High
**Likelihood**: Low
**Impact**: A cycle that appears in `list_purgeable_cycles()` output but somehow lacks a `cycle_review_index` row (transient read inconsistency, test setup error, or implementation bug) gets its data deleted without the retrospective having been computed. Raw signals are permanently destroyed with no review record. This violates the fundamental retention principle.

**Test Scenarios**:
1. Insert a cycle with sessions and observations but NO `cycle_review_index` row. Ensure the cycle's computed_at (if it had one) would make it outside the K window. Run GC. Assert sessions and observations still exist (AC-04).
2. Verify that `get_cycle_review()` returning `Ok(None)` emits a `tracing::warn!` with the cycle ID and reason (AC-15 gate-skip log assertion).
3. Verify that `get_cycle_review()` returning `Err(_)` also skips the cycle without aborting the pass (FR-04 error handling path).

**Coverage Requirement**: Both the None and Err paths of the gate must be tested independently. The Err path is a distinct failure mode from the None path.

---

### R-06: Partial Transaction Rollback Leaves Inconsistent State

**Severity**: Med
**Likelihood**: Low
**Impact**: If the database returns an error mid-transaction (e.g., constraint violation, I/O error), sqlx rolls back the transaction. For a cycle where observations and query_log were deleted but sessions was not yet reached, the rollback restores all rows. This is correct behavior — the cycle appears purgeable again on the next tick. The risk is if rollback is silent and the implementation proceeds to `mark_signals_purged()` despite the rollback, setting `raw_signals_available = 0` while the data still exists.

**Test Scenarios**:
1. Simulate a transaction error after observations delete but before sessions delete. Verify the cycle's observations are still present after rollback (idempotency — next tick would retry).
2. Verify that `mark_signals_purged()` is NOT called if `gc_cycle_activity()` returns an error.
3. Integration test: run GC twice on the same set of purgeable cycles. Assert zero rows affected on the second run (NFR-04 idempotency — AC not explicitly numbered but implied by NFR-04).

**Coverage Requirement**: Error propagation from gc_cycle_activity() must be tested — the mark_signals_purged call must be conditional on success.

---

### R-07: Unattributed Prune Deletes Active Session Observations

**Severity**: Med
**Likelihood**: Med
**Impact**: An agent writes observations mid-session. The session has `feature_cycle IS NULL` (not yet attributed to a cycle). The unattributed cleanup runs and deletes those observations, breaking the in-flight retrospective pipeline which depends on raw observations to compute the cycle review. The architecture resolves this with the Active status guard (SR-06), but if the SQL predicate is wrong, Active sessions are pruned.

**Test Scenarios**:
1. Insert a session with `feature_cycle IS NULL` and `status = 'Active'` plus observations. Run GC. Assert observations still exist (AC-06 Active guard).
2. Insert a session with `feature_cycle IS NULL` and `status = 'Closed'` plus observations. Run GC. Assert observations are deleted (AC-06 closed unattributed path).
3. Verify the SQL for `gc_unattributed_activity()` contains `status != 'Active'` or equivalent discriminant guard.

**Coverage Requirement**: Both the Active (must survive) and Closed (must be pruned) cases must be tested in the same test scenario to confirm the predicate distinguishes them correctly.

---

### R-08: max_cycles_per_tick Cap Not Enforced

**Severity**: Med
**Likelihood**: Med
**Impact**: On first deployment, hundreds of historical cycles may be purgeable. Without the cap, the tick processes all of them sequentially, each requiring write pool acquisition. The tick monopolizes the background thread for minutes. Subsequent write operations queue behind GC work. Historical precedent: ADR-002 documents this scenario explicitly with 200 purgeable cycles.

**Test Scenarios**:
1. Insert N = 20 purgeable cycles. Set `max_cycles_per_tick = 5`. Run one GC tick. Assert exactly 5 cycles pruned, 15 remain purgeable (AC-16 first tick).
2. Run a second tick. Assert 5 more cycles pruned (10 remain).
3. Run four ticks total. Assert all 20 cycles pruned.
4. Assert oldest cycles (lowest `computed_at`) are processed first, not arbitrary order.

**Coverage Requirement**: The ordering assertion (oldest first) is as important as the count assertion. Processing newest-first would prune the most recent learning data first, defeating the retention window semantics.

---

### R-09: Two-Hop Subquery Full-Table Scan on observations

**Severity**: Med
**Likelihood**: Med
**Impact**: The DELETE subquery `WHERE session_id IN (SELECT session_id FROM sessions WHERE feature_cycle = ?)` must use `idx_observations_session` to stay performant at 152 MB scale. If the query planner chooses a full-table scan (e.g., because statistics are stale, or the index is not recognized for a DELETE), each per-cycle GC iteration scans 152 MB. With 10 cycles per tick, this is 1.5 GB of I/O per maintenance pass, which will degrade the server noticeably. This is the assumption flagged in the SCOPE-RISK-ASSESSMENT assumptions section.

**Test Scenarios**:
1. `EXPLAIN QUERY PLAN` for the observations DELETE subquery must reference `idx_observations_session` (not "SCAN observations") — NFR-03 assertion.
2. Same EXPLAIN check for the query_log DELETE subquery against `idx_query_log_session`.
3. Integration test must fail with a diagnostic message if either query plan shows a full-table scan.

**Coverage Requirement**: EXPLAIN QUERY PLAN assertions are mandatory. They are the only way to verify index usage without running at production data scale.

---

### R-10: RetentionConfig validate() Missing max_cycles_per_tick = 0 Check

**Severity**: Med
**Likelihood**: Med
**Impact**: `max_cycles_per_tick = 0` means the GC processes zero cycles per tick, silently becoming a no-op while reporting no error. The observations table grows unbounded with no indication of the misconfiguration. Entry #3766 (InferenceConfig validate() gap rated as blocking by human reviewer) shows this class of omission has historically required rework at review time.

**Test Scenarios**:
1. `RetentionConfig { max_cycles_per_tick: 0, .. }.validate()` returns `Err(_)` with message containing `"max_cycles_per_tick"` (AC-12b).
2. `RetentionConfig { max_cycles_per_tick: 1001, .. }.validate()` returns `Err(_)` (upper bound check, range [1, 1000]).
3. `RetentionConfig { activity_detail_retention_cycles: 0, .. }.validate()` returns `Err(_)` (AC-11).
4. `RetentionConfig { audit_log_retention_days: 0, .. }.validate()` returns `Err(_)` (AC-12).
5. All three fields at valid boundary values (1, 1, 1) must pass validate() without error.

**Coverage Requirement**: All three validate() paths tested independently. Upper bounds tested as well as lower bounds. Entry #2577 confirms boundary tests must ship in the same implementation pass.

---

### R-11: PhaseFreqTable Mismatch Warning Fires on Inverted Condition

**Severity**: Low
**Likelihood**: Med
**Impact**: ADR-003 specifies the warning fires when `oldest_retained_computed_at <= lookback_cutoff`. The inverse condition (`oldest_retained_computed_at > lookback_cutoff`) represents sufficient data coverage — no warning needed. An inverted predicate would either: (a) warn on every tick regardless of data coverage (noise), or (b) silently suppress warnings when data IS truncated (missed diagnostic). The imprecision of the `computed_at` proxy makes this risk plausible.

**Test Scenarios**:
1. Configure K = 5 cycles all reviewed within the past 7 days; set `query_log_lookback_days = 365`. Assert a `warn` log event is emitted containing `"query_log_lookback_days"` and `"retention window"` (AC-17).
2. Configure K = 5 cycles reviewed within the past 7 days; set `query_log_lookback_days = 3`. Assert no `warn` event is emitted for mismatch (sufficient coverage case).
3. Verify the check is skipped (no warning) when `cycle_review_index` has fewer than K rows.

**Coverage Requirement**: Both the warning-fires and warning-suppressed cases are tested to distinguish a correct predicate from an inverted one.

---

### R-12: gc_audit_log Timestamp Unit Mismatch (Millis vs Seconds)

**Severity**: Med
**Likelihood**: Med
**Impact**: `audit_log.timestamp` is stored in Unix seconds (per db.rs schema). The GC query computes `strftime('%s', 'now') - retention_days * 86400`. If the implementation accidentally stores or compares in milliseconds (a common confusion given that `observations.ts_millis` is in milliseconds), two failure modes arise: (a) nothing is deleted (cutoff is in the distant past), or (b) all rows are deleted regardless of age (cutoff is in the far future). Either corrupts the audit log.

**Test Scenarios**:
1. Insert `audit_log` rows with `timestamp = now_unix_secs - (200 * 86400)` (200 days ago in seconds). Run GC with `audit_log_retention_days = 180`. Assert rows are deleted (AC-09 old rows pruned).
2. Insert `audit_log` rows with `timestamp = now_unix_secs - (100 * 86400)` (100 days ago in seconds). Run GC with `audit_log_retention_days = 180`. Assert rows are NOT deleted (AC-09 recent rows preserved).
3. Assert the GC query does NOT multiply by 1000 anywhere in the audit_log DELETE path.

**Coverage Requirement**: Both sides of the retention boundary must be tested. Testing only deletion (not preservation) would not catch a unit error that deletes everything.

---

### R-14: Protected Tables Touched by GC

**Severity**: High
**Likelihood**: Low
**Impact**: `entries`, `GRAPH_EDGES`, `cycle_events`, `cycle_review_index` (rows deleted, not updated), and `observation_phase_metrics` must not be touched by the GC pass. Accidental deletion of entries or graph edges would degrade the knowledge base irreversibly. An SQL typo (wrong table name in a DELETE) is the most likely cause.

**Test Scenarios**:
1. Pre-GC snapshot: count rows in entries, GRAPH_EDGES, co_access, cycle_events, cycle_review_index, observation_phase_metrics.
2. Run GC with purgeable cycles present.
3. Post-GC: assert each protected table count is identical to pre-GC snapshot (AC-03).
4. Insert one row in each protected table before GC. Assert each row survives GC unchanged (AC-14 targeted check).

**Coverage Requirement**: Row-level verification for protected tables (not just count) confirms no accidental partial deletion.

---

### R-15: RetentionConfig Defaults Not Applied When [retention] Block Absent

**Severity**: Med
**Likelihood**: Low
**Impact**: If `#[serde(default)]` is missing from `RetentionConfig` or its individual fields, an absent `[retention]` block in `config.toml` causes a deserialization error at startup (not a graceful default), or silently initializes fields to 0 (invalid values that validate() should catch — but may not if validate() is called after the zero-initialization).

**Test Scenarios**:
1. Parse a `config.toml` with no `[retention]` section. Assert `RetentionConfig` fields are `activity_detail_retention_cycles = 50`, `audit_log_retention_days = 180`, `max_cycles_per_tick = 10` (AC-10 absent block test).
2. Parse a `config.toml` with explicit `[retention]` values different from defaults. Assert explicit values are applied.
3. `RetentionConfig::default()` unit test: assert all three fields match documented defaults (AC-10 unit test).

**Coverage Requirement**: The absent-block case must be an integration test with a real TOML parse, not just a unit test of `Default::default()`. Serde behavior on absent sections differs from Rust's Default trait.

---

### R-16: oldest_retained_computed_at Query Returns Wrong Boundary

**Severity**: Low
**Likelihood**: Med
**Impact**: ADR-003 requires retrieving the K-th most recent cycle's `computed_at` to compute the PhaseFreqTable alignment check. If the query retrieves the K+1th or the most recent (1st) instead, the warning either fires too early (false positive) or never fires (false negative). This is a query offset/limit error.

**Test Scenarios**:
1. Insert exactly K cycles. Verify the oldest retained boundary is the K-th cycle's `computed_at`, not an out-of-range value.
2. Insert fewer than K cycles. Verify the check is skipped (no warning, no error).
3. Correlate: when the AC-17 mismatch warning test passes (R-11 scenario 1), verify the `oldest_retained_cycle_computed_at` value in the log matches the actual K-th cycle's timestamp.

**Coverage Requirement**: The boundary between K and K+1 cycles must be explicitly tested to confirm off-by-one is not present.

---

## Integration Risks

### Two-hop join atomicity within transaction

The observations and query_log DELETEs use subqueries referencing `sessions`. Within a per-cycle transaction, `sessions` rows for the cycle are deleted last. The DELETE statements for observations and query_log execute while `sessions` rows are still present — this is correct. However, if sessions rows are deleted first (R-02), the subquery resolves to an empty set and observations/query_log rows survive. The transaction boundary defines a valid execution window; the delete order within it is mandatory.

### list_purgeable_cycles reads cycle_review_index; GC writes it

`list_purgeable_cycles()` uses `read_pool()` while `mark_signals_purged()` uses `write_pool_server()`. In SQLite WAL mode, a reader and writer can proceed concurrently without blocking. However, if `list_purgeable_cycles()` and `mark_signals_purged()` run against the same cycle in rapid succession on separate ticks, the `raw_signals_available` update from tick N may not be visible to the read_pool reader on tick N+1 until the WAL checkpoint. This is not a correctness risk (the cycle would not appear in the purgeable list again since its sessions are gone), but it may cause a spurious "cycle has no sessions" state.

### Background tick step ordering: step 4 before step 5

Step 4 (cycle GC) runs before step 5 (stale session sweep) and step 6 (gc_sessions time-based). If step 6 runs and deletes sessions by time-boundary before step 4's unattributed cleanup runs, those sessions are already gone — the unattributed cleanup's `NOT IN (SELECT session_id FROM sessions)` query correctly picks up any residual observation/query_log rows whose sessions were deleted by step 6. The ordering is safe, but the interaction must be understood: step 6 is a partial substitute for unattributed cleanup, not a conflict.

---

## Edge Cases

- **Exactly K reviewed cycles, zero purgeable**: `list_purgeable_cycles(k=50)` with exactly 50 rows in `cycle_review_index` returns empty list. GC exits immediately. No deletes. This must not emit errors or warnings (it is the steady-state after catch-up).
- **K = 1**: Only the most recently reviewed cycle is retained. All others are purgeable. Verify the NOT IN subquery with LIMIT 1 is syntactically valid in SQLite and returns correct results.
- **Zero cycles reviewed** (fresh deployment): `cycle_review_index` is empty. `list_purgeable_cycles()` returns empty. GC is a no-op. No warnings.
- **Cycle with sessions but zero observations**: GC deletes the session and injection_log rows, `observations_deleted = 0`. Must not fail or error — zero-rows-affected is a valid result.
- **Session with feature_cycle referencing a non-existent cycle_review_index**: This is the normal state for open cycles. The purgeable set query only returns cycles that DO have a `cycle_review_index` row, so sessions with an unreviewed cycle are never in the purgeable set. No separate guard needed.
- **audit_log with timestamp = 0**: A row with `timestamp = 0` (epoch) is extremely old. It must be deleted by any `audit_log_retention_days` value in [1, 3650]. Confirm the boundary handles timestamp = 0 without overflow.
- **max_cycles_per_tick = 1 with 1000 purgeable cycles**: Verify the GC processes exactly one cycle per tick without crashing or resetting the purgeable list to include already-pruned cycles.
- **Concurrent retention config read at startup**: If two goroutines/tasks both reach `validate()` simultaneously (unlikely but possible in test harness), the config is already immutable by the time tick runs (loaded once, passed by value). No shared mutation risk.

---

## Security Risks

### What untrusted input enters this feature?

- `config.toml` `[retention]` block: operator-controlled. Values validated by `validate()` at startup before the server accepts connections. Range checks prevent extreme values that could deny service (e.g., `max_cycles_per_tick = 1000000`).
- `feature_cycle` strings passed as SQL bind parameters to all GC queries. These originate from `cycle_review_index.feature_cycle` which was written by the MCP tool handler at cycle creation time. The GC does not accept user-supplied cycle IDs at query time — it reads them from the database.

### Injection risk

All GC SQL uses parameterized queries (`sqlx` bind parameters). No string interpolation of `feature_cycle` values into SQL. The two-hop subquery does not accept external input at GC time — the cycle list is resolved from `cycle_review_index` and passed as a bound parameter to each statement. No SQL injection vector.

### Blast radius if this component is compromised

The GC has DELETE authority on `observations`, `query_log`, `injection_log`, `sessions`, and `audit_log`. A bug that allows an attacker to influence the `feature_cycle` bind parameter (e.g., by writing a malicious cycle ID into `cycle_review_index`) could trigger deletion of arbitrary sessions. However, write access to `cycle_review_index` already requires MCP Admin-level capability, so this is a second-order risk requiring prior privilege escalation. The targeted UPDATE to `cycle_review_index.raw_signals_available` has no code path that accepts external input.

### Deletion of accountability records

`audit_log` deletion is governed by `audit_log_retention_days`. Since this is operator-configurable with a default of 180 days, an operator setting `audit_log_retention_days = 1` would delete almost all audit history. The `validate()` lower bound of 1 day prevents `0` but does not prevent aggressive values. This is a known operational trade-off, not a security bug.

---

## Failure Modes

### GC pass finds no purgeable cycles

Expected behavior: `list_purgeable_cycles()` returns empty vec. Pass exits after logging `purgeable_count = 0`. No deletes. No warnings. The audit_log cleanup still runs. This is the normal steady-state on a healthy deployment.

### get_cycle_review() returns Err on a purgeable cycle

Per FR-04: skip the cycle, emit `tracing::warn!` with cycle ID and error, continue to next cycle. The pass does not abort. Subsequent cycles in the same tick are processed normally. The failed cycle is retried on the next tick.

### gc_cycle_activity() transaction fails (e.g., SQLite constraint error)

Per-cycle transaction rolls back. Observations, query_log, injection_log, sessions for that cycle are all restored. `mark_signals_purged()` must NOT be called if the transaction failed. The cycle reappears as purgeable on the next tick. The GC log should record the error at `warn` level. This is the NFR-04 idempotency guarantee from the other direction.

### Server crashes after gc_cycle_activity() commits but before mark_signals_purged()

Documented consequence in ADR-001: `raw_signals_available` stays 1 for a pruned cycle. The retrospective report record remains valid (summary_json intact). On the next tick, `list_purgeable_cycles()` queries `cycle_review_index` — the cycle's sessions are gone, so the cycle will not produce activity data deletions again, but the flag inconsistency persists. Accepted as low-severity; a future consistency scan could detect and repair `raw_signals_available = 1` rows with no associated sessions.

### Server crashes mid-transaction (observations deleted, sessions not yet)

SQLite rolls back the partial transaction on restart. The cycle appears purgeable on the next tick. All rows are restored. This is the ADR-001 crash safety guarantee of per-cycle transactions.

### Configuration validation fails at startup

`validate()` returns `Err`. Server aborts startup with a structured error message naming the offending field and its invalid value. No GC pass runs. This is the correct behavior — a misconfigured retention window should not silently default to zero.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (write pool stall from long-running observations DELETE) | R-04, R-09 | ADR-001: per-cycle transactions release connection between cycles. NFR-03: index verification via EXPLAIN QUERY PLAN. |
| SR-02 (write pool deadlock if drain task holds connection) | R-04 | ADR-001: per-cycle transactions ensure write pool is released between cycles. Drain task interleaves with GC. |
| SR-03 (raw BEGIN/COMMIT risks silent data loss in sqlx) | R-04 | Architecture constraint 5: pool.begin()/tx.commit() API required. Verified by code review in R-04 scenario 3. |
| SR-04 (tools.rs 60-day DELETE site overlooked during delivery) | R-01 | Specification AC-01a and AC-01b are independent verifiable line items. R-01 requires two separate grep assertions, one per file. |
| SR-05 (INSERT OR REPLACE clobbers summary_json) | R-03 | Architecture uses targeted UPDATE. mark_signals_purged() verified via summary_json preservation check in R-03 scenario 1. |
| SR-06 (unattributed prune deletes Active session observations) | R-07 | Architecture adds Active status guard. R-07 tests both Active (must survive) and Closed (must be pruned) paths. |
| SR-07 (PhaseFreqTable silent truncated window) | R-11, R-16 | ADR-003: tick-time tracing::warn!. R-11 tests warning fires and suppresses correctly. R-16 tests boundary accuracy. |
| SR-08 (K-window never advances if retro never called) | — | Accepted as documented operational constraint (AC-04 note). No code mitigation; operator must call context_cycle_review. |
| SR-09 (RetentionConfig re-read each tick) | R-15 | NFR-06: config loaded once at startup, passed by value. Code review confirms no per-tick config.toml read. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-01, R-02, R-04) | 8 scenarios minimum |
| High | 8 (R-03, R-05, R-07, R-08, R-09, R-10, R-12, R-14) | 20 scenarios minimum |
| Medium | 4 (R-06, R-11, R-15, R-16) | 9 scenarios minimum |
| Low | 1 (R-13) | 1 scenario (idempotency re-run) |

**Non-negotiable test coverage (Gate 3c blockers)**:
1. Two independent grep assertions for both 60-day DELETE sites (R-01)
2. Cascade order mutation test — invert sessions/injection_log order and confirm test failure (R-02)
3. summary_json preservation check alongside raw_signals_available = 0 (R-03)
4. EXPLAIN QUERY PLAN assertions for observations and query_log DELETE subqueries (R-09)
5. max_cycles_per_tick cap multi-tick drain test (R-08)
6. Active-session unattributed guard (R-07)
7. validate() boundary tests for all three RetentionConfig fields (R-10)
8. audit_log timestamp unit test — both sides of retention boundary (R-12)

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `"lesson-learned failures gate rejection"` — found entries #3579 (test omission at delivery wave), #2758 (Gate 3c non-negotiable test grep), #2577 (validate() boundary tests must ship with implementation). Applied to R-01 (dual-site grep requirement), R-10 (validate boundary tests), and non-negotiable coverage list.
- Queried: `/uni-knowledge-search` for `"SQLite write pool connection transaction background tick"` — found entries #2249 (write pool deadlock lesson-learned), #2269 and #2159 (manual BEGIN/COMMIT connection identity). Applied to R-04 evidence.
- Queried: `/uni-knowledge-search` for `"cycle_review_index observations deletion session cascade"` — found entry #3914 (two-hop join pattern for crt-036) and #3793 (crt-033 ADR-001 write_pool_server constraint). Applied to R-02 and R-03.
- Queried: `/uni-knowledge-search` for `"boundary test validate config range check startup abort"` — found entry #3766 (InferenceConfig validate gap rated blocking by human reviewer). Applied to R-10 severity elevation.
- Stored: nothing novel to store — all patterns identified here (per-cycle transaction, two-hop join, validate boundary tests) are already captured in entries #2249, #2159, #3914. The audit_log timestamp unit risk (R-12) is feature-specific and not a recurrent pattern across 2+ features yet.
