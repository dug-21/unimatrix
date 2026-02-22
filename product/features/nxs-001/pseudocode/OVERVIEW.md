# nxs-001 Pseudocode Overview

## Components and Data Flow

```
C1: crate-setup  -->  Cargo.toml files (workspace + crate)
C2: schema       -->  EntryRecord, Status, NewEntry, QueryFilter, TimeRange, DatabaseConfig, 8 table constants
C3: error        -->  StoreError enum, Result<T> alias
C4: store        -->  Store struct wrapping redb::Database (open, open_with_config, compact)
C5: counter      -->  next_entry_id(), read_counter(), increment/decrement (operate on WriteTransaction)
C6: write        -->  Store::insert, update, update_status, delete, put_vector_mapping
C7: read         -->  Store::get, exists, query_by_topic/category/tags/time_range/status, get_vector_mapping, read_counter
C8: query        -->  Store::query(QueryFilter) -- set intersection across index queries
C9: test-infra   -->  TestDb, TestEntry builder, assert_index_consistent/absent, seed_entries
C10: lib         -->  pub re-exports from all modules
```

## Data Flow: Write Path

```
Caller -> Store::insert(NewEntry)
  -> begin_write()
  -> counter::next_entry_id(&txn) -> u64
  -> build EntryRecord (id, created_at, updated_at assigned)
  -> bincode::serde::encode_to_vec(&record, config) -> Vec<u8>
  -> txn.open_table(ENTRIES).insert(id, bytes)
  -> txn.open_table(TOPIC_INDEX).insert((topic, id), ())
  -> txn.open_table(CATEGORY_INDEX).insert((category, id), ())
  -> for tag: txn.open_multimap_table(TAG_INDEX).insert(tag, id)
  -> txn.open_table(TIME_INDEX).insert((created_at, id), ())
  -> txn.open_table(STATUS_INDEX).insert((status as u8, id), ())
  -> counter::increment_counter(&txn, status_counter_key, 1)
  -> txn.commit()
  -> Ok(id)
```

## Data Flow: Read Path (Combined Query)

```
Caller -> Store::query(QueryFilter)
  -> begin_read()
  -> for each present filter field: collect HashSet<u64> of matching IDs
  -> intersect all sets -> Vec<u64>
  -> batch fetch from ENTRIES -> deserialize each -> Vec<EntryRecord>
  -> Ok(results)
```

## Shared Types

All shared types live in `schema.rs` (C2):
- `EntryRecord` -- Serialize + Deserialize, PartialEq, Clone, Debug
- `Status` -- repr(u8), Serialize + Deserialize, Copy, PartialEq, Eq, Hash
- `NewEntry` -- insert input struct
- `QueryFilter` -- Default, Clone, Debug
- `TimeRange` -- Copy, Clone, Debug
- `DatabaseConfig` -- Default, Clone, Debug
- 8 table definition constants

Error types live in `error.rs` (C3):
- `StoreError` -- Debug, Display, Error
- `type Result<T> = std::result::Result<T, StoreError>`

## Sequencing Constraints

Phase 1 (no deps): C1, C2, C3
Phase 2 (needs Phase 1): C4, C5
Phase 3 (needs Phase 2): C6, C7
Phase 4 (needs Phase 3): C8, C10
Phase 5 (needs all): C9

## Bincode Configuration

All serialization uses:
```
let config = bincode::config::standard();
bincode::serde::encode_to_vec(&record, config)
bincode::serde::decode_from_slice::<EntryRecord, _>(&bytes, config)
```

NEVER use `bincode::encode_to_vec` (native Encode/Decode path). Only the serde-compatible functions respect `#[serde(default)]`.
