# C4: Store Pseudocode

## Purpose

`Store` wrapper struct providing database lifecycle management (open/create/compact). All other modules operate via methods on `Store`.

## Module: db.rs

### Store Struct

```
pub struct Store {
    db: redb::Database,
}
```

Store wraps redb::Database. Since Database is Send + Sync, Store is automatically Send + Sync. Shareable via Arc<Store>.

### Store::open(path)

```
fn open(path: impl AsRef<Path>) -> Result<Self>:
    open_with_config(path, DatabaseConfig::default())
```

### Store::open_with_config(path, config)

```
fn open_with_config(path: impl AsRef<Path>, config: DatabaseConfig) -> Result<Self>:
    let builder = redb::Builder::new()
    builder.set_cache_size(config.cache_size)
    let db = builder.create(path)?     // creates if absent, opens if exists

    // Ensure all 8 tables exist
    let txn = db.begin_write()?
    txn.open_table(ENTRIES)?           // creates table if absent
    txn.open_table(TOPIC_INDEX)?
    txn.open_table(CATEGORY_INDEX)?
    txn.open_multimap_table(TAG_INDEX)?
    txn.open_table(TIME_INDEX)?
    txn.open_table(STATUS_INDEX)?
    txn.open_table(VECTOR_MAP)?
    txn.open_table(COUNTERS)?
    txn.commit()?

    Ok(Store { db })
```

Note: `open_table` and `open_multimap_table` create the table if it doesn't exist when called within a write transaction.

### Store::compact(&self)

```
fn compact(&self) -> Result<()>:
    // redb::Database::compact() is a method but requires &mut self or consumes self.
    // Check redb v3.1 API -- may need to use compact() differently.
    // If compact requires ownership/mut, this may need to consume self or use interior mutability.
    // Per redb v3.1: Database::compact() -> Result<bool, CompactionError>
    self.db.compact()?
    Ok(())
```

### Internal: db(&self) accessor

```
fn db(&self) -> &redb::Database:
    &self.db
```

Used internally by write.rs, read.rs, counter.rs, query.rs for transaction creation.

## Error Handling

- `builder.create()` errors map to StoreError::Database via From impl
- `begin_write()` errors map to StoreError::Transaction
- `open_table()` errors map to StoreError::Table
- `commit()` errors map to StoreError::Storage
- `compact()` errors map to StoreError::Compaction

## Key Test Scenarios

- AC-03: Open new database, verify all 8 tables exist
- AC-14: Open creates file, close+reopen preserves data, custom cache accepted, compact succeeds
- R10: Database lifecycle edge cases
