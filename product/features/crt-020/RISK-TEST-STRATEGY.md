# Risk-Based Test Strategy: crt-020

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Vote inflation via injection_log multi-row per (session_id, entry_id) — dedup HashSet not populated correctly | High | Med | Critical |
| R-02 | Stop hook write path does not set `implicit_votes_applied = 1` on all session close code paths — background tick processes real-time sessions | High | Low | High |
| R-03 | Migration v12→v13 `ALTER TABLE` guard missing or incorrect — existing sessions reset or migration fails on re-open | High | Low | High |
| R-04 | `apply_implicit_votes` marks sessions applied before votes are written — crash between mark and vote leaves sessions silently unprocessed | High | Low | High |
| R-05 | Confidence snapshot (alpha0/beta0) taken inside `spawn_blocking` holding RwLock across await point — deadlock or stale prior | Med | Med | High |
| R-06 | `ended_at IS NULL` sessions with valid outcome never processed — `ORDER BY ended_at ASC NULLS LAST` places them permanently at the end | Med | Med | High |
| R-07 | Cold-start drain starves newly-closed sessions: oldest-first ordering delays recent sessions for 1–N ticks during backlog | Med | High | Med |
| R-08 | `scan_injection_log_by_sessions` chunking (50 session IDs per IN clause) produces partial result if chunk boundary splits a session's rows | Med | Low | Med |
| R-09 | Inline confidence recomputation in `apply_implicit_votes` uses stale prior when `ConfidenceStateHandle` is updated between snapshot and computation | Med | Low | Med |
| R-10 | `TimedOut` sessions with non-NULL outcome processed if SQL filter uses `status IN (1, 2)` — violates resolved decision (zero signal for TimedOut) | Med | Med | Med |
| R-11 | `gc_sessions` races with `apply_implicit_votes` if ordering is violated — session deleted mid-processing yields lost injection_log rows | Med | Low | Med |
| R-12 | Non-success sessions (rework, abandoned) silently receive `implicit_votes_applied = 1` with zero votes — implementation misses the outcome-filter branch and passes them to helpful_ids | Med | Low | Med |
| R-13 | Signal quality dilution: sessions injecting many entries (e.g., 20+) credit all entries equally — low-utility entries gain votes as fast as high-utility ones | Low | High | Low |
| R-14 | `tick_timeout` breach when cold-start batch processes 500 sessions with high injection rates — inline confidence recomputation at 2,500+ entries exceeds remaining tick budget | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Vote Inflation from Injection_Log Multi-Row Dedup
**Severity**: High
**Likelihood**: Med
**Impact**: An entry injected 5 times in one session receives 5 helpful votes instead of 1. Confidence scores for frequently-re-injected entries inflate artificially. High-injection entries dominate rankings for the wrong reason. Historical precedent: Unimatrix #1044 — a COUNT DISTINCT bug in crt-018 was caught at implementation time by the risk strategy; this is the analogous risk for crt-020.

**Test Scenarios**:
1. Insert 5 injection_log rows for `(session_A, entry_X)`. Run `apply_implicit_votes` for `session_A` (outcome = "success"). Assert `helpful_count` for entry_X incremented by exactly 1 (not 5).
2. Insert injection_log rows: `(session_A, entry_X)` × 3 and `(session_A, entry_Y)` × 2. Run tick. Assert entry_X `helpful_count += 1`, entry_Y `helpful_count += 1`.
3. Mix: entry_X appears in both session_A (success) and session_B (success). Run both sessions through tick. Assert `helpful_count` incremented by 2 (one per session, correctly).
4. Session with `outcome = "rework"`: verify no votes applied and session marked `implicit_votes_applied = 1` (zero signal confirmed).

**Coverage Requirement**: The HashSet dedup in `apply_implicit_votes` must be verified. At least one test must assert the exact count after multiple injection_log rows for the same entry_id. R-01.4 confirms rework sessions produce strictly zero votes.

---

### R-02: Stop Hook Missing `implicit_votes_applied = 1` on All Close Paths
**Severity**: High
**Likelihood**: Low
**Impact**: Double counting: a session receives real-time votes from `run_confidence_consumer` AND background votes from the tick. Entry confidence inflates by ~2x for sessions that close via Stop hook. The core dedup invariant of ADR-003 breaks.

**Test Scenarios**:
1. Simulate a normal Stop hook close: write `SessionRecord` with `implicit_votes_applied = true`. Assert the sessions table has `implicit_votes_applied = 1`. Run `query_sessions_pending_implicit_votes` — assert the session does NOT appear.
2. Verify all code paths in `listener.rs` that write a session close set `implicit_votes_applied = 1`: normal Stop, sweep-triggered close, abandoned close. Use test DB and each close path.
3. Write session with `implicit_votes_applied = 0`, run tick, assert votes applied and flag set to 1. Then manually invoke `run_confidence_consumer` equivalent. Assert `helpful_count` did NOT increment a second time.
4. Session written with `implicit_votes_applied = 1` by Stop hook path. Run tick. Assert `query_sessions_pending_implicit_votes` returns 0 results for that session.

**Coverage Requirement**: Every session write path in `listener.rs` and `sessions.rs` that represents a session close must set the flag. A grep-level audit of all `insert_session` / `update_session` call sites at session-close time is required.

---

### R-03: Migration v12→v13 Guard Correctness
**Severity**: High
**Likelihood**: Low
**Impact**: If the `ALTER TABLE sessions ADD COLUMN implicit_votes_applied` guard is absent or incorrect, re-opening a v13 database tries to add the column again and fails. Alternatively, a fresh database could skip the column entirely. Either breaks session persistence or causes startup failure.

**Test Scenarios**:
1. Open a fresh database (no entries table). Assert migration no-ops. Open again. Assert no error.
2. Create a v12 database (sessions table without `implicit_votes_applied`). Run `migrate_if_needed`. Assert column exists, `schema_version = 13`, all existing session rows have `implicit_votes_applied = 0`.
3. Run migration on an already-v13 database. Assert idempotent — no error, no duplicate column.
4. Insert 10 sessions into a v12 database. Migrate to v13. Assert all 10 sessions have `implicit_votes_applied = 0`.
5. Assert v13 migration creates exactly: one new column on `sessions` and one new index (`idx_sessions_pending_votes`). No new tables. The `implicit_unhelpful_pending` table must NOT exist in v13.

**Coverage Requirement**: Migration must be idempotent (run twice, no error). v12→v13 must preserve existing session data with correct defaults. Both DDL statements (column and index) must be guarded. R-03.5 explicitly asserts the removed table is absent.

---

### R-04: Sessions Marked Applied Before Votes Written
**Severity**: High
**Likelihood**: Low
**Impact**: If the implementation calls `mark_implicit_votes_applied` before `record_usage_with_confidence`, a crash between them leaves sessions permanently flagged as processed with no votes applied. Entries never receive the signal they earned. Silent data loss.

**Test Scenarios**:
1. Instrument `apply_implicit_votes` to panic after `mark_implicit_votes_applied` but before `record_usage_with_confidence`. Assert that after recovery, the session has `implicit_votes_applied = 1` but `helpful_count` for its entries is 0. This is the expected (if unfortunate) behavior — document it explicitly.
2. Verify that the implementation calls vote writes BEFORE marking sessions applied: code review / integration test asserting `record_usage_with_confidence` completes without error before `mark_implicit_votes_applied`.
3. Happy path: run full tick with 5 success sessions. Assert all have `implicit_votes_applied = 1` AND all entries have `helpful_count > 0`.
4. Run tick with a session whose entry_ids no longer exist (deleted entries). Assert session is still marked applied (AC-09: skip missing entries silently). Assert no error returned.

**Coverage Requirement**: Operation ordering must be verified in integration tests via assertion sequence: votes written → flag set. The crash-between scenario must be documented as a known limitation rather than a silent surprise.

---

### R-05: Confidence Prior Snapshot Lock Pattern
**Severity**: Med
**Likelihood**: Med
**Impact**: If `alpha0`/`beta0` are read inside `spawn_blocking` (after the sync boundary), the RwLock on `ConfidenceStateHandle` is held in a blocking context. If the snapshot captures a reference to the guard (not the values), the borrow could outlive the guard. The ADR-004 pattern specifies values must be copied, not the guard.

**Test Scenarios**:
1. Verify via compilation and test that `alpha0` and `beta0` are `f64` values (Copy types) captured by the closure, not references to `ConfidenceState`.
2. Update `ConfidenceStateHandle` (change alpha0) after snapshot but before `spawn_blocking` returns. Assert the vote computation used the pre-update snapshot (closure captured the old value).
3. Test with a `ConfidenceStateHandle` that panics on `read()`. Assert `unwrap_or_else(|e| e.into_inner())` recovers the poisoned lock and continues.
4. Run `apply_implicit_votes` concurrently with a `ConfidenceStateHandle` write. Assert no deadlock within 5 seconds.

**Coverage Requirement**: The snapshot pattern (read values, drop guard, enter spawn_blocking) must be enforced at implementation. Test with concurrent prior updates.

---

### R-06: NULL `ended_at` Sessions Never Processed
**Severity**: Med
**Likelihood**: Med
**Impact**: Sessions closed via `sweep_stale_sessions` may have `ended_at = NULL` if the sweep path does not set it. `ORDER BY ended_at ASC NULLS LAST` places these permanently after all timestamped sessions. During normal operation they sit at the back of the queue indefinitely. After each tick processes the timestamped sessions, they surface — but only if the batch cap isn't consumed by timestamped sessions ahead of them.

**Test Scenarios**:
1. Insert a session with `ended_at = NULL`, `outcome = "success"`, `status = Completed`, `implicit_votes_applied = 0`. Insert 100 sessions with valid `ended_at`. Set `IMPLICIT_VOTE_BATCH_LIMIT = 100`. Run tick. Assert the NULL-ended_at session was NOT processed (batch consumed by timestamped sessions). Run a second tick. Assert it IS processed.
2. Insert only NULL-ended_at sessions. Run tick. Assert all are processed.
3. Verify that the sweep path (`sweep_stale_sessions`) writes a non-NULL `ended_at` when marking sessions Completed/TimedOut. If it does, R-06 is mitigated at source — document finding.

**Coverage Requirement**: NULLS LAST behavior must be explicitly tested. The sweep path must be verified to write `ended_at`.

---

### R-07: Cold-Start Starvation of Recent Sessions
**Severity**: Med
**Likelihood**: High
**Impact**: During first tick after v13 upgrade, historical sessions fill the batch cap (500). Newly-closed sessions from the current day queue behind 30 days of history. For up to N ticks (N = ceil(backlog/500)), confidence scores for recently-used entries do not receive their implicit votes. This is documented in ADR-002 as accepted behavior, but the test must verify the drain completes correctly, not just that it starts.

**Test Scenarios**:
1. Insert 1,200 sessions (all with `implicit_votes_applied = 0`, `status = Completed`, varied `ended_at`). Run 3 ticks (each processes 500, 500, 200). Assert after tick 3 all sessions have `implicit_votes_applied = 1`.
2. Insert 200 old sessions and 10 new sessions. After tick 1 (batch = 200, all fit), assert all 210 sessions processed. Verify new sessions are included.
3. Insert 600 sessions. Run 1 tick (limit 500). Assert oldest 500 (by `ended_at ASC`) processed. Assert newest 100 still have `implicit_votes_applied = 0`.

**Coverage Requirement**: Multi-tick drain correctness. The batch cap must advance the watermark without skipping sessions.

---

### R-08: Injection Log Chunk Boundary Split
**Severity**: Med
**Likelihood**: Low
**Impact**: `scan_injection_log_by_sessions` processes 50 session IDs per IN clause chunk. If the chunking implementation has an off-by-one that omits session IDs at chunk boundaries (e.g., session 50 is the last in chunk 1 but first in chunk 2 and is processed twice, or omitted), votes are doubled or dropped.

**Test Scenarios**:
1. Insert exactly 50 sessions with injection_log entries, then 1 more (51 total). Run scan. Assert all 51 sessions' entries are returned.
2. Insert 100 sessions (2 full chunks). Run scan. Assert all 100 sessions' entries returned with no duplicates.
3. Insert 1 session with 200 injection_log rows. Chunk size is for session IDs, not rows. Assert all 200 rows returned for that single session.
4. Empty session list passed to `scan_injection_log_by_sessions`. Assert returns empty result, no error.

**Coverage Requirement**: Chunk boundaries at exactly 50 and 100 sessions must be explicitly tested.

---

### R-09: Stale Prior in Confidence Closure
**Severity**: Med
**Likelihood**: Low
**Impact**: `alpha0`/`beta0` are snapshotted once before `spawn_blocking`. If the empirical prior update (crt-019 `run_maintenance` step 2b) fires concurrently and updates the handle, the implicit vote step uses the pre-update prior for confidence recomputation. Bounded to a 15-minute staleness window. However, if the prior changes drastically (e.g., first-ever empirical update fires mid-cold-start), confidence values computed for hundreds of entries use outdated parameters.

**Test Scenarios**:
1. Set `alpha0 = 1.0, beta0 = 1.0`. Take snapshot. Update handle to `alpha0 = 2.0, beta0 = 3.0`. Run `apply_implicit_votes` with the snapshot. Assert confidence computed using `alpha0 = 1.0, beta0 = 1.0` (the snapshot values, not the updated handle).
2. Assert the `record_usage_with_confidence` call within one tick uses the same snapshot — not an independent read from the handle.

**Coverage Requirement**: Single-snapshot pattern verified. The single call within one `apply_implicit_votes` invocation must use the snapshotted prior values, not the live handle.

---

### R-10: TimedOut Sessions with Non-NULL Outcome Processed
**Severity**: Med
**Likelihood**: Med
**Impact**: ADR-002 SQL uses `status = 1 (Completed)` only, excluding `TimedOut`. But ARCHITECTURE.md Open Question 3 notes ambiguity — some implementations might use `status IN (1, 2)` per the SCOPE.md AC-02 wording. If the filter includes TimedOut sessions that happen to have a non-NULL outcome, they receive implicit votes contrary to the resolved decision (zero signal for TimedOut, SCOPE.md resolved decision #4).

**Test Scenarios**:
1. Insert a session with `status = TimedOut (2)`, `outcome = "success"`, `implicit_votes_applied = 0`. Run tick. Assert session NOT processed (no votes applied, `implicit_votes_applied` still 0).
2. Insert sessions with `status = Completed (1)`, `outcome = "success"` and `status = TimedOut (2)`, `outcome = "success"`. Run tick. Assert only the Completed session processed.
3. Assert `query_sessions_pending_implicit_votes` SQL uses `status = 1` (not `status IN (1, 2)`). Verify via test that queries the raw SQL or via behavioral test with TimedOut sessions.

**Coverage Requirement**: The exact SQL filter must be verified — not just the happy-path behavior.

---

### R-11: GC Ordering Race
**Severity**: Med
**Likelihood**: Low
**Impact**: If `apply_implicit_votes` runs before `gc_sessions` in a tick (wrong ordering), GC may delete a session that `apply_implicit_votes` just queried but not yet processed. The injection_log rows for that session are cascade-deleted by GC. `apply_implicit_votes` then tries to scan injection_log for the deleted session and finds no rows — silently applying zero votes. The session was not marked applied (it was deleted), so no flag is left. The entry receives no signal for that session.

**Test Scenarios**:
1. Verify in `maintenance_tick` (or equivalent) that `run_maintenance` (which includes `gc_sessions`) completes before `run_implicit_vote_tick` is called. Assert via execution order in integration test.
2. Insert a session older than `DELETE_THRESHOLD_SECS`. Run `gc_sessions`. Assert session deleted. Run `apply_implicit_votes`. Assert no error and processed count = 0 for that session.
3. Insert a session exactly at the GC threshold boundary. Run GC then tick. Verify the session's fate is consistent — either deleted (no votes, no error) or processed (votes applied, flag set).

**Coverage Requirement**: Tick step ordering must be an explicit test assertion, not just a code-review check.

---

### R-12: Rework/Abandoned Sessions Silently Included in Helpful IDs
**Severity**: Med
**Likelihood**: Low
**Impact**: The v1 simplification (ADR-001) removes all branching on rework/abandoned outcomes — only `"success"` sessions generate helpful_ids. A missing or incorrect outcome filter in `apply_implicit_votes` (e.g., iterating all sessions without checking `outcome`) passes non-success entries into `helpful_ids`. Entries injected during failed sessions receive spurious helpful votes. Confidence scores for unreliable entries inflate without attribution.

**Test Scenarios**:
1. Batch contains session_A (`outcome = "success"`, entry_X, entry_Y) and session_B (`outcome = "rework"`, entry_X, entry_Z). Run `apply_implicit_votes`. Assert entry_X `helpful_count += 1` (from session_A only, not doubled). Assert entry_Z `helpful_count` unchanged. Assert entry_Y `helpful_count += 1`.
2. Batch contains only rework sessions. Assert `helpful_count` unchanged for all entries in those sessions.
3. Batch contains only abandoned sessions. Assert same — zero helpful votes applied.
4. Verify the implementation outcome-filter branch: read the code to confirm the `match`/`if` on `outcome == "success"` is present and covers all non-success variants.

**Coverage Requirement**: The outcome filter must be tested with mixed-outcome batches in a single tick. Non-success sessions must never contribute to `helpful_ids`.

---

## Integration Risks

**I-01: `record_usage_with_confidence` SQLite BUSY under concurrent load.**
The background tick and real-time `run_confidence_consumer` both use `BEGIN IMMEDIATE`. SQLite WAL mode serializes writers — the tick holds the write lock for the duration of `record_usage_with_confidence` (potentially 250ms for a full batch). Concurrent MCP `context_store`, `context_correct`, or explicit-vote calls during that window receive `SQLITE_BUSY`. The existing rusqlite setup uses a busy timeout — verify it is set and sufficient (typically 5000ms).

**I-02: `sweep_stale_sessions` setting `implicit_votes_applied` value.**
The sweep path marks sessions `TimedOut` or `Completed`. It must either set `implicit_votes_applied = 0` (default, correct) or leave it unset (relying on DEFAULT). If the sweep path writes an explicit `1`, swept sessions skip background processing — they had no Stop hook signal, so their entries get no vote. The sweep close path must not set `implicit_votes_applied = 1`.

**I-03: `run_confidence_consumer` draining signal_queue for swept sessions.**
Swept sessions (orphaned, no Stop hook) go into background tick processing with `implicit_votes_applied = 0`. But if `sweep_stale_sessions` also writes to `signal_queue` (verify it does not), those sessions could receive both real-time and background votes. The architecture states only Stop-hook-triggered sessions generate signal_queue entries — verify this is true for swept sessions.

**I-04: `apply_implicit_votes` module location final decision.**
ARCHITECTURE.md Open Question 1 leaves the location of `apply_implicit_votes` unresolved (background.rs vs implicit_votes.rs). This affects where the confidence closure is imported from. If located in `unimatrix-store`, the confidence dependency must be injected (via closure parameter) rather than imported directly — verify the chosen location does not create a circular crate dependency.

---

## Edge Cases

**E-01: Empty injection_log for an eligible session.** A session completed but the UserPromptSubmit hook never ran (no entries injected). `scan_injection_log_by_sessions` returns no rows. `apply_implicit_votes` must handle this gracefully — no votes applied, session marked applied. If not marked, the session re-appears on every tick as a false positive.

**E-02: Single-entry batch.** `IMPLICIT_VOTE_BATCH_LIMIT = 1`. Verify tick processes exactly 1 session correctly and returns count = 1.

**E-04: Same entry in multiple sessions in same batch.** Entry_X appears in sessions A, B, C (all success) processed in the same tick. `helpful_count` must be incremented by 3 (one per session). The helpful_ids are session-scoped before dedup — verify cross-session accumulation is additive, not deduplicated across sessions.

**E-06: Exactly 500 sessions in DB (at batch cap).** Run tick with `IMPLICIT_VOTE_BATCH_LIMIT = 500` and exactly 500 eligible sessions. Assert all 500 processed in one tick. Second tick processes 0 sessions.

**E-07: Sessions with `outcome = "abandoned"` (status = Completed).** SCOPE resolved decision: zero signal. Verify the algorithm marks these `implicit_votes_applied = 1` without writing any helpful votes — the session is consumed (preventing re-scan) but produces no signal.

**E-08: Batch contains a mix of success and zero-signal sessions.** All sessions are marked `implicit_votes_applied = 1` after the tick regardless of outcome. Only success sessions produce vote writes. Verify: `query_sessions_pending_implicit_votes` returns 0 for all sessions after a tick, even those with `outcome = "rework"`.

---

## Security Risks

**S-01: No untrusted external input in the tick path.**
`apply_implicit_votes` reads only from `sessions` and `injection_log` — both written by the server itself via authenticated internal paths (Stop hook, UserPromptSubmit hook). No external agent can directly inject data into these tables. The implicit vote tick has no MCP-facing surface. Attack surface is negligible.

**S-02: `entry_id` values from injection_log used in SQL.**
`record_usage_with_confidence` constructs SQL using entry IDs from `injection_log`. These are `u64` values stored as `INTEGER` in SQLite. Verify they are bound as parameters (not string-interpolated into SQL). SQL injection via malformed entry IDs is not possible if rusqlite parameter binding is used consistently — verify this in `background.rs` and `write_ext.rs`.

**S-03: `session_ids` in `mark_implicit_votes_applied`.**
Session IDs are `String` values. They must be bound as SQL parameters, not interpolated. Verify the batch UPDATE uses `?` placeholders with rusqlite binding.

---

## Failure Modes

**F-01: `apply_implicit_votes` returns error.**
`run_implicit_vote_tick` logs `tracing::warn` and returns. The tick continues. No retry until the next 15-minute tick. Sessions remain at `implicit_votes_applied = 0` and will be retried next tick. This is the correct behavior — warn but don't abort.

**F-02: `spawn_blocking` panics.**
The `Err(JoinError)` branch in `run_implicit_vote_tick` logs warn and returns. No tick abort. Same retry behavior as F-01.

**F-03: `mark_implicit_votes_applied` fails after votes written.**
Sessions are not marked applied. On next tick, they are re-scanned. `record_usage_with_confidence` is called again, incrementing `helpful_count` a second time for success sessions. This is the only scenario where double-counting of implicit votes (not double-counting between real-time and background) can occur. The architecture has no per-entry dedup for this case (the session-level flag is the sole guard). This failure mode must be documented as a known limitation. Note: with the ADR-001 simplification, there is no second write call for unhelpful — only the single helpful write is at risk of double-application.

**F-04: Database locked when tick runs.**
If rusqlite's busy timeout is too short and `SQLITE_BUSY` is returned during a long MCP write, `apply_implicit_votes` returns a `StoreError::Sqlite` containing `BUSY`. The tick logs warn and retries next tick. Sessions are not double-processed. Verify the busy timeout is set to at least 5000ms in `db.rs`.

**F-05: `ConfidenceStateHandle` poisoned.**
`confidence_state.read().unwrap_or_else(|e| e.into_inner())` recovers the poisoned lock (established pattern from ADR-004). The snapshot may return stale values from before the panic. The confidence recomputation proceeds with potentially stale alpha0/beta0. This is acceptable — the prior is re-snapshotted on the next tick.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (cold-start backlog) | R-07 | ADR-002 resolves: 500-session cap, oldest-first, estimated 1–5 ticks to drain. Test scenario R-07.1 verifies multi-tick drain. |
| SR-02 (pair accumulation counter storage) | — | Resolved by ADR-001: no implicit unhelpful votes in v1 eliminates the `implicit_unhelpful_pending` table entirely. Counter location risk is moot. |
| SR-03 (inline confidence recomputation tick duration) | R-14 | ADR-004 resolves: inline is bounded at ~500ms for 500 sessions × 5 entries; switch to deferred is a config knob if needed. |
| SR-04 (double-counting between real-time and background) | R-02, R-04, F-03 | ADR-003 resolves: `implicit_votes_applied` flag set by Stop hook prevents tick re-processing. R-02 tests flag coverage on all close paths. F-03 documents residual risk (mark-after-vote failure). |
| SR-05 (signal quality dilution, no injection cap) | R-13 | ARCHITECTURE.md Open Question 2: no cap in scope. Accepted as out-of-scope for v1, flagged as Low-priority risk. |
| SR-06 (TimedOut sessions with non-NULL outcome) | R-10 | ADR-002 SQL resolves: filter uses `status = 1 (Completed)` only. TimedOut excluded by SQL. R-10 tests the boundary. |
| SR-07 (SQLite BUSY contention) | I-01 | WAL mode + busy timeout mitigates. Integration risk I-01 documents and F-04 describes failure mode. |
| SR-08 (crt-019 API stability dependency) | R-05, R-09 | crt-019 merged per MEMORY.md. Snapshot pattern verified in R-05 and R-09. |
| SR-09 (injection_log scan correctness / COUNT DISTINCT analog) | R-01 | R-01 directly addresses. Unimatrix #1044 cited as historical evidence of severity. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 4 scenarios minimum |
| High | 4 (R-02, R-03, R-04, R-05) | 16 scenarios minimum |
| Medium | 9 (R-06–R-12) | 22 scenarios minimum |
| Low | 2 (R-13, R-14) | 2 scenarios (observability + benchmark) |

**Integration risks** (I-01 through I-04): require verification tests for sweep path behavior (I-02, I-03), busy timeout configuration (I-01), and module dependency check (I-04).

**Edge cases** (E-01, E-02, E-04, E-06, E-07, E-08): all should have explicit unit tests. E-01 (empty injection_log), E-07 (abandoned sessions), and E-08 (mixed-outcome batch) are the highest-risk edge cases — they represent silent no-ops or incorrect signal application that are easy to miss.

**Open implementation gap**: ARCHITECTURE.md Open Question 1 (`apply_implicit_votes` module location) is unresolved. The tester must verify no circular crate dependency is introduced regardless of where the function lands.

**Simplification note**: ADR-001 removed the `implicit_unhelpful_pending` table, `increment_pending_and_drain_ready`, and `gc_pending_counters`. The corresponding risks (former R-01, R-08, R-12) and their edge cases (former E-03, E-05) and security risk (former S-04) are eliminated. The new R-12 targets the implementation-correctness risk that the simplified algorithm incorrectly includes non-success sessions in helpful_ids — the most likely error mode after the pair-accumulation code is stripped out.
