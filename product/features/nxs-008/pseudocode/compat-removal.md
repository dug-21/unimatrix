# Component: compat-removal (Wave 4)

## Files Deleted

- `crates/unimatrix-store/src/handles.rs` (~428 lines)
- `crates/unimatrix-store/src/dispatch.rs` (~134 lines)
- `crates/unimatrix-store/src/tables.rs` (~182 lines)

## Files Modified

- `crates/unimatrix-store/src/txn.rs` - Remove SqliteReadTransaction, mapping fns
- `crates/unimatrix-store/src/lib.rs` - Remove compat re-exports
- `crates/unimatrix-store/src/schema.rs` - Remove runtime serialize_entry/deserialize_entry

**Risk**: LOW (RISK-05 -- compat code is dead after Waves 1-3)
**ADR**: ADR-001 (keep SqliteWriteTransaction), ADR-002 (counters.rs)

## txn.rs Simplification

### Remove

- `SqliteReadTransaction` struct and impl
- `primary_key_column()` function
- `data_column()` function

### Keep

- `SqliteWriteTransaction` struct with:
  - `pub(crate) guard: MutexGuard<'a, Connection>`
  - `committed: bool`
  - `new()` -> BEGIN IMMEDIATE
  - `commit()` -> COMMIT
  - `Drop` -> ROLLBACK if not committed

### Result (~35 lines)

```rust
use std::sync::MutexGuard;
use rusqlite::Connection;
use crate::error::Result;

/// Write transaction wrapper for server compatibility (ADR-001).
pub struct SqliteWriteTransaction<'a> {
    pub(crate) guard: MutexGuard<'a, Connection>,
    committed: bool,
}

impl<'a> SqliteWriteTransaction<'a> {
    pub(crate) fn new(guard: MutexGuard<'a, Connection>) -> Result<Self> {
        guard.execute_batch("BEGIN IMMEDIATE")
            .map_err(crate::error::StoreError::Sqlite)?;
        Ok(Self { guard, committed: false })
    }

    pub fn commit(mut self) -> Result<()> {
        self.guard.execute_batch("COMMIT")
            .map_err(crate::error::StoreError::Sqlite)?;
        self.committed = true;
        Ok(())
    }
}

impl<'a> Drop for SqliteWriteTransaction<'a> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.guard.execute_batch("ROLLBACK");
        }
    }
}
```

## lib.rs Cleanup

### Remove

```rust
// DELETE these module declarations:
mod tables;
mod handles;
mod dispatch;

// DELETE these re-exports:
pub use tables::{...};
pub use handles::{...};
pub use dispatch::{...};

// DELETE:
pub use txn::SqliteReadTransaction;
```

### Keep

```rust
pub use txn::SqliteWriteTransaction;
pub use db::Store;

// Schema types
pub use schema::{EntryRecord, Status, NewEntry, QueryFilter, TimeRange, DatabaseConfig};
pub use schema::{CoAccessRecord, co_access_key, status_counter_key};
pub use schema::{AgentRecord, TrustLevel, Capability, AuditEvent, Outcome};

// Helpers
pub use hash::compute_content_hash;
pub use error::{StoreError, Result};

// Operational types
pub use signal::{SignalRecord, SignalType, SignalSource};
pub use sessions::{SessionRecord, SessionLifecycleStatus, GcStats,
                   TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS};
pub use injection_log::InjectionLogRecord;

// New public helpers for server crate
pub use read::{entry_from_row, load_tags_for_entries, apply_tags, ENTRY_COLUMNS};

// Counter module
pub mod counters;
```

### Remove from re-exports

```rust
// DELETE:
pub use schema::{serialize_entry, deserialize_entry};
pub use schema::{serialize_co_access, deserialize_co_access};
pub use signal::{serialize_signal, deserialize_signal};
```

## db.rs Cleanup

### Remove

- `begin_read()` method from Store impl

## schema.rs Cleanup

### Remove from runtime

- `serialize_entry()` function (move to migration_compat or delete)
- `deserialize_entry()` function (move to migration_compat or delete)
- `serialize_co_access()` function
- `deserialize_co_access()` function

Keep `CoAccessRecord` struct (still used as a return type).

## Verification (Static Analysis)

After deletion, grep to confirm zero references:

```bash
grep -r "open_table\|open_multimap\|begin_read\|TableU64Blob\|TableStrU64\|MultimapSpec\|TableSpec\|SqliteReadTransaction" crates/ --include="*.rs"
# Expected: 0 hits (excluding comments)

grep -r "serialize_entry\|deserialize_entry" crates/ --include="*.rs"
# Expected: only in migration_compat.rs and tests
```
