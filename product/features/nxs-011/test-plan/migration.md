# Test Plan: Migration (migration.rs)

**Component**: `crates/unimatrix-store/src/migration.rs`
**Risks**: R-03 (migration failure leaves DB inconsistent)
**ACs**: AC-11, AC-17
**Spec reference**: FR-08, ADR-003

---

## Design Principles

- Every migration test uses a fresh temporary database (TC-04 — no shared state).
- All tests are `#[tokio::test]` using an async `sqlx::SqliteConnection` directly.
- Tests call `migrate_if_needed(&mut conn, db_path)` in isolation — not via `Store::open()`.
- Migration tests do NOT construct pools (testing the migration connection in isolation per ADR-003).
- The 16 existing migration integration tests (baseline, AC-11) must all continue to pass.

---

## Baseline Integration Tests (must be preserved) — (AC-11)

These are the 16 existing tests in `crates/unimatrix-store/tests/migration/`. They must all pass with the adapted sqlx `migrate_if_needed` signature. No test may be deleted; tests that previously used `rusqlite::Connection` are rewritten to use `sqlx::SqliteConnection`.

Baseline test names are pre-existing; the test plan does not prescribe them. The pass/fail result is the gate.

---

## New Integration Tests — Version Transition Regression Harness (AC-17)

Each test below starts from a temp DB at the specified initial version, calls `migrate_if_needed`, and asserts the result. Tests are isolated (separate temp files).

### MG-I-01: `test_migration_fresh_db_reaches_v12`
- **Arrange**: Create empty temp SQLite file (no schema — version 0)
- **Act**: Open `SqliteConnection`; call `migrate_if_needed(&mut conn, db_path).await`
- **Assert**: No error returned; query `SELECT value FROM meta WHERE key='schema_version'` returns `"12"`; all 13 expected tables exist (`entries`, `entry_tags`, `vector_map`, `agent_registry`, `audit_log`, `counters`, `co_access`, `sessions`, `injection_log`, `query_log`, `signal_queue`, `observations`, `outcome_index`)
- **Risk**: R-03

### MG-I-02 through MG-I-12: `test_migration_v{N}_to_v{N+1}` (one per transition)

For each N in {0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11}:

- **Arrange**: Create temp DB; manually set schema to version N (apply DDL up to but not including version N+1); set `meta.schema_version = N`
- **Act**: Call `migrate_if_needed(&mut conn, db_path).await`
- **Assert**: Returns `Ok(())`; `meta.schema_version` is now `N+1` (or 12 if N=11)
- **Assert**: Tables specific to version N+1 DDL exist (cross-reference `migration.rs` DDL for each transition)
- **Teardown**: Drop connection (temp file auto-deleted)

| Test Name | Initial Version | Expected Version |
|-----------|----------------|-----------------|
| `test_migration_v0_to_v1` | 0 | 1 |
| `test_migration_v1_to_v2` | 1 | 2 |
| `test_migration_v2_to_v3` | 2 | 3 |
| `test_migration_v3_to_v4` | 3 | 4 |
| `test_migration_v4_to_v5` | 4 | 5 |
| `test_migration_v5_to_v6` | 5 | 6 |
| `test_migration_v6_to_v7` | 6 | 7 |
| `test_migration_v7_to_v8` | 7 | 8 |
| `test_migration_v8_to_v9` | 8 | 9 |
| `test_migration_v9_to_v10` | 9 | 10 |
| `test_migration_v10_to_v11` | 10 | 11 |
| `test_migration_v11_to_v12` | 11 | 12 |

### MG-I-13: `test_migration_idempotent_on_v12_db` — (AC-11, AC-17)
- **Arrange**: Create temp DB already at v12 (full schema applied)
- **Act**: Call `migrate_if_needed(&mut conn, db_path).await` twice in sequence
- **Assert**: Both calls return `Ok(())`; `schema_version` remains `12`; no duplicate table creation errors; no DDL changes
- **Risk**: R-03 (idempotency)

---

## Integration Tests — Failure and Sequencing (ADR-003)

### MG-I-14: `test_migration_failure_blocks_pool_construction` — (R-03)
- **Arrange**: Prepare a DB at version 6 (partially migrated); inject a sqlx SQL error at the v7 step (e.g., by corrupting the schema_version row after v6 is applied — mock approach)
- **Act**: Call `Store::open(db_path, PoolConfig::test_default()).await`
- **Assert**: Returns `Err(StoreError::Migration { .. })`; no `read_pool` or `write_pool` connections opened (verify via connection count or by checking no pool-level logging)
- **Risk**: R-03

### MG-I-15: `test_migration_connection_dropped_before_pool_construction` — (ADR-003)
- **Arrange**: Open a store against a fresh DB
- **Act**: `Store::open()` completes successfully
- **Assert**: By inspection of the call sequence, the migration connection was closed before pool construction. Verify by checking that Pool connection count after `open()` reflects only pool connections (not an extra migration connection held open)
- **Teardown**: `store.close().await`
- **Risk**: R-03

---

## Static Verification (grep)

### MG-S-01: No rusqlite in migration.rs
- **Check**: `grep -n "rusqlite" crates/unimatrix-store/src/migration.rs` returns zero matches
- **Risk**: R-05, AC-01

### MG-S-02: migrate_if_needed accepts `&mut SqliteConnection`
- **Check**: Function signature in `migration.rs` matches `pub(crate) async fn migrate_if_needed(conn: &mut sqlx::SqliteConnection, db_path: &Path) -> Result<()>`
- **Risk**: R-03

---

## Notes

- The 12 version transition tests (MG-I-02 through MG-I-12) require helper utilities to seed a DB at an arbitrary schema version. This setup helper should be extracted into `test_helpers.rs` (or a migration-test-specific helper module), not duplicated per test.
- Schema version seeding helper: `async fn seed_schema_at_version(path: &Path, version: u32)` — opens a connection, creates minimal schema up to the target version, sets `meta.schema_version`.
- The delivery agent must cross-reference `migration.rs` DDL to identify which tables/columns appear in each version transition (OQ-DURING-03 equivalent for migration test data).
- AC-17 gate: at minimum 12 individual transition tests + 1 fresh-db test + 1 idempotency test = 14 tests in the new regression harness.
