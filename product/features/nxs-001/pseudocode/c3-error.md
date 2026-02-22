# C3: Error Pseudocode

## Purpose

Single `StoreError` enum covering all failure modes. Provides ergonomic `?` propagation via `From` impls.

## Module: error.rs

### StoreError Enum

```
#[derive(Debug)]
enum StoreError {
    EntryNotFound(u64),
    Database(redb::DatabaseError),
    Transaction(redb::TransactionError),
    Table(redb::TableError),
    Storage(redb::StorageError),
    Compaction(redb::CompactionError),
    Serialization(String),
    Deserialization(String),
    InvalidStatus(u8),
}
```

### Display Implementation

Each variant produces a descriptive message:
- EntryNotFound(id) -> "entry not found: {id}"
- Database(e) -> "database error: {e}"
- Transaction(e) -> "transaction error: {e}"
- Table(e) -> "table error: {e}"
- Storage(e) -> "storage error: {e}"
- Compaction(e) -> "compaction error: {e}"
- Serialization(msg) -> "serialization error: {msg}"
- Deserialization(msg) -> "deserialization error: {msg}"
- InvalidStatus(byte) -> "invalid status byte: {byte}"

### Error Trait Implementation

```
impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)>
        // Return inner error for Database, Transaction, Table, Storage, Compaction
        // Return None for others
}
```

### From Implementations

```
impl From<redb::DatabaseError> for StoreError -> Database(e)
impl From<redb::TransactionError> for StoreError -> Transaction(e)
impl From<redb::TableError> for StoreError -> Table(e)
impl From<redb::StorageError> for StoreError -> Storage(e)
impl From<redb::CompactionError> for StoreError -> Compaction(e)
```

For bincode errors, wrap as String since bincode v2 error types are EncodeError/DecodeError:
```
impl From<bincode::error::EncodeError> for StoreError -> Serialization(e.to_string())
impl From<bincode::error::DecodeError> for StoreError -> Deserialization(e.to_string())
```

### Result Type Alias

```
pub type Result<T> = std::result::Result<T, StoreError>;
```

## Error Handling

- No panics. All paths return Result.
- redb error types are wrapped, not re-exported.
- Callers do not need to depend on redb directly.

## Key Test Scenarios

- AC-15: Each StoreError variant is constructible and displays meaningful message
- R12: Error type discrimination (distinguish redb vs serialization vs application errors)
- Verify Display output contains relevant context
