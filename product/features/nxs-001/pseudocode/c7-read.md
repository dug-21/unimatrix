# C7: Read Pseudocode

## Purpose

All read operations using redb ReadTransaction (MVCC snapshots). Point lookups and individual index queries. The combined QueryFilter query lives in C8.

## Module: read.rs

### Store::get(&self, entry_id: u64) -> Result<EntryRecord>

```
fn get(&self, entry_id: u64) -> Result<EntryRecord>:
    let txn = self.db.begin_read()?
    let table = txn.open_table(ENTRIES)?
    match table.get(entry_id)?:
        Some(guard) -> deserialize_entry(guard.value())
        None -> Err(StoreError::EntryNotFound(entry_id))
```

### Store::exists(&self, entry_id: u64) -> Result<bool>

```
fn exists(&self, entry_id: u64) -> Result<bool>:
    let txn = self.db.begin_read()?
    let table = txn.open_table(ENTRIES)?
    match table.get(entry_id)?:
        Some(_) -> Ok(true)
        None -> Ok(false)
```

### Store::query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>>

```
fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>>:
    let txn = self.db.begin_read()?
    let ids = collect_ids_by_topic(&txn, topic)?
    fetch_entries(&txn, &ids)
```

Internal helper (shared with C8):

```
fn collect_ids_by_topic(txn: &ReadTransaction, topic: &str) -> Result<HashSet<u64>>:
    let table = txn.open_table(TOPIC_INDEX)?
    let range_start = (topic, 0u64)
    let range_end = (topic, u64::MAX)
    let mut ids = HashSet::new()
    for result in table.range(range_start..=range_end)?:
        let (key, _) = result?
        let (_, entry_id) = key.value()
        ids.insert(entry_id)
    Ok(ids)
```

### Store::query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>>

```
fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>>:
    let txn = self.db.begin_read()?
    let ids = collect_ids_by_category(&txn, category)?
    fetch_entries(&txn, &ids)
```

Internal:

```
fn collect_ids_by_category(txn: &ReadTransaction, category: &str) -> Result<HashSet<u64>>:
    // Same pattern as topic: range scan on (category, 0)..=(category, u64::MAX)
    let table = txn.open_table(CATEGORY_INDEX)?
    let mut ids = HashSet::new()
    for result in table.range((category, 0u64)..=(category, u64::MAX))?:
        let (key, _) = result?
        ids.insert(key.value().1)
    Ok(ids)
```

### Store::query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>>

```
fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>>:
    if tags.is_empty():
        return Ok(vec![])

    let txn = self.db.begin_read()?
    let ids = collect_ids_by_tags(&txn, tags)?
    fetch_entries(&txn, &ids)
```

Internal:

```
fn collect_ids_by_tags(txn: &ReadTransaction, tags: &[String]) -> Result<HashSet<u64>>:
    let table = txn.open_multimap_table(TAG_INDEX)?
    let mut result_set: Option<HashSet<u64>> = None

    for tag in tags:
        let mut tag_ids = HashSet::new()
        let values = table.get(tag.as_str())?
        for result in values:
            let guard = result?
            tag_ids.insert(guard.value())

        result_set = match result_set:
            None -> Some(tag_ids)               // first tag: use its set
            Some(existing) -> Some(existing.intersection(&tag_ids).copied().collect())
                                                // subsequent: intersect

    Ok(result_set.unwrap_or_default())
```

### Store::query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>>

```
fn query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>>:
    if range.start > range.end:
        return Ok(vec![])    // inverted range returns empty

    let txn = self.db.begin_read()?
    let ids = collect_ids_by_time_range(&txn, range)?
    fetch_entries(&txn, &ids)
```

Internal:

```
fn collect_ids_by_time_range(txn: &ReadTransaction, range: TimeRange) -> Result<HashSet<u64>>:
    let table = txn.open_table(TIME_INDEX)?
    let mut ids = HashSet::new()
    for result in table.range((range.start, 0u64)..=(range.end, u64::MAX))?:
        let (key, _) = result?
        ids.insert(key.value().1)
    Ok(ids)
```

### Store::query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>>

```
fn query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>>:
    let txn = self.db.begin_read()?
    let ids = collect_ids_by_status(&txn, status)?
    fetch_entries(&txn, &ids)
```

Internal:

```
fn collect_ids_by_status(txn: &ReadTransaction, status: Status) -> Result<HashSet<u64>>:
    let table = txn.open_table(STATUS_INDEX)?
    let status_byte = status as u8
    let mut ids = HashSet::new()
    for result in table.range((status_byte, 0u64)..=(status_byte, u64::MAX))?:
        let (key, _) = result?
        ids.insert(key.value().1)
    Ok(ids)
```

### Store::get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>>

```
fn get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>>:
    let txn = self.db.begin_read()?
    let table = txn.open_table(VECTOR_MAP)?
    match table.get(entry_id)?:
        Some(guard) -> Ok(Some(guard.value()))
        None -> Ok(None)
```

### Shared Helper: fetch_entries(txn, ids)

```
fn fetch_entries(txn: &ReadTransaction, ids: &HashSet<u64>) -> Result<Vec<EntryRecord>>:
    let table = txn.open_table(ENTRIES)?
    let mut results = Vec::with_capacity(ids.len())
    for &id in ids:
        match table.get(id)?:
            Some(guard) -> results.push(deserialize_entry(guard.value())?)
            None -> {}   // entry might have been deleted between index scan and fetch
    Ok(results)
```

## Internal Functions Exported to C8

The `collect_ids_by_*` functions must be accessible from query.rs (C8). Make them `pub(crate)` and accept `&ReadTransaction` so C8 can call them within its own transaction.

## Error Handling

- ReadTransaction errors -> TransactionError
- Table open errors -> TableError
- Missing entry on get() -> EntryNotFound
- Missing entry on exists() -> Ok(false), not error
- Missing vector mapping -> Ok(None), not error
- Deserialization failures -> DeserializationError

## Key Test Scenarios

- AC-06: Get by ID, verify fields match. Get nonexistent returns EntryNotFound.
- AC-07: Query by topic, correct entries returned.
- AC-08: Query by category, correct entries returned.
- AC-09: Query by tags (intersection), edge cases.
- AC-10: Time range query, boundary conditions.
- AC-11: Status query.
- AC-13: Vector mapping lookup.
- R1: Index queries match direct ENTRIES lookup.
