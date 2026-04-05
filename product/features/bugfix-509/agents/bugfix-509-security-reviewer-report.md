# Security Review: bugfix-509-security-reviewer

## Risk Level: low

## Summary

PR #525 adds a single compound index `idx_entry_tags_tag_entry_id ON entry_tags(tag, entry_id)` to
fix an O(K) linear scan in the S1 co-occurrence self-join. The change is purely additive DDL: no
existing rows, columns, logic, or trust boundaries are modified. No injection vectors, access control
changes, or new external inputs are introduced. The migration is transactional and idempotent. Risk
is low.

---

## Findings

### Finding 1: No OWASP Injection Risk
- **Severity**: low (informational)
- **Location**: `crates/unimatrix-store/src/db.rs:590-594`, `migration.rs:898-914`
- **Description**: All DDL strings are hardcoded string literals with no user-controlled interpolation.
  `sqlx::query(...)` without `.bind()` is used correctly — there is no parameter binding surface on
  DDL statements and no opportunity for injection.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 2: Migration Atomicity — Correctly Wrapped in Transaction
- **Severity**: low (informational)
- **Location**: `crates/unimatrix-store/src/migration.rs:75-92`
- **Description**: The v22→v23 block runs inside the outer `txn` established in `migrate_if_needed`.
  On any failure the caller calls `txn.rollback()` and propagates the error — schema_version stays at
  22 and the database is left in a consistent state. `CREATE INDEX IF NOT EXISTS` is safe to re-run
  on rollback and retry.
- **Recommendation**: No action required. Behaviour is consistent with all prior migration blocks.
- **Blocking**: no

### Finding 3: Idempotency Guard Is Correct
- **Severity**: low (informational)
- **Location**: `crates/unimatrix-store/src/migration.rs:65-67`
- **Description**: `if current_version >= CURRENT_SCHEMA_VERSION { return Ok(()); }` at the top of
  `migrate_if_needed` short-circuits before any migration SQL runs on already-migrated databases.
  The `if current_version < 23` block inside `run_main_migrations` provides a second guard. Both
  are correct; the outer guard is the definitive one.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 4: Three-Path Coverage — Legacy v5→v6 Array Also Updated
- **Severity**: low (informational)
- **Location**: `crates/unimatrix-store/src/migration.rs:1206`
- **Description**: The fix correctly adds the compound index to the legacy static DDL array used by
  the v5→v6 migration path. Databases upgrading from very old schemas (pre-v6) will receive the
  index as part of the full table rebuild. This is consistent with how all other indexes are handled
  in the array.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 5: No Lock Contention Risk on Upgrade
- **Severity**: low (informational)
- **Location**: `crates/unimatrix-store/src/migration.rs:36-39`
- **Description**: Migration runs on a dedicated non-pooled `SqliteConnection` opened before pool
  construction (documented in the module header). The connection is dropped before the read/write
  pool is created. No concurrent readers or writers can observe the database mid-migration. `CREATE
  INDEX` on SQLite acquires an exclusive write lock for its duration, which is safe here because the
  pool does not yet exist.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 6: Test Fixture Does Not Use `IF NOT EXISTS` — Intentional and Correct
- **Severity**: low (informational)
- **Location**: `crates/unimatrix-store/tests/migration_v22_to_v23.rs:438` (comment)
- **Description**: The v22 database builder in the test fixture deliberately omits
  `idx_entry_tags_tag_entry_id` to simulate the pre-fix state. The comment makes the intent explicit.
  The production `CREATE INDEX IF NOT EXISTS` form in the migration block is safe regardless of
  whether the index already exists — a second application is a no-op.
- **Recommendation**: No action required.
- **Blocking**: no

### Finding 7: Test Assertion Strength — MIG-V23-U-04 Row Count Guard
- **Severity**: low (advisory)
- **Location**: `crates/unimatrix-store/tests/migration_v22_to_v23.rs:612-616`
- **Description**: The test asserts `row_count >= 1` rather than `row_count == 2`. A compound index
  on two columns should always return exactly 2 rows from `pragma_index_info`. The `>= 1` check
  would pass even if only one column were present. This is a weak assertion, though the subsequent
  `assert_eq!(first_col, "tag")` check partially compensates by confirming leading-column ordering.
  The second column (`entry_id`) is not independently verified. In practice the SQLite DDL
  `ON entry_tags(tag, entry_id)` cannot produce a 1-column index, so this is a theoretical gap
  rather than a real exposure.
- **Recommendation**: Consider tightening to `assert_eq!(row_count, 2, ...)` on a follow-up. Not
  blocking.
- **Blocking**: no

### Finding 8: No Secrets, No Unsafe Code
- **Severity**: low (informational)
- **Location**: all changed files
- **Description**: `#![forbid(unsafe_code)]` is active on `unimatrix-store`. No credentials,
  tokens, API keys, or hardcoded secrets appear in any changed file. No `std::process::exit` calls
  introduced.
- **Recommendation**: No action required.
- **Blocking**: no

---

## Blast Radius Assessment

Worst case if the migration block has a subtle bug:

1. **If the `CREATE INDEX` statement were wrong** (wrong table, wrong columns): SQLite would return
   an error, the transaction would roll back, schema_version would remain at 22, and the store would
   fail to open with a `StoreError::Migration`. The process would error-exit rather than silently
   corrupt data. Safe failure mode.

2. **If the version counter UPDATE failed silently**: The outer `run_main_migrations` error path
   rolls back the transaction. On next open, `current_version` would still be 22 and the v22→v23
   block would run again — harmless due to `IF NOT EXISTS`.

3. **If the test fixture `create_v22_database` produced wrong DDL**: Only test runs would be
   affected; production code is not touched. The worst outcome would be a false-passing test, which
   is a quality concern but not a security concern.

No blast radius path leads to data loss, privilege escalation, information disclosure, or denial of
service under normal operating conditions.

---

## Regression Risk

- **Existing query behaviour**: Adding an index never changes query semantics; SQLite's query planner
  may choose the new index for tag-filtered lookups but will return identical result sets.
- **Migration chain integrity**: The `assert!(read_schema_version >= 22)` relaxation in
  `migration_v21_v22.rs` tests is correct — those tests now run on top of a v23 store after the
  compound index migration also fires. The change avoids a false failure without weakening the
  semantic guarantee (goal_clusters table presence is still asserted separately).
- **`test_schema_version_is_14`** in `sqlite_parity.rs` was updated to assert 23. This is
  mechanical and correct.
- **`test_migration_v7_to_v8_backfill`** in `server.rs` was updated from version 22 to 23. This
  is the expected cascading assertion update when the schema version advances.
- **No existing indexes are dropped or modified.** The new index is purely additive.

Regression risk: very low.

---

## Dependency Safety

No new crate dependencies introduced. No `Cargo.toml` changes in the diff. No CVE exposure.

---

## PR Comments

Posted 1 comment on PR #525 (see below). No blocking findings.

---

## Knowledge Stewardship

- nothing novel to store — the migration transaction pattern (commit on Ok, rollback on Err,
  `IF NOT EXISTS` idempotency) is already established practice in this codebase and is documented
  by prior ADRs retrieved from Unimatrix (#4088, #760). The weak `>= 1` row count in MIG-V23-U-04
  is noted as advisory above but is not a recurring security anti-pattern warranting a lesson entry.
