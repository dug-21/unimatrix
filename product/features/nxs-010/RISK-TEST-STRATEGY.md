# Risk-Based Test Strategy: nxs-010

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Migration v10->v11 partially applies: tables created but backfill or version bump fails, leaving DB in inconsistent state | High | Low | High |
| R-02 | Backfill SQL produces incorrect aggregates (wrong COUNT, wrong SUM of durations, wrong MIN(started_at)) from sessions with NULL/empty feature_cycle edge cases | High | Med | Critical |
| R-03 | AUTOINCREMENT sqlite_sequence table not created or corrupted, causing insert_query_log to fail silently in fire-and-forget path | Med | Low | Med |
| R-04 | Fire-and-forget query_log write panics inside spawn_blocking, crashing the tokio worker thread instead of logging a warning | High | Med | Critical |
| R-05 | UDS and MCP paths construct QueryLogRecord with divergent field values (different JSON encoding, missing fields, inconsistent retrieval_mode/source strings) | Med | Med | High |
| R-06 | JSON serialization of result_entry_ids or similarity_scores produces malformed output for edge cases (empty results, NaN scores, very large arrays) | Med | Med | High |
| R-07 | update_topic_delivery_counters called on nonexistent topic silently succeeds (0 rows affected) instead of returning error | Med | Med | High |
| R-08 | Migration runs on fresh database (no entries table) and backfill query fails because sessions table does not exist yet | High | Low | Med |
| R-09 | Concurrent Store::open() calls race on migration, causing duplicate backfill rows or schema version inconsistency | Med | Low | Med |
| R-10 | INSERT OR REPLACE on upsert_topic_delivery deletes and recreates the row, losing counter values set by concurrent update_topic_delivery_counters | High | Med | Critical |
| R-11 | query_log write holds SQLite write lock, blocking a concurrent migration or topic_delivery upsert in another thread | Med | Low | Med |
| R-12 | scan_query_log_by_session returns results in wrong order (not ts ASC) or returns rows from other sessions due to incorrect WHERE clause | Med | Low | Med |
| R-13 | Backfill double-counts sessions where feature_cycle contains leading/trailing whitespace variants of the same topic name | Low | Low | Low |
| R-14 | topic_deliveries.total_duration_secs overflows i64 for sessions with NULL ended_at (COALESCE produces negative or nonsensical values) | Med | Med | High |

## Risk-to-Scenario Mapping

### R-01: Migration partial application leaves inconsistent state
**Severity**: High
**Likelihood**: Low
**Impact**: Database opens at v10 with partially created tables. Next migration attempt may fail or produce duplicate rows.

**Test Scenarios**:
1. Run v10->v11 migration on a v10 database with attributed sessions. Verify both tables exist, backfill rows are correct, and schema_version = 11.
2. Run migration on a v11 database. Verify it is a no-op (idempotent guard).
3. Simulate migration interruption (create tables but do not bump version). Re-run migration. Verify INSERT OR IGNORE prevents duplicate topic_deliveries rows and version updates correctly.

**Coverage Requirement**: Integration test with a real v10 database (seeded with sessions) that verifies atomic completion of all migration steps.

### R-02: Backfill SQL produces incorrect aggregates
**Severity**: High
**Likelihood**: Med
**Impact**: Downstream features (col-020, crt-018) consume incorrect topic_deliveries data as ground truth. Wrong session counts or durations corrupt cross-session analysis.

**Test Scenarios**:
1. Seed v10 database with 3 topics: topic-A (3 sessions, known durations), topic-B (1 session), topic-C (sessions with NULL ended_at). Run migration. Verify each topic's total_sessions, total_duration_secs, created_at (MIN), and status = 'completed'.
2. Seed v10 database with sessions where feature_cycle IS NULL or empty string. Verify these are excluded from backfill.
3. Seed v10 database with zero attributed sessions. Verify migration succeeds with 0 topic_deliveries rows.
4. Seed session with ended_at = NULL (incomplete session). Verify COALESCE(SUM(ended_at - started_at), 0) handles this without error and produces a reasonable value.

**Coverage Requirement**: Integration test with controlled session data covering all edge cases in the GROUP BY aggregation.

### R-03: AUTOINCREMENT table corruption
**Severity**: Med
**Likelihood**: Low
**Impact**: query_log inserts fail. Fire-and-forget semantics mean failures are only visible in warn logs, not to users.

**Test Scenarios**:
1. Insert 3 query_log rows. Verify each gets a unique, monotonically increasing query_id > 0.
2. Insert a query_log row with query_id = 0 in the record struct. Read back. Verify query_id was auto-allocated (not 0).

**Coverage Requirement**: Unit test verifying AUTOINCREMENT allocates IDs correctly.

### R-04: Fire-and-forget query_log write panics in spawn_blocking
**Severity**: High
**Likelihood**: Med
**Impact**: Panic in spawn_blocking kills the tokio worker thread. Repeated panics exhaust the thread pool, degrading server responsiveness.

**Test Scenarios**:
1. Invoke UDS search with a valid query. Verify query_log row is written with source="uds" and retrieval_mode="strict". Verify search response is returned before the write completes.
2. Invoke MCP search with a valid query. Verify query_log row is written with source="mcp" and retrieval_mode="flexible".
3. Invoke UDS search when Store is in a state where insert_query_log returns an error (e.g., closed connection). Verify a warning is logged and no panic occurs. Search response is unaffected.
4. Invoke UDS search with session_id = None. Verify query_log write is skipped (guard condition).

**Coverage Requirement**: Integration test for both UDS and MCP paths. Error path test must verify no panic propagation.

### R-05: UDS/MCP QueryLogRecord field divergence
**Severity**: Med
**Likelihood**: Med
**Impact**: Downstream analysis (crt-019) processes query_log rows with inconsistent schemas, producing incorrect search quality metrics.

**Test Scenarios**:
1. Execute one search via UDS and one via MCP with identical query text and result set. Compare the two query_log rows field-by-field. All fields except source, retrieval_mode, and session_id should be identical in structure.
2. Verify both paths use the shared QueryLogRecord constructor (FR-08.1) by checking that result_entry_ids and similarity_scores JSON arrays have identical element counts matching result_count.

**Coverage Requirement**: Integration test comparing query_log rows from both transport paths.

### R-06: JSON serialization edge cases for result arrays
**Severity**: Med
**Likelihood**: Med
**Impact**: Malformed JSON in result_entry_ids or similarity_scores causes deserialization failures in downstream consumers (crt-019, col-021).

**Test Scenarios**:
1. Insert query_log row with empty results (result_count=0, result_entry_ids="[]", similarity_scores="[]"). Read back and deserialize both as empty Vec.
2. Insert query_log row with 10 results. Verify result_entry_ids deserializes as Vec<u64> and similarity_scores as Vec<f64> with matching lengths.
3. Insert query_log row where a similarity score is exactly 0.0 or 1.0. Verify JSON round-trip preserves the value.

**Coverage Requirement**: Unit test covering empty, single, and multi-element JSON arrays with edge-case numeric values.

### R-07: update_topic_delivery_counters on nonexistent topic
**Severity**: Med
**Likelihood**: Med
**Impact**: col-020 calls update_topic_delivery_counters for a topic that was never upserted (race condition or missing backfill). Counter update silently succeeds with 0 rows affected. Downstream reports show stale data.

**Test Scenarios**:
1. Call update_topic_delivery_counters for a topic that does not exist. Verify it returns an error (not Ok(())).
2. Insert a topic, call update_topic_delivery_counters with positive deltas, verify counters are incremented.
3. Insert a topic, call update_topic_delivery_counters with negative deltas (correction scenario), verify counters are decremented.

**Coverage Requirement**: Unit test verifying error return for missing topic and correct arithmetic for existing topic.

### R-10: INSERT OR REPLACE destroys concurrent counter updates
**Severity**: High
**Likelihood**: Med
**Impact**: upsert_topic_delivery uses INSERT OR REPLACE. If col-020 calls update_topic_delivery_counters between the read and the replace, the counter values from the update are lost. The replaced row has the stale values from the upsert caller.

**Test Scenarios**:
1. Insert a topic with total_sessions=5. Call upsert_topic_delivery with total_sessions=0 (fresh record from attribution). Verify total_sessions is now 0 (replaced). Document this as expected INSERT OR REPLACE behavior.
2. Verify that upsert_topic_delivery and update_topic_delivery_counters are not called concurrently for the same topic in any documented workflow.

**Coverage Requirement**: Unit test demonstrating replace semantics. Architecture review confirming no concurrent upsert+update workflow exists.

### R-14: Backfill duration overflow for sessions with NULL ended_at
**Severity**: Med
**Likelihood**: Med
**Impact**: Sessions where ended_at is NULL (process killed, incomplete sessions) cause `ended_at - started_at` to evaluate as NULL. COALESCE(SUM(...), 0) handles the case where ALL sessions have NULL ended_at, but a mix of NULL and non-NULL ended_at values silently drops the NULL rows from SUM. This produces an undercount, not an error.

**Test Scenarios**:
1. Seed 3 sessions for the same topic: 2 with valid ended_at (100s and 200s duration), 1 with NULL ended_at. Run backfill. Verify total_duration_secs = 300 (NULL row excluded from SUM, not errored).
2. Seed all sessions with NULL ended_at. Run backfill. Verify total_duration_secs = 0 (COALESCE kicks in).

**Coverage Requirement**: Integration test with mixed NULL/non-NULL ended_at sessions.

## Integration Risks

### Store::open() init sequence (Unimatrix #375)
The migration runs before create_tables(). Both emit CREATE TABLE IF NOT EXISTS for the new tables. Risk: if the ordering is ever changed (create_tables first), the backfill would fail because the sessions table might not exist yet on a partial migration path. Test must verify that opening a v10 database with nxs-010 code runs migration first.

### UDS listener fire-and-forget task accumulation
The UDS path already spawns multiple fire-and-forget tasks per search (injection_log, co-access). Adding query_log is a third task. Per Unimatrix #731, unbatched fire-and-forget writes previously caused blocking pool saturation. The architecture notes sequential UDS processing mitigates this, but test should verify no task accumulation under repeated rapid searches.

### MCP tool handler spawn_blocking isolation
The MCP path uses a separate spawn_blocking call for query_log, distinct from the batched usage+confidence write (vnc-010 fix). Verify the query_log spawn_blocking does not interfere with the batched write (no shared mutable state, no lock contention on Store).

### Module registration in lib.rs
Both new modules (topic_deliveries.rs, query_log.rs) must be registered in unimatrix-store/src/lib.rs with `pub mod` and `pub use` re-exports. Missing registration causes compile errors but could be missed if only integration tests (which import from the crate) are run.

## Edge Cases

| Edge Case | Risk ID | Expected Behavior |
|-----------|---------|-------------------|
| Empty sessions table (no attributed sessions) | R-02 | Backfill produces 0 topic_deliveries rows, migration succeeds |
| Session with feature_cycle = "" (empty string) | R-02 | Excluded from backfill by WHERE clause |
| Session with NULL ended_at | R-14 | Excluded from SUM duration, not an error |
| All sessions for a topic have NULL ended_at | R-14 | total_duration_secs = 0 via COALESCE |
| Search returns 0 results | R-06 | query_log row with result_count=0, result_entry_ids="[]", similarity_scores="[]" |
| Very long query text (>10KB) | R-06 | SQLite TEXT has no practical limit; insert succeeds |
| UDS search with session_id = None | R-04 | query_log write skipped (guard condition) |
| MCP search with session_id = None | R-04 | query_log written with empty string session_id |
| Concurrent upsert and counter update on same topic | R-10 | INSERT OR REPLACE overwrites counter values -- last writer wins |
| get_topic_delivery for nonexistent topic | R-07 | Returns None |
| scan_query_log_by_session for session with 0 queries | R-12 | Returns empty Vec |
| Migration re-run on v11 database | R-01 | No-op (version guard) |
| topic name with unicode characters | R-02 | TEXT PRIMARY KEY handles unicode; no risk |

## Security Risks

### Untrusted Input: query_text
**Source**: User-provided search queries via hooks (UDS) or agent tool calls (MCP).
**Risk**: SQL injection via query_text if parameterized queries are not used in insert_query_log.
**Blast radius**: query_log table corruption or data exfiltration from other tables.
**Mitigation**: All Store methods must use rusqlite parameterized queries (? placeholders). The existing codebase consistently uses parameterized queries. Verify insert_query_log does not use string interpolation.

### Untrusted Input: session_id
**Source**: Hook payload (UDS) or MCP context (MCP).
**Risk**: Malicious session_id could be crafted to cause index bloat or collisions.
**Blast radius**: Low -- session_id is a TEXT field with an index. Unusual values do not affect other tables.
**Mitigation**: Parameterized queries. No additional validation needed beyond existing patterns.

### Untrusted Input: result_entry_ids / similarity_scores JSON
**Source**: Internally generated from search results. Not directly user-controlled.
**Risk**: Minimal. These are serialized from Vec<u64> and Vec<f64> via serde_json. No injection vector.
**Blast radius**: None -- internal data only.

### Path Traversal / Deserialization
**Risk**: None. This feature does not accept file paths or deserialize untrusted binary data. All input is text/numeric via parameterized SQL.

## Failure Modes

| Failure | Expected Behavior | Recovery |
|---------|-------------------|----------|
| Migration fails mid-transaction | Transaction rolls back. Database remains at v10. Next Store::open() retries migration. | Automatic retry on next startup. |
| insert_query_log fails (disk full, locked DB) | Warning logged. Search response already returned. Query not recorded. | Tolerable data gap. Warn log alerts operators. |
| upsert_topic_delivery fails | Error propagated to caller (col-017 attribution). Attribution may retry or log error. | Caller handles error. Topic delivery record created on next attribution. |
| update_topic_delivery_counters on missing topic | Error returned to caller. | Caller (col-020) must ensure topic exists before incrementing counters. |
| serde_json::to_string fails for entry_ids/scores | unwrap_or_default() produces empty string. query_log row written with empty result_entry_ids/similarity_scores. | Degraded but non-fatal. Downstream analysis skips rows with empty arrays. |
| AUTOINCREMENT exhaustion (i64 max) | SQLite returns SQLITE_FULL. insert_query_log fails. | Practically impossible (~9.2 quintillion rows). Not a real concern. |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (DDL-before-migration ordering) | R-01, R-08 | Architecture confirms migrate_if_needed() runs before create_tables() in Store::open(). Both use IF NOT EXISTS. Fresh DB skips migration entirely. Validated by Unimatrix #375 procedure. |
| SR-02 (Schema version collision with col-017) | R-01 | Architecture targets v11 with guard `current_version < 11`. col-017 must merge first. Merge order is a delivery constraint, not an architecture risk. |
| SR-03 (AUTOINCREMENT vs counter divergence) | R-03 | ADR-001 documents decision boundary. AUTOINCREMENT used for append-only logs; counters for entity tables. Precedent: observations table (Unimatrix #382). |
| SR-04 (Backfill quality depends on col-017) | R-02 | Backfill SQL is correct given correct session data. If col-017 attribution is buggy, backfill faithfully reproduces bad data. Mitigation: col-020 recomputes from raw sessions, not backfill aggregates. |
| SR-05 (Fire-and-forget failure semantics) | R-04 | ADR-002 defines failure semantics: warn-level log, no retry, no error propagation. Guard conditions specified for UDS (skip if no session_id) and MCP (always write). |
| SR-06 (No GC policy, volume growth) | -- | NFR-03 sizes for 30K rows/year. Two indexes ensure scan performance. Accepted risk; GC deferred. |
| SR-07 (UDS/MCP field divergence) | R-05 | FR-08.1 requires shared QueryLogRecord constructor. Both paths use identical field population logic. |
| SR-08 (col-017 dependency is critical-path) | R-02 | Delivery gated on col-017 integration tests passing (C-05). Backfill produces 0 rows without col-017 (safe but useless). |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-02, R-04, R-10) | 9 scenarios |
| High | 4 (R-01, R-05, R-06, R-07, R-14) | 13 scenarios |
| Medium | 4 (R-03, R-08, R-09, R-11, R-12) | 5 scenarios |
| Low | 1 (R-13) | 0 scenarios (accepted risk) |
