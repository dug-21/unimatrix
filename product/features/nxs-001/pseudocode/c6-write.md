# C6: Write Pseudocode

## Purpose

All write operations: insert, update, update_status, delete, put_vector_mapping. Each operates as an atomic multi-table transaction.

## Module: write.rs

### Store::insert(&self, entry: NewEntry) -> Result<u64>

```
fn insert(&self, entry: NewEntry) -> Result<u64>:
    let now = current_unix_timestamp_secs()
    let txn = self.db.begin_write()?

    // Step 1: Generate ID
    let id = counter::next_entry_id(&txn)?

    // Step 2: Build EntryRecord
    let record = EntryRecord {
        id,
        title: entry.title,
        content: entry.content,
        topic: entry.topic,
        category: entry.category,
        tags: entry.tags,
        source: entry.source,
        status: entry.status,
        confidence: 0.0,
        created_at: now,
        updated_at: now,
        last_accessed_at: 0,
        access_count: 0,
        supersedes: None,
        superseded_by: None,
        correction_count: 0,
        embedding_dim: 0,
    }

    // Step 3: Serialize and write to ENTRIES
    let config = bincode::config::standard()
    let bytes = bincode::serde::encode_to_vec(&record, config)?
    {
        let mut table = txn.open_table(ENTRIES)?
        table.insert(id, bytes.as_slice())?
    }

    // Step 4: Write TOPIC_INDEX
    {
        let mut table = txn.open_table(TOPIC_INDEX)?
        table.insert((record.topic.as_str(), id), ())?
    }

    // Step 5: Write CATEGORY_INDEX
    {
        let mut table = txn.open_table(CATEGORY_INDEX)?
        table.insert((record.category.as_str(), id), ())?
    }

    // Step 6: Write TAG_INDEX (multimap)
    {
        let mut table = txn.open_multimap_table(TAG_INDEX)?
        for tag in &record.tags:
            table.insert(tag.as_str(), id)?
    }

    // Step 7: Write TIME_INDEX
    {
        let mut table = txn.open_table(TIME_INDEX)?
        table.insert((record.created_at, id), ())?
    }

    // Step 8: Write STATUS_INDEX
    {
        let mut table = txn.open_table(STATUS_INDEX)?
        table.insert((record.status as u8, id), ())?
    }

    // Step 9: Increment status counter
    counter::increment_counter(&txn, status_counter_key(record.status), 1)?

    // Step 10: Commit
    txn.commit()?
    Ok(id)
```

### Store::update(&self, entry: EntryRecord) -> Result<()>

```
fn update(&self, entry: EntryRecord) -> Result<()>:
    let txn = self.db.begin_write()?

    // Step 1: Read old record
    let old = {
        let table = txn.open_table(ENTRIES)?
        match table.get(entry.id)?:
            Some(guard) -> {
                let bytes = guard.value()
                let config = bincode::config::standard()
                let (record, _) = bincode::serde::decode_from_slice::<EntryRecord, _>(bytes, config)?
                record
            }
            None -> return Err(StoreError::EntryNotFound(entry.id))
    }

    // Step 2: Diff and update TOPIC_INDEX
    if old.topic != entry.topic:
        let mut table = txn.open_table(TOPIC_INDEX)?
        table.remove((old.topic.as_str(), entry.id))?
        table.insert((entry.topic.as_str(), entry.id), ())?

    // Step 3: Diff and update CATEGORY_INDEX
    if old.category != entry.category:
        let mut table = txn.open_table(CATEGORY_INDEX)?
        table.remove((old.category.as_str(), entry.id))?
        table.insert((entry.category.as_str(), entry.id), ())?

    // Step 4: Diff and update TAG_INDEX
    if old.tags != entry.tags:
        let mut table = txn.open_multimap_table(TAG_INDEX)?
        // Remove old tags not in new set
        let old_set: HashSet<&str> = old.tags.iter().map(|s| s.as_str()).collect()
        let new_set: HashSet<&str> = entry.tags.iter().map(|s| s.as_str()).collect()
        for removed_tag in old_set.difference(&new_set):
            table.remove(removed_tag, entry.id)?
        for added_tag in new_set.difference(&old_set):
            table.insert(added_tag, entry.id)?

    // Step 5: Diff and update TIME_INDEX (if created_at changed -- unusual but handle it)
    if old.created_at != entry.created_at:
        let mut table = txn.open_table(TIME_INDEX)?
        table.remove((old.created_at, entry.id))?
        table.insert((entry.created_at, entry.id), ())?

    // Step 6: Diff and update STATUS_INDEX + counters
    if old.status != entry.status:
        let mut table = txn.open_table(STATUS_INDEX)?
        table.remove((old.status as u8, entry.id))?
        table.insert((entry.status as u8, entry.id), ())?
        counter::decrement_counter(&txn, status_counter_key(old.status), 1)?
        counter::increment_counter(&txn, status_counter_key(entry.status), 1)?

    // Step 7: Write updated record to ENTRIES
    let mut updated = entry
    updated.updated_at = current_unix_timestamp_secs()
    let config = bincode::config::standard()
    let bytes = bincode::serde::encode_to_vec(&updated, config)?
    {
        let mut table = txn.open_table(ENTRIES)?
        table.insert(updated.id, bytes.as_slice())?
    }

    txn.commit()?
    Ok(())
```

### Store::update_status(&self, entry_id: u64, new_status: Status) -> Result<()>

```
fn update_status(&self, entry_id: u64, new_status: Status) -> Result<()>:
    let txn = self.db.begin_write()?

    // Step 1: Read existing record
    let mut record = {
        let table = txn.open_table(ENTRIES)?
        match table.get(entry_id)?:
            Some(guard) -> deserialize(guard.value())
            None -> return Err(StoreError::EntryNotFound(entry_id))
    }

    let old_status = record.status

    // Step 2: No-op if same status
    if old_status == new_status:
        return Ok(())

    // Step 3: Migrate STATUS_INDEX
    {
        let mut table = txn.open_table(STATUS_INDEX)?
        table.remove((old_status as u8, entry_id))?
        table.insert((new_status as u8, entry_id), ())?
    }

    // Step 4: Update record
    record.status = new_status
    record.updated_at = current_unix_timestamp_secs()
    let config = bincode::config::standard()
    let bytes = bincode::serde::encode_to_vec(&record, config)?
    {
        let mut table = txn.open_table(ENTRIES)?
        table.insert(entry_id, bytes.as_slice())?
    }

    // Step 5: Adjust counters
    counter::decrement_counter(&txn, status_counter_key(old_status), 1)?
    counter::increment_counter(&txn, status_counter_key(new_status), 1)?

    txn.commit()?
    Ok(())
```

### Store::delete(&self, entry_id: u64) -> Result<()>

```
fn delete(&self, entry_id: u64) -> Result<()>:
    let txn = self.db.begin_write()?

    // Step 1: Read existing record (need data for index cleanup)
    let record = {
        let table = txn.open_table(ENTRIES)?
        match table.get(entry_id)?:
            Some(guard) -> deserialize(guard.value())
            None -> return Err(StoreError::EntryNotFound(entry_id))
    }

    // Step 2: Remove from ENTRIES
    {
        let mut table = txn.open_table(ENTRIES)?
        table.remove(entry_id)?
    }

    // Step 3: Remove from TOPIC_INDEX
    {
        let mut table = txn.open_table(TOPIC_INDEX)?
        table.remove((record.topic.as_str(), entry_id))?
    }

    // Step 4: Remove from CATEGORY_INDEX
    {
        let mut table = txn.open_table(CATEGORY_INDEX)?
        table.remove((record.category.as_str(), entry_id))?
    }

    // Step 5: Remove from TAG_INDEX
    {
        let mut table = txn.open_multimap_table(TAG_INDEX)?
        for tag in &record.tags:
            table.remove(tag.as_str(), entry_id)?
    }

    // Step 6: Remove from TIME_INDEX
    {
        let mut table = txn.open_table(TIME_INDEX)?
        table.remove((record.created_at, entry_id))?
    }

    // Step 7: Remove from STATUS_INDEX
    {
        let mut table = txn.open_table(STATUS_INDEX)?
        table.remove((record.status as u8, entry_id))?
    }

    // Step 8: Remove from VECTOR_MAP (if present -- don't error if absent)
    {
        let mut table = txn.open_table(VECTOR_MAP)?
        table.remove(entry_id)?   // returns Option, ignore if None
    }

    // Step 9: Decrement status counter
    counter::decrement_counter(&txn, status_counter_key(record.status), 1)?

    txn.commit()?
    Ok(())
```

### Store::put_vector_mapping(&self, entry_id: u64, hnsw_data_id: u64) -> Result<()>

```
fn put_vector_mapping(&self, entry_id: u64, hnsw_data_id: u64) -> Result<()>:
    let txn = self.db.begin_write()?
    {
        let mut table = txn.open_table(VECTOR_MAP)?
        table.insert(entry_id, hnsw_data_id)?
    }
    txn.commit()?
    Ok(())
```

### Helper: current_unix_timestamp_secs() -> u64

```
fn current_unix_timestamp_secs() -> u64:
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
```

### Helper: deserialize(bytes) -> Result<EntryRecord>

```
fn deserialize_entry(bytes: &[u8]) -> Result<EntryRecord>:
    let config = bincode::config::standard()
    let (record, _) = bincode::serde::decode_from_slice::<EntryRecord, _>(bytes, config)?
    Ok(record)
```

### Helper: serialize(record) -> Result<Vec<u8>>

```
fn serialize_entry(record: &EntryRecord) -> Result<Vec<u8>>:
    let config = bincode::config::standard()
    let bytes = bincode::serde::encode_to_vec(record, config)?
    Ok(bytes)
```

## Error Handling

- All operations use `?` for error propagation. If any step fails, the transaction is dropped (not committed) and all changes roll back automatically.
- EntryNotFound for update/delete/update_status when entry_id doesn't exist.
- Serialization/Deserialization errors for bincode failures.

## Key Test Scenarios

- AC-04: Insert, verify all 6 index tables populated
- AC-05: 100 sequential inserts, monotonic IDs
- AC-12: Status change, verify STATUS_INDEX migration + counter adjustment
- AC-18: Update with topic/category/tag changes, verify stale index entries removed
- R1: Insert then verify every index table
- R2: Update every indexed field, verify old removed + new inserted
- R6: Drop transaction before commit, verify no changes
- R8: Status transitions -- Active->Deprecated, Proposed->Active, Deprecated->Active
