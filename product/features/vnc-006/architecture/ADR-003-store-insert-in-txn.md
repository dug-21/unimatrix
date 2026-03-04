## ADR-003: Store::insert_in_txn for Atomic Write+Audit

### Context

Currently, `Store::insert()` opens its own `WriteTransaction`, writes the entry and all indexes, and commits. Audit logging happens separately after the insert returns. This means:

1. An entry can be inserted but the audit record lost (crash between insert and audit)
2. The service layer cannot add additional writes (audit, co-access) atomically with the entry insert

The service layer needs to write audit records in the same transaction as the entry insert, ensuring atomicity.

### Decision

Add `Store::insert_in_txn()` that accepts an external `&WriteTransaction` and performs all index writes within it. The caller (StoreService) manages the transaction lifecycle:

```rust
impl Store {
    /// Insert an entry within an externally-managed transaction.
    /// The caller is responsible for committing the transaction.
    pub(crate) fn insert_in_txn(
        &self,
        txn: &WriteTransaction,
        entry: NewEntry,
        now: u64,
    ) -> Result<EntryRecord> {
        let id = counter::next_entry_id(txn)?;
        // ... build EntryRecord, write ENTRIES, indexes, counters ...
        Ok(record)
    }
}
```

The existing `Store::insert()` is preserved unchanged — it continues to manage its own transaction. `insert_in_txn` is the new method that StoreService uses.

Transaction flow in StoreService:
```rust
let txn = self.store.begin_write()?;
let record = self.store.insert_in_txn(&txn, entry, now)?;
// Write audit record in same transaction
audit_table.insert(event_id, audit_bytes)?;
txn.commit()?;
```

`WriteTransaction` is redb's type and stays `pub(crate)` — it does not appear in any public API of unimatrix-store.

### Consequences

- **Easier**: Atomic entry+audit writes. Future atomic operations (entry+vector in same txn) follow the same pattern. StoreService can compose multiple writes atomically.
- **Harder**: `insert_in_txn` duplicates some code from `insert` (or `insert` calls `insert_in_txn` internally). Transaction lifetime management moves to the service layer, which must handle commit/rollback correctly.
