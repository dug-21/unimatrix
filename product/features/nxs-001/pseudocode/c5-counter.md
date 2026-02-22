# C5: Counter Pseudocode

## Purpose

Atomic ID generation and statistical counters. All counter operations take a write transaction reference (not &Store) to execute within the caller's transaction.

## Module: counter.rs

### next_entry_id(txn: &WriteTransaction) -> Result<u64>

```
fn next_entry_id(txn: &WriteTransaction) -> Result<u64>:
    let mut table = txn.open_table(COUNTERS)?
    let current = match table.get("next_entry_id")? {
        Some(guard) -> guard.value(),
        None -> 0,    // first call: no key exists yet
    }
    let id = if current == 0 { 1 } else { current }
    // Store next value
    table.insert("next_entry_id", id + 1)?
    Ok(id)
```

First call: key missing, defaults to 0, assigns ID 1, stores 2 as next.
Subsequent: reads current, assigns current, stores current + 1.

Wait -- re-reading the spec more carefully. FR-06.1 says "reads 0, stores 1, returns 1" or "reads missing, defaults to 1, stores 2, returns 1". Let me use the cleaner pattern:

```
fn next_entry_id(txn: &WriteTransaction) -> Result<u64>:
    let mut table = txn.open_table(COUNTERS)?
    let current = match table.get("next_entry_id")? {
        Some(guard) -> guard.value(),
        None -> 1,    // first ID is 1 (not 0)
    }
    table.insert("next_entry_id", current + 1)?
    Ok(current)
```

This way: first call returns 1, stores 2. Second call returns 2, stores 3. ID 0 is never assigned.

### read_counter(db: &Database, key: &str) -> Result<u64>

```
fn read_counter_value(db: &redb::Database, key: &str) -> Result<u64>:
    let txn = db.begin_read()?
    let table = txn.open_table(COUNTERS)?
    match table.get(key)? {
        Some(guard) -> Ok(guard.value()),
        None -> Ok(0),    // missing key returns 0
    }
```

This is a standalone read (not within a write transaction). For Store method exposure, this becomes:

```
impl Store:
    fn read_counter(&self, name: &str) -> Result<u64>:
        read_counter_value(&self.db, name)
```

### increment_counter(txn, key, delta)

```
fn increment_counter(txn: &WriteTransaction, key: &str, delta: u64) -> Result<()>:
    let mut table = txn.open_table(COUNTERS)?
    let current = match table.get(key)? {
        Some(guard) -> guard.value(),
        None -> 0,
    }
    table.insert(key, current + delta)?
    Ok(())
```

### decrement_counter(txn, key, delta)

```
fn decrement_counter(txn: &WriteTransaction, key: &str, delta: u64) -> Result<()>:
    let mut table = txn.open_table(COUNTERS)?
    let current = match table.get(key)? {
        Some(guard) -> guard.value(),
        None -> 0,
    }
    table.insert(key, current.saturating_sub(delta))?
    Ok(())
```

Uses saturating_sub to prevent underflow. A counter should never go below 0.

## Counter Keys

- `"next_entry_id"` -- next ID to assign
- `"total_active"` -- entries with Status::Active
- `"total_deprecated"` -- entries with Status::Deprecated
- `"total_proposed"` -- entries with Status::Proposed

### status_counter_key(status: Status) -> &'static str

```
fn status_counter_key(status: Status) -> &'static str:
    match status:
        Active -> "total_active"
        Deprecated -> "total_deprecated"
        Proposed -> "total_proposed"
```

## Error Handling

- All operations propagate table/transaction errors via `?`
- Missing keys return 0 (not error)
- next_entry_id operates within caller's transaction (no separate transaction)

## Key Test Scenarios

- AC-05: 100 sequential inserts, all IDs monotonically increasing, first ID is 1
- R5: Counter reads back match inserts; no ID gaps on successful inserts
- Verify counter key "next_entry_id" matches last_id + 1 after inserts
