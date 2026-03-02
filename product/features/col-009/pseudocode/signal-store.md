# Pseudocode: signal-store

## Purpose

Add the SIGNAL_QUEUE redb table (schema v4) and the `SignalRecord` types used throughout col-009. Adds `insert_signal`, `drain_signals`, `signal_queue_len` to `Store`. This is the persistence layer for the signal pipeline.

## Files

- CREATE `crates/unimatrix-store/src/signal.rs`
- MODIFY `crates/unimatrix-store/src/schema.rs` — add SIGNAL_QUEUE table definition
- MODIFY `crates/unimatrix-store/src/migration.rs` — bump to v4, add migrate_v3_to_v4
- MODIFY `crates/unimatrix-store/src/db.rs` — add Store methods
- MODIFY `crates/unimatrix-store/src/lib.rs` — re-export signal module

## New File: `signal.rs`

```rust
use serde::{Deserialize, Serialize};

// LAYOUT FROZEN: bincode v2 positional encoding. Fields may only be APPENDED.
// See ADR-001 (col-009). Do not reorder or remove fields.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SignalRecord {
    pub signal_id: u64,       // field 0 — monotonic key
    pub session_id: String,   // field 1
    pub created_at: u64,      // field 2 — Unix seconds
    pub entry_ids: Vec<u64>,  // field 3 — deduplicated
    pub signal_type: SignalType,    // field 4
    pub signal_source: SignalSource, // field 5
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum SignalType {
    Helpful = 0,
    Flagged = 1,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(u8)]
pub enum SignalSource {
    ImplicitOutcome = 0,
    ImplicitRework = 1,
}

pub fn serialize_signal(record: &SignalRecord) -> crate::error::Result<Vec<u8>> {
    // Use bincode::serde::encode_to_vec with standard() config — same as EntryRecord
    let bytes = bincode::serde::encode_to_vec(record, bincode::config::standard())?;
    Ok(bytes)
}

pub fn deserialize_signal(bytes: &[u8]) -> crate::error::Result<SignalRecord> {
    let (record, _) = bincode::serde::decode_from_slice::<SignalRecord, _>(
        bytes, bincode::config::standard()
    )?;
    Ok(record)
}
```

## Modification: `schema.rs`

Add after OBSERVATION_METRICS:

```rust
/// Signal work queue: signal_id -> bincode bytes (SignalRecord).
/// Transient — records deleted after drain. Schema v4 (col-009).
pub const SIGNAL_QUEUE: TableDefinition<u64, &[u8]> =
    TableDefinition::new("signal_queue");
```

## Modification: `migration.rs`

```rust
// Change CURRENT_SCHEMA_VERSION from 3 to 4
pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 4;

// In migrate_if_needed(), add:
} else if current_version == 3 {
    migrate_v3_to_v4(&txn)?;
}

// New function:
fn migrate_v3_to_v4(txn: &redb::WriteTransaction) -> Result<()> {
    // Open SIGNAL_QUEUE — this triggers redb table creation
    txn.open_table(SIGNAL_QUEUE)?;

    // Write next_signal_id = 0 to COUNTERS (only if absent)
    {
        let mut counters = txn.open_table(COUNTERS)?;
        if counters.get("next_signal_id")?.is_none() {
            counters.insert("next_signal_id", 0u64)?;
        }
    }

    // No entry scan-and-rewrite needed (SIGNAL_QUEUE is new, no existing data)
    Ok(())
}
```

IMPORTANT: The existing `migrate_if_needed` structure updates `schema_version` at the end of the function after the branch. This remains unchanged. The migrate_v3_to_v4 function does NOT write schema_version itself.

## Modification: `db.rs` — Store methods

Add to `impl Store`:

```rust
/// Insert a signal into SIGNAL_QUEUE.
///
/// Allocates a new signal_id from the COUNTERS table (next_signal_id).
/// Enforces the 10,000-record cap: if signal_queue_len() >= 10_000,
/// deletes the oldest record (lowest signal_id) before inserting.
/// Returns the allocated signal_id.
pub fn insert_signal(&self, record: &SignalRecord) -> Result<u64> {
    let txn = self.db.begin_write()?;
    let signal_id = {
        // 1. Read and increment next_signal_id from COUNTERS
        let mut counters = txn.open_table(COUNTERS)?;
        let next_id = match counters.get("next_signal_id")? {
            Some(guard) => guard.value(),
            None => 0u64,
        };
        counters.insert("next_signal_id", next_id + 1)?;

        // 2. Enforce cap: if queue >= 10_000, delete oldest (lowest signal_id)
        {
            let mut queue = txn.open_table(SIGNAL_QUEUE)?;
            let current_len = queue.len()?;
            if current_len >= 10_000 {
                // Find and delete oldest (first key in ascending order)
                let oldest_key = {
                    let iter = queue.iter()?;
                    iter.next()
                        .and_then(|r| r.ok())
                        .map(|(k, _)| k.value())
                };
                if let Some(k) = oldest_key {
                    queue.remove(k)?;
                }
            }
        }

        // 3. Insert new record with allocated signal_id
        let mut full_record = record.clone();
        full_record.signal_id = next_id;
        let bytes = crate::signal::serialize_signal(&full_record)?;
        {
            let mut queue = txn.open_table(SIGNAL_QUEUE)?;
            queue.insert(next_id, bytes.as_slice())?;
        }

        next_id
    };
    txn.commit()?;
    Ok(signal_id)
}

/// Drain all SignalRecords of the given signal_type from SIGNAL_QUEUE.
///
/// Reads all matching records and deletes them in a single write transaction.
/// Returns the drained records.
/// Idempotent on empty queue — returns Ok(empty Vec).
pub fn drain_signals(&self, signal_type: SignalType) -> Result<Vec<SignalRecord>> {
    let txn = self.db.begin_write()?;
    let mut drained = Vec::new();
    let mut keys_to_delete = Vec::new();

    {
        let queue = txn.open_table(SIGNAL_QUEUE)?;
        // Scan all records, collect matching ones
        for entry in queue.iter()? {
            let (k, v) = entry?;
            let key = k.value();
            let bytes = v.value();
            match crate::signal::deserialize_signal(bytes) {
                Ok(record) if record.signal_type == signal_type => {
                    keys_to_delete.push(key);
                    drained.push(record);
                }
                Ok(_) => {} // Different signal_type — leave it
                Err(e) => {
                    // Log warning, skip corrupted record, still delete to avoid
                    // re-processing junk data on every drain
                    tracing::warn!("drain_signals: failed to deserialize record {key}: {e}");
                    keys_to_delete.push(key);
                }
            }
        }
    }

    // Delete matched records
    {
        let mut queue = txn.open_table(SIGNAL_QUEUE)?;
        for key in &keys_to_delete {
            queue.remove(key)?;
        }
    }

    txn.commit()?;
    Ok(drained)
}

/// Return the total count of all records in SIGNAL_QUEUE (any signal_type).
pub fn signal_queue_len(&self) -> Result<u64> {
    let txn = self.db.begin_read()?;
    let queue = txn.open_table(SIGNAL_QUEUE)?;
    Ok(queue.len()?)
}
```

## Modification: `db.rs` — Store::open_with_config

In `open_with_config`, add SIGNAL_QUEUE to the table creation block:

```rust
txn.open_table(SIGNAL_QUEUE).map_err(StoreError::Table)?;
```

(Add after `OBSERVATION_METRICS` in the existing block)

## Modification: `lib.rs`

```rust
pub mod signal;
pub use signal::{SignalRecord, SignalSource, SignalType};
```

## Error Handling

- `insert_signal`: propagates `StoreError` from redb operations; serialization errors from `serialize_signal`
- `drain_signals`: per-record deserialization errors are logged as warnings and the record is deleted; the drain continues
- `signal_queue_len`: propagates redb read errors

## Key Test Scenarios

1. `test_insert_signal_allocates_monotonic_ids` — two inserts return 0, 1
2. `test_insert_signal_cap_drops_oldest` — insert 10,001 signals; len == 10,000; signal_id=0 absent
3. `test_drain_signals_by_type` — insert 2 Helpful + 1 Flagged; drain Helpful returns 2; drain Flagged returns 1
4. `test_drain_signals_idempotent` — drain empty queue returns `Ok([])`
5. `test_drain_signals_deletes_records` — drain twice; second call returns empty
6. `test_signal_queue_len_counts_all_types` — 1 Helpful + 1 Flagged → len == 2
7. `test_migration_v3_to_v4` — open v3 db with 10 entries; after open: schema_version==4, SIGNAL_QUEUE exists, next_signal_id==0, all 10 entries intact
8. `test_signal_record_roundtrip` — serialize + deserialize preserves all fields
9. `test_current_schema_version_is_4` — assert CURRENT_SCHEMA_VERSION == 4
