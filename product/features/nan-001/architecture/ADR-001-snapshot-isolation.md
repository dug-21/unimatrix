## ADR-001: Snapshot Isolation via BEGIN DEFERRED Transaction

### Context

The export reads 8 tables sequentially. If the MCP server is running concurrently and writes to the database between table reads, the export could contain rows from different logical states — for example, an `entry_tags` row referencing an entry that was created after the `entries` table was already read (SR-07 from the Scope Risk Assessment).

SQLite WAL mode provides snapshot isolation per-statement by default, not per-multi-statement-sequence. Without an explicit transaction, each `SELECT` sees the latest committed state at the time it executes.

Alternatives considered:
- **No transaction**: Simpler code but violates AC-14 (deterministic output) under concurrent writes. Unacceptable for a backup/restore format.
- **BEGIN IMMEDIATE**: Acquires a write lock. Unnecessary — export only reads. Would block the MCP server's writes for the duration of export.
- **BEGIN EXCLUSIVE**: Even more restrictive. Same objection as IMMEDIATE.

### Decision

Wrap the entire export in a single `BEGIN DEFERRED` transaction. This acquires a read snapshot at the first read statement and holds it until `COMMIT`. All 8 table reads see a consistent point-in-time view of the database.

The transaction is started immediately after `Store::open()` and `lock_conn()`:

```rust
conn.execute_batch("BEGIN DEFERRED")?;
// ... all table reads ...
conn.execute_batch("COMMIT")?;
```

`DEFERRED` is the default transaction type in SQLite. It does not acquire any locks until the first read, at which point it acquires a shared (read) lock. This allows the MCP server to continue writing via WAL — the export sees the snapshot, the server writes to new WAL frames.

### Consequences

- **Positive**: Export output is a consistent snapshot of the database, even under concurrent MCP server writes. Satisfies AC-14 (deterministic output for a given state).
- **Positive**: No write blocking — the MCP server is unaffected.
- **Negative**: The read snapshot is held for the duration of the export. For large databases, this may delay WAL checkpoint. At the expected scale (<1000 entries, <5 seconds), this is negligible.
- **Negative**: The `Store::lock_conn()` mutex is held for the entire export duration. If any other thread in the same process needed the connection, it would block. Since export runs as a standalone CLI invocation (not inside the MCP server process), this is not a concern.
