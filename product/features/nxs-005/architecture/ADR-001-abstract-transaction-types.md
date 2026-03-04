## ADR-001: Abstract Transaction Types Behind Feature Flag

### Context

`Store::begin_read()` returns `redb::ReadTransaction` and `Store::begin_write()` returns `redb::WriteTransaction`. These are public methods used by unimatrix-server to directly access AGENT_REGISTRY and AUDIT_LOG tables. Under the `backend-sqlite` feature, these redb types do not exist.

The cleanest solution would be to move all agent registry and audit log operations into Store methods (eliminating the need for external transaction access entirely). However, this was considered and rejected because the server's agent registry and audit log code is substantial (~200 lines each) and tightly coupled to the server's request handling flow. Moving it would be a significant refactoring that exceeds nxs-005's zero-change-outside-store scope.

### Decision

Use conditional compilation with type aliases to abstract the transaction return types:

```rust
// In db.rs or a new types.rs
#[cfg(not(feature = "backend-sqlite"))]
pub type ReadTransaction<'a> = redb::ReadTransaction;
#[cfg(not(feature = "backend-sqlite"))]
pub type WriteTransaction<'a> = redb::WriteTransaction;

#[cfg(feature = "backend-sqlite")]
pub type ReadTransaction<'a> = SqliteReadTransaction<'a>;
#[cfg(feature = "backend-sqlite")]
pub type WriteTransaction<'a> = SqliteWriteTransaction<'a>;
```

Where `SqliteReadTransaction` and `SqliteWriteTransaction` are thin wrappers around `MutexGuard<'a, rusqlite::Connection>` that expose table-open methods compatible with the server's usage pattern.

The server code that opens AGENT_REGISTRY and AUDIT_LOG will need minimal cfg adjustments -- the transaction wrappers provide equivalent `open_table()` / `open_multimap_table()` methods that return SQL-based table handles.

**Alternative considered**: Making `begin_read`/`begin_write` return trait objects. Rejected because the server code uses redb-specific table handle types throughout, and the call sites are too numerous for dynamic dispatch to be ergonomic.

**Alternative considered**: Feature-flag the entire unimatrix-server crate. Rejected -- violates the "no changes outside store crate" constraint (AC-15). However, the server does import `redb::ReadTransaction` directly. The type aliases in the store crate's public API resolve this: the server imports `unimatrix_store::ReadTransaction` instead of `redb::ReadTransaction`.

### Consequences

- Server code must change imports from `redb::ReadTransaction` to `unimatrix_store::ReadTransaction`. This is a minor change outside the store crate, but it is unavoidable and minimal.
- The type aliases are transparent -- redb users get the exact same type they had before.
- SQLite transaction wrappers must implement enough of the redb-like API to satisfy server usage.
- Future features (nxs-006) that eliminate direct transaction access will make this ADR obsolete.
