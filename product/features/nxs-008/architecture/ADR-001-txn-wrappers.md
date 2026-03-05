# ADR-001: Keep SqliteWriteTransaction, Remove SqliteReadTransaction

**Status**: Accepted
**Context**: nxs-008, Open Question #1 from SCOPE.md
**Mitigates**: SR-04 (Compat Layer Open Questions)

## Decision

**Keep `SqliteWriteTransaction`; remove `SqliteReadTransaction`.**

`SqliteWriteTransaction` provides real value: it wraps `BEGIN IMMEDIATE` / `COMMIT` / `ROLLBACK` with RAII Drop semantics (auto-rollback if not committed). This is a genuine transaction safety abstraction, not a redb-pattern artifact. The server's `write_in_txn` pattern (audit.rs, store_ops.rs, store_correct.rs) depends on it for atomic multi-table writes.

`SqliteReadTransaction` is a zero-value wrapper around `MutexGuard<Connection>`. It exists only because the redb API required a read transaction to open tables. With direct SQL, callers can use `Store::lock_conn()` directly. Its `open_table` / `open_multimap_table` methods only exist to power the compat layer dispatch (dispatch.rs), which is removed.

## Implementation

1. `txn.rs` retains `SqliteWriteTransaction` with its current API: `new()`, `commit()`, `Drop` rollback
2. `SqliteReadTransaction` is removed. Server read paths use `Store::lock_conn()` or new Store methods
3. `Store::begin_write()` remains; `Store::begin_read()` is removed
4. The `primary_key_column()` and `data_column()` helper functions in txn.rs are removed (only used by compat handles)
5. Waves 1-3 use `SqliteWriteTransaction` for transactional writes and `Store::lock_conn()` for reads
6. Wave 4 deletes `SqliteReadTransaction` and the column-mapping functions

## Consequences

- Server code that uses `begin_read()` + `open_table()` is rewritten to use Store methods or `lock_conn()` directly
- `SqliteWriteTransaction` no longer exposes `open_table` / `open_multimap_table` after Wave 4; server code uses direct SQL via `&*txn.guard`
- The `guard` field on `SqliteWriteTransaction` becomes `pub(crate)` scoped (server accesses it via a helper method or public accessor)
