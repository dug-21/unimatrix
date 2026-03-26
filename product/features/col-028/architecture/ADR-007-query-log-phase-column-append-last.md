## ADR-007: query_log.phase Column Appended as Last Positional Parameter

### Context

`analytics.rs` uses positional `?1`..`?N` binding for the `query_log` INSERT. The
current statement has 8 columns (?1 through ?8). `scan_query_log_by_sessions`,
`scan_query_log_by_session`, and `row_to_query_log` all use positional column indices
(0 through 8) for the SELECT result.

Adding `phase` as a column in any position other than last requires reindexing all
existing positional params in the INSERT and all existing positional indices in the
SELECT/deserializer. Reindexing is error-prone: a silent off-by-one will produce
incorrect data deserialization at runtime (not a compile error).

Adding `phase` as the last positional parameter (?9 in the INSERT, index 9 in
`row_to_query_log`) requires no reindexing of existing binds.

The existing pattern (v14→v15 `feature_entries.phase`, v15→v16 `cycle_events.goal`,
v13→v14 `domain_metrics_json`) consistently appends new columns at the end.

### Decision

Add `phase TEXT` as the last column in `query_log` via `ALTER TABLE query_log ADD COLUMN phase TEXT`.
Add `idx_query_log_phase` index after the column is added:
`CREATE INDEX IF NOT EXISTS idx_query_log_phase ON query_log (phase)`.

Update the INSERT in `analytics.rs`:
```sql
INSERT INTO query_log
    (session_id, query_text, ts, result_count,
     result_entry_ids, similarity_scores, retrieval_mode, source, phase)
 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
```

Update both SELECT statements to include `phase` at position 9:
```sql
SELECT query_id, session_id, query_text, ts, result_count,
       result_entry_ids, similarity_scores, retrieval_mode, source, phase
FROM query_log ...
```

Update `row_to_query_log` to deserialize at index 9:
```rust
phase: row.try_get::<Option<String>, _>(9)
    .map_err(|e| StoreError::Database(e.into()))?,
```

The migration uses the `pragma_table_info` pre-check (pattern #1264) for idempotency:
```rust
let has_phase_column: bool = sqlx::query_scalar::<_, i64>(
    "SELECT COUNT(*) FROM pragma_table_info('query_log') WHERE name = 'phase'",
)
.fetch_one(&mut **txn).await
.map(|count| count > 0)
.unwrap_or(false);

if !has_phase_column {
    sqlx::query("ALTER TABLE query_log ADD COLUMN phase TEXT")
        .execute(&mut **txn).await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
}
// Index creation uses IF NOT EXISTS — always safe to run
sqlx::query("CREATE INDEX IF NOT EXISTS idx_query_log_phase ON query_log (phase)")
    .execute(&mut **txn).await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
```

Pre-existing rows receive `phase = NULL`. Downstream consumers must treat NULL as
"no-phase session" (not as a valid phase label).

**SR-01 enforcement**: The INSERT column list, both SELECT statements, and
`row_to_query_log` are a single atomic change unit. They must be modified together.
AC-17 (end-to-end round-trip test reading back `phase`) is the runtime guard against
any divergence.

**SR-02 cascade**: The following files assert `schema_version == 16` and must be
updated as part of this feature:

- `crates/unimatrix-store/tests/migration_v15_to_v16.rs` — all `assert_eq!(... 16)`
  assertions and the `test_current_schema_version_is_16` function name.
- `crates/unimatrix-server/src/server.rs` — lines 2059 and 2084.

New test file to create: `crates/unimatrix-store/tests/migration_v16_to_v17.rs`
following the `migration_v15_to_v16.rs` pattern.

### Consequences

- No existing positional bindings are reindexed — zero risk of silent off-by-one.
- The migration is idempotent (pragma_table_info pre-check, AC-15).
- `CURRENT_SCHEMA_VERSION` advances from 16 to 17 (AC-13).
- The `eval/scenarios/tests.rs` helper `insert_query_log_row` uses a raw SQL INSERT
  (not `QueryLogRecord::new`). Its positional INSERT must also be updated to include
  the phase column. All 15 call sites in that file call the helper, not the struct
  constructor directly — updating the helper updates all call sites.
- `uds/listener.rs:1324` calls `QueryLogRecord::new` with the current 6-argument
  signature. After this ADR, the signature gains a 7th argument (`phase: Option<String>`).
  That call site must pass `None` — no phase semantics, just a compile fix (SR-03).

Related: pattern #1264 (pragma_table_info idempotency), pattern #2933 (schema version
cascade), ADR-001 col-028 through ADR-006 col-028.
