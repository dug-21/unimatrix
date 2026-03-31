# Test Plan: CycleGcPass (unimatrix-store retention methods)

**Component:** `crates/unimatrix-store/src/retention.rs` (new file)
**Risks Covered:** R-02, R-03 (partial), R-06, R-07, R-08 (partial), R-09, R-12, R-14
**ACs Covered:** AC-02, AC-03, AC-06, AC-07, AC-08, AC-09, AC-14

All tests in this component are `#[tokio::test]` async tests that construct an
in-memory SQLite `SqlxStore` directly. No MCP layer is involved.

---

## Unit Test Expectations

### `test_gc_cycle_based_pruning_correctness` (AC-02)

**Arrange:**
- Insert N = 5 reviewed cycles into `cycle_review_index`, each with distinct
  `computed_at` values (oldest to newest).
- For each cycle insert: 2 sessions, 3 observations per session, 2 query_log rows
  per session, 1 injection_log row per session.
- Set K = 3.

**Act:** Call `list_purgeable_cycles(3, max_per_tick=100)` then `gc_cycle_activity`
for each returned cycle. Call `store_cycle_review` with `raw_signals_available: 0`
after each `gc_cycle_activity` completes.

**Assert:**
- Exactly 2 cycles were purgeable (the 2 oldest by `computed_at`).
- After GC: `observations` for those 2 cycles is 0 rows (via session_id join).
- After GC: `query_log` for those 2 cycles is 0 rows.
- After GC: `injection_log` for those 2 cycles is 0 rows.
- After GC: `sessions` for those 2 cycles is 0 rows.
- The 3 newest cycles retain all their rows (count unchanged from pre-GC).

---

### `test_gc_protected_tables_regression` (AC-03)

**Arrange:**
- Insert rows into `entries`, `GRAPH_EDGES`, `co_access`, `cycle_events`,
  `cycle_review_index`, `observation_phase_metrics`.
- Insert 2 purgeable reviewed cycles with sessions and observations.
- Record `COUNT(*)` for each protected table.

**Act:** Run the full GC pass (list_purgeable, gc_cycle_activity, gc_unattributed,
gc_audit_log).

**Assert:** `COUNT(*)` for each protected table is identical to the pre-GC snapshot.

---

### `test_gc_query_log_pruned_with_cycle` (AC-07)

**Arrange:**
- Insert 2 reviewed cycles: cycle A (purgeable, outside K), cycle B (retained, inside K).
- For each cycle: insert sessions, query_log rows linked to those sessions.

**Act:** Run `gc_cycle_activity` for cycle A only.

**Assert:**
- `query_log` rows whose session_id belongs to cycle A's sessions: 0.
- `query_log` rows whose session_id belongs to cycle B's sessions: unchanged count.

---

### `test_gc_cascade_delete_order` (AC-08)

This test covers R-02 (cascade order) and validates the order-enforcement
mutation described in the Risk Strategy.

**Arrange:**
- Insert 1 reviewed purgeable cycle with 2 sessions, injection_log rows per
  session, observations, query_log rows.

**Act part 1 (correct order):** Call `gc_cycle_activity(feature_cycle)`.

**Assert part 1:**
- 0 `injection_log` rows remain for those sessions.
- 0 `sessions` rows remain for that cycle.
- `CycleGcStats.sessions_deleted == 2`, `CycleGcStats.injection_log_deleted > 0`.

**Mutation assertion (order-inversion check):**
The test must include an explicit comment (or a secondary in-memory DB sub-test)
demonstrating that if sessions were deleted before injection_log (inverted order),
injection_log rows would remain. The verification method:
- Re-insert the same rows.
- Execute SQL with sessions deleted first, then injection_log via the subquery.
- Assert injection_log rows are still present (count > 0).
- Document: this is the R-02 order enforcement proof.

**Assert part 2 (no orphans):**
After the correct-order GC: query `injection_log WHERE session_id NOT IN (SELECT
session_id FROM sessions)`. Result count must be 0.

---

### `test_gc_unattributed_active_guard` (AC-06)

**Arrange:**
- Session A: `feature_cycle = NULL`, `status = Active` (value 0), 3 observations.
- Session B: `feature_cycle = NULL`, `status = Closed` (non-zero value), 3 observations.
- Session C: `feature_cycle = NULL`, `status = Closed`, no observations (only injection_log).

**Act:** Call `gc_unattributed_activity()`.

**Assert:**
- Session A's observations: still present (3 rows).
- Session A itself: still present in `sessions`.
- Session B's observations: deleted.
- Session B: deleted from `sessions`.
- Session C's injection_log: deleted.
- Session C: deleted from `sessions`.

---

### `test_gc_audit_log_retention_boundary` (AC-09)

**Arrange:**
- Insert audit_log row R1: `timestamp = now_unix_secs - (200 * 86400)` (200 days ago).
- Insert audit_log row R2: `timestamp = now_unix_secs - (100 * 86400)` (100 days ago).
- Insert audit_log row R3: `timestamp = now_unix_secs - (1 * 86400)` (yesterday).

**Act:** Call `gc_audit_log(180)`.

**Assert:**
- R1 (200 days old) is deleted.
- R2 (100 days old) is present.
- R3 (1 day old) is present.

**Also assert:** The GC query does NOT use millisecond arithmetic (no `* 1000` or
`/ 1000` in the audit_log DELETE path). Verified by code inspection.

---

### `test_gc_protected_tables_row_level` (AC-14)

**Arrange:**
- Insert one named/identifiable row into each protected table:
  `entries`, `GRAPH_EDGES`, `cycle_events`, `cycle_review_index` (as a retained
  cycle, K = 1), `observation_phase_metrics`.
- Insert 2 purgeable cycles with sessions and observations.

**Act:** Run full GC.

**Assert:** Each named row from the protected tables is still retrievable by its
primary key / identifying field after GC.

---

### `test_gc_query_plan_uses_index` (NFR-03 — R-09)

This test does not perform runtime data deletion. It issues `EXPLAIN QUERY PLAN`
for the two GC DELETE subqueries against a real SQLite connection.

**Arrange:** Open an in-memory SQLite DB (using the store's connection pool, or a
raw sqlx connection). Create the schema (or use a minimal test DB with the relevant
tables and indexes).

**Act:**
```sql
EXPLAIN QUERY PLAN
DELETE FROM observations WHERE session_id IN
  (SELECT session_id FROM sessions WHERE feature_cycle = 'test-cycle');

EXPLAIN QUERY PLAN
DELETE FROM query_log WHERE session_id IN
  (SELECT session_id FROM sessions WHERE feature_cycle = 'test-cycle');
```

**Assert:**
- The query plan text for the observations DELETE references `idx_observations_session`
  (not "SCAN observations" as a full-table scan).
- The query plan text for the query_log DELETE references `idx_query_log_session`
  (not "SCAN query_log").
- If either assertion fails, the test fails with a diagnostic: "full-table scan
  detected on {table}; index {expected_idx} not used. Query plan: {plan_output}".

**Note:** In-memory SQLite indexes must be created explicitly in the test setup.
Reference the schema in `crates/unimatrix-store/src/db.rs` for exact index names.

---

## Edge Cases

- **Zero observations for a cycle's sessions:** `gc_cycle_activity` must not fail;
  `CycleGcStats.observations_deleted == 0` is a valid result.
- **`list_purgeable_cycles` with exactly K reviewed cycles:** Returns empty Vec.
  No GC runs. No errors.
- **K = 1:** Only the most recent cycle is retained. All others purgeable. Verify
  the `NOT IN (... LIMIT 1)` subquery is syntactically valid and returns correct results.
- **`gc_audit_log` with `timestamp = 0`:** Epoch row must be deleted by any valid
  `audit_log_retention_days` value.
