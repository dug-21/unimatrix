# Component 1: SQL Fix — `read.rs`

## Purpose

Fix a one-token column-name bug in `query_stale_prerequisite_edges_for_cycle` that caused
a runtime SQLite error ("no such column: fe.feature_cycle"), silently swallowed by the caller
via `unwrap_or_else`, producing a false-negative empty result for every invocation.

## File

`crates/unimatrix-store/src/read.rs`, line 1618

## Function (unchanged signature)

```rust
pub async fn query_stale_prerequisite_edges_for_cycle(
    &self,
    feature_cycle: &str,
) -> Result<Vec<(u64, u64)>>
```

## Current (Broken) SQL

```sql
SELECT ge.source_id, ge.target_id
FROM graph_edges ge
JOIN entries e ON e.id = ge.source_id
JOIN feature_entries fe ON fe.entry_id = ge.source_id
WHERE ge.relation_type = 'Prerequisite'
  AND e.status = 1
  AND fe.feature_cycle = ?1         -- WRONG: column does not exist
```

The `feature_entries` table defines its cycle column as `feature_id` (DDL in `db.rs:616-621`,
write path in `write_ext.rs:274`, confirmed read path in `analytics.rs:687`). The name
`feature_cycle` appears nowhere in the schema. SQLite raises "no such column: fe.feature_cycle"
at runtime; the error propagates as `StoreError::Database` through `map_err` on line 1623.

## Fixed SQL

```sql
SELECT ge.source_id, ge.target_id
FROM graph_edges ge
JOIN entries e ON e.id = ge.source_id
JOIN feature_entries fe ON fe.entry_id = ge.source_id
WHERE ge.relation_type = 'Prerequisite'
  AND e.status = 1
  AND fe.feature_id = ?1            -- FIXED: matches feature_entries DDL column name
```

## Change Description

Line 1618: replace the literal `fe.feature_cycle` with `fe.feature_id`.

All surrounding code is unchanged:
- `.bind(feature_cycle)` on line 1620 — the Rust variable name is `feature_cycle`; this is
  correct and unchanged. The variable name is not the column name.
- `.fetch_all(self.read_pool())` on line 1621 — unchanged.
- `.map_err(|e| StoreError::Database(e.into()))` on line 1623 — unchanged.
- The row-mapping iterator (lines 1625-1635) — unchanged.
- Function signature and return type — unchanged.

## Error Handling

No change to error handling. The function already propagates errors via `Result`:
- SQLite column errors surface as `StoreError::Database(sqlx::Error::Database(...))`.
- After the fix, the only expected errors are genuine database failures, not column-name
  mismatches.
- The caller in `tools.rs:2169-2177` uses `unwrap_or_else` to swallow errors at the
  handler layer (out of scope for vnc-016; tracked via follow-up GitHub issue).
- The Rust unit test (Component 2) calls the function directly and surfaces errors via
  `Result::expect` / `Result::unwrap` assertions — it does NOT use `unwrap_or_else`.

## Key Test Scenarios

1. After the fix: a database containing `(feature_entries.feature_id = X, entry_id = A)`,
   `(graph_edges source=A, target=B, relation_type='Prerequisite')`, and
   `(entries.id=A, status=1)` must cause the function to return `Ok(vec![(A, B)])`.

2. Before the fix: calling the function against any database must return
   `Err(StoreError::Database(...))` containing "no such column: fe.feature_cycle". This
   fail-first behavior must be manually verified by the implementer before applying the fix,
   confirming the test (Component 2) is not vacuous.

3. Scoping: querying with cycle_id `X` when `feature_entries` contains a row for cycle `Y`
   (same entry A) must return an empty vec, confirming the WHERE clause filters by cycle.

## Constraints

- C-10: `feature_entries.feature_id` is assumed stable. The fix depends on it.
- C-08: No new test infrastructure. The Rust unit test uses existing helpers.
- Single token change: only `feature_cycle` becomes `feature_id` in the SQL string literal.
  No other character is changed.
