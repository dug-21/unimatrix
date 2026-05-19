## ADR-007: Schema Migration v26→v27 — Four Indexes for context_graph

### Context

Four indexes are missing from the current schema (v26) that are required for
efficient `context_graph` query performance:

1. `idx_entries_supersedes ON entries(supersedes)` — required for the forward CTE
   step: `FROM entries e JOIN chain c ON e.supersedes = c.id`. Without it, each
   recursive step does a full `entries` table scan. At 3k entries, this is <2ms per
   step; at 30k+ entries, it becomes unacceptable.

2. `idx_entries_superseded_by ON entries(superseded_by)` — required for the backward
   CTE step: `FROM entries e JOIN chain c ON e.superseded_by = c.id`. Same rationale.

3. `idx_graph_edges_source_type ON graph_edges(source_id, relation_type)` — required
   for outgoing neighbor queries: `WHERE source_id = ?1 AND relation_type IN (...)`.
   Without it, the query uses `idx_graph_edges_source_id` and applies the
   `relation_type IN` filter in memory. The composite index collapses this into a
   single range scan.

4. `idx_graph_edges_target_type ON graph_edges(target_id, relation_type)` — required
   for incoming neighbor queries: `WHERE target_id = ?1 AND relation_type IN (...)`.
   Same rationale as index 3.

Indexes 3 and 4 are also used by `inverse` and `filter` modes (W1B-2c, #598) — they
query patterns like "entries in category X with zero incoming edges of type Y", which
requires scanning by `(target_id, relation_type)`. Adding them in vnc-018 avoids a
second migration at #598 delivery time.

SR-01 (scope risk) flags that these indexes must be applied before any `context_graph`
handler is reachable. The migration sequencing in `migrate_if_needed` guarantees this:
`migrate_if_needed` runs to completion before the connection pools are constructed,
and the pools are constructed before the MCP server starts accepting connections.

### Decision

Add a v26→v27 migration block in `migration.rs` with all four indexes:

```rust
// v26 → v27: indexes for context_graph CTE and neighbor queries (vnc-018, GH #596).
if current_version < 27 {
    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_entries_supersedes ON entries(supersedes)"
    )
    .execute(&mut **txn).await.map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_entries_superseded_by ON entries(superseded_by)"
    )
    .execute(&mut **txn).await.map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_graph_edges_source_type \
         ON graph_edges(source_id, relation_type)"
    )
    .execute(&mut **txn).await.map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_graph_edges_target_type \
         ON graph_edges(target_id, relation_type)"
    )
    .execute(&mut **txn).await.map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    sqlx::query("UPDATE counters SET value = 27 WHERE name = 'schema_version'")
        .execute(&mut **txn).await.map_err(|e| StoreError::Migration { source: Box::new(e) })?;
}
```

`CURRENT_SCHEMA_VERSION` bumped to 27.

The same four `CREATE INDEX IF NOT EXISTS` statements are added to
`db.rs::create_tables_if_needed()` for fresh database consistency (the established
pattern for all indexes).

This is an **index-only migration**. No new tables, no new columns, no data back-fill.
The schema cascade checklist (Pattern #4373) applies in reduced form:

- `migration.rs`: add v26→v27 block, bump `CURRENT_SCHEMA_VERSION` to 27.
- `db.rs`: add 4 index DDL calls to `create_tables_if_needed`; bump schema_version
  literal to 27.
- `sqlite_parity.rs`: update `test_schema_version_is_26` → 27; add 4 index-existence
  assertions. Column-count assertions do NOT change (no new columns).
- `server.rs`: update all `assert_eq!(version, 26)` to 27.
- `migration_v25_to_v26.rs`: rename exact-version assertion to `>= 26`.
- New file `migration_v26_to_v27.rs`: assert all 4 index names exist.
- `db.rs` `test_schema_version_initialized_to_current_on_fresh_db`: update expected
  value to 27.

### Consequences

Easier: CTE recursive steps are indexed — supersession chain queries are O(log N)
per step rather than O(N). Composite index makes neighbor queries single range scans.
The migration is fully idempotent (`CREATE INDEX IF NOT EXISTS`) — safe to re-run.
The #598 delivery increment gets indexes 3 and 4 for free.

Harder: standard schema cascade checklist must be followed precisely (Pattern #4373
documents all touch points). The delivery agent must run
`grep -r 'schema_version.*== 26' crates/` after the bump and confirm zero matches
before marking migration complete. Missing one assertion in an older migration test
will cause a test failure that is not immediately obvious.
