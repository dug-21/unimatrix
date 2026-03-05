# ADR-002: Counter Helpers Move to a `counters` Module in unimatrix-store

**Status**: Accepted
**Context**: nxs-008, Open Question #2 from SCOPE.md
**Mitigates**: SR-04 (Compat Layer Open Questions)

## Decision

Counter helper functions (`next_entry_id`, `increment_counter`, `decrement_counter`, `read_counter`, `set_counter`) are consolidated into a new `counters.rs` module in the store crate.

Currently, counter helpers exist in two places:
- `tables.rs`: `next_entry_id`, `increment_counter`, `decrement_counter` (take `&SqliteWriteTransaction`)
- `write.rs`: `read_counter`, `set_counter`, `increment_counter`, `decrement_counter` (take `&rusqlite::Connection`)

These two sets have different signatures (one takes a transaction wrapper, the other takes a raw connection). The normalized codebase needs a single set.

## Implementation

Create `crates/unimatrix-store/src/counters.rs` with:

```rust
pub(crate) fn read_counter(conn: &Connection, name: &str) -> Result<u64>;
pub(crate) fn set_counter(conn: &Connection, name: &str, value: u64) -> Result<()>;
pub(crate) fn increment_counter(conn: &Connection, name: &str, delta: u64) -> Result<()>;
pub(crate) fn decrement_counter(conn: &Connection, name: &str, delta: u64) -> Result<()>;
pub(crate) fn next_entry_id(conn: &Connection) -> Result<u64>;
```

All functions take `&rusqlite::Connection`. Callers with a `SqliteWriteTransaction` access the connection via `&*txn.guard`. This eliminates the need for the transaction-wrapper overloads in tables.rs.

The server-crate callers (`store_ops.rs`, `store_correct.rs`) that currently import `next_entry_id` and `increment_counter` from `unimatrix_store::tables` are updated to use the new `counters` module. These functions are re-exported from `lib.rs` for server access.

## Consequences

- `tables.rs` counter helpers are deleted (Wave 4)
- `write.rs` internal counter helpers are deleted; `counters.rs` is used instead
- Single source of truth for counter operations
- `counters.rs` is a stable module that survives the compat layer removal
