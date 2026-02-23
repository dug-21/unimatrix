# Pseudocode: C6 Server Combined Transaction Methods

## File: `crates/unimatrix-server/src/server.rs`

### Change: Fix `insert_with_audit` (GH #14)

The current implementation calls `self.vector_store.insert(entry_id, embedding)` after
the transaction commits. This writes VECTOR_MAP in a separate transaction. The fix
moves the VECTOR_MAP write into the combined transaction.

```
pub(crate) async fn insert_with_audit(
    &self,
    entry: NewEntry,
    embedding: Vec<f32>,
    audit_event: AuditEvent,
) -> Result<(u64, EntryRecord), ServerError>:
    let store = Arc::clone(&self.store)
    let audit_log = Arc::clone(&self.audit)

    // NEW: Allocate data_id BEFORE the transaction
    let data_id = self.vector_index.allocate_data_id()

    // Step 1: Combined write transaction
    let (entry_id, record) = spawn_blocking(move || {
        let txn = store.begin_write()?

        // Generate entry ID
        let id = next_entry_id(&txn)?

        // Compute content hash, build EntryRecord, etc. (unchanged)
        // ...

        // Write ENTRIES (unchanged)
        // Write TOPIC_INDEX (unchanged)
        // Write CATEGORY_INDEX (unchanged)
        // Write TAG_INDEX (unchanged)
        // Write TIME_INDEX (unchanged)
        // Write STATUS_INDEX (unchanged)
        // Increment status counter (unchanged)

        // NEW: Write VECTOR_MAP in the same transaction
        {
            let mut table = txn.open_table(VECTOR_MAP)?
            table.insert(id, data_id)?
        }

        // Write audit event (unchanged)
        audit_log.write_in_txn(&txn, audit_event_with_target)?

        txn.commit()?
        Ok((id, record))
    }).await??

    // Step 2: HNSW insert only (VECTOR_MAP already committed)
    // CHANGED: use insert_hnsw_only instead of vector_store.insert
    self.vector_index.insert_hnsw_only(entry_id, data_id, &embedding)?

    Ok((entry_id, record))
```

Key changes:
1. `self.vector_index.allocate_data_id()` before spawn_blocking
2. `VECTOR_MAP.insert(id, data_id)` inside the transaction
3. `self.vector_index.insert_hnsw_only(entry_id, data_id, &embedding)` after commit
4. Removed `self.vector_store.insert(entry_id, embedding)` call

### New Method: `correct_with_audit`

```
pub(crate) async fn correct_with_audit(
    &self,
    original_id: u64,
    correction_entry: NewEntry,
    embedding: Vec<f32>,
    reason: Option<String>,
    audit_event: AuditEvent,
) -> Result<(EntryRecord, EntryRecord), ServerError>:
    let store = Arc::clone(&self.store)
    let audit_log = Arc::clone(&self.audit)
    let data_id = self.vector_index.allocate_data_id()

    let (deprecated_original, new_correction) = spawn_blocking(move || {
        let txn = store.begin_write()?

        // 1. Read and validate original entry
        let original_bytes = {
            let table = txn.open_table(ENTRIES)?
            let guard = table.get(original_id)?
                .ok_or(ServerError::Core(CoreError::Store(StoreError::EntryNotFound(original_id))))?
            guard.value().to_vec()
        }
        let mut original = deserialize_entry(&original_bytes)?

        // 2. Verify original is not already deprecated
        if original.status == Status::Deprecated:
            return Err(ServerError::InvalidInput {
                field: "original_id",
                reason: "cannot correct a deprecated entry"
            })

        // 3. Generate new entry ID
        let new_id = next_entry_id(&txn)?

        // 4. Deprecate original
        let old_status = original.status
        original.status = Status::Deprecated
        original.superseded_by = Some(new_id)
        original.correction_count += 1
        original.updated_at = now()

        // 5. Serialize and overwrite original in ENTRIES
        let original_bytes = serialize_entry(&original)?
        {
            let mut table = txn.open_table(ENTRIES)?
            table.insert(original_id, original_bytes.as_slice())?
        }

        // 6. Update STATUS_INDEX for original: remove old, insert new
        {
            let mut table = txn.open_table(STATUS_INDEX)?
            table.remove((old_status as u8, original_id))?
            table.insert((Status::Deprecated as u8, original_id), ())?
        }

        // 7. Update status counters for original
        decrement_counter(&txn, status_counter_key(old_status), 1)?
        increment_counter(&txn, status_counter_key(Status::Deprecated), 1)?

        // 8. Build correction EntryRecord
        let content_hash = compute_content_hash(&correction_entry.title, &correction_entry.content)
        let correction = EntryRecord {
            id: new_id,
            title: correction_entry.title,
            content: correction_entry.content,
            topic: correction_entry.topic,
            category: correction_entry.category,
            tags: correction_entry.tags,
            source: correction_entry.source,
            status: correction_entry.status,  // Active
            confidence: 0.0,
            created_at: now(),
            updated_at: now(),
            last_accessed_at: 0,
            access_count: 0,
            supersedes: Some(original_id),
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 0,
            created_by: correction_entry.created_by.clone(),
            modified_by: correction_entry.created_by,
            content_hash,
            previous_hash: String::new(),
            version: 1,
            feature_cycle: correction_entry.feature_cycle,
            trust_source: correction_entry.trust_source,
        }

        // 9. Write correction to ENTRIES
        let correction_bytes = serialize_entry(&correction)?
        {
            let mut table = txn.open_table(ENTRIES)?
            table.insert(new_id, correction_bytes.as_slice())?
        }

        // 10. Write all indexes for correction
        // TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX
        // (same as insert_with_audit pattern)

        // 11. Increment status counter for correction
        increment_counter(&txn, status_counter_key(correction.status), 1)?

        // 12. Write VECTOR_MAP for correction
        {
            let mut table = txn.open_table(VECTOR_MAP)?
            table.insert(new_id, data_id)?
        }

        // 13. Write audit event with both IDs
        let audit_with_ids = AuditEvent {
            target_ids: vec![original_id, new_id],
            ..audit_event
        }
        audit_log.write_in_txn(&txn, audit_with_ids)?

        // 14. Commit
        txn.commit()?

        Ok((original, correction))
    }).await??

    // HNSW insert for the correction (after commit)
    self.vector_index.insert_hnsw_only(
        new_correction.id, data_id, &embedding
    )?

    Ok((deprecated_original, new_correction))
```

### New Method: `deprecate_with_audit`

```
pub(crate) async fn deprecate_with_audit(
    &self,
    entry_id: u64,
    reason: Option<String>,
    audit_event: AuditEvent,
) -> Result<EntryRecord, ServerError>:
    let store = Arc::clone(&self.store)
    let audit_log = Arc::clone(&self.audit)

    let record = spawn_blocking(move || {
        let txn = store.begin_write()?

        // 1. Read existing entry
        let entry_bytes = {
            let table = txn.open_table(ENTRIES)?
            let guard = table.get(entry_id)?
                .ok_or(ServerError::Core(CoreError::Store(StoreError::EntryNotFound(entry_id))))?
            guard.value().to_vec()
        }
        let mut record = deserialize_entry(&entry_bytes)?

        // 2. Idempotency check: already deprecated -> return as-is, no audit
        if record.status == Status::Deprecated:
            // No state change, no audit event, no commit needed
            // Just drop the transaction
            return Ok(record)

        // 3. Update status
        let old_status = record.status
        record.status = Status::Deprecated
        record.updated_at = now()

        // 4. Serialize and overwrite in ENTRIES
        let bytes = serialize_entry(&record)?
        {
            let mut table = txn.open_table(ENTRIES)?
            table.insert(entry_id, bytes.as_slice())?
        }

        // 5. Update STATUS_INDEX
        {
            let mut table = txn.open_table(STATUS_INDEX)?
            table.remove((old_status as u8, entry_id))?
            table.insert((Status::Deprecated as u8, entry_id), ())?
        }

        // 6. Update status counters
        decrement_counter(&txn, status_counter_key(old_status), 1)?
        increment_counter(&txn, status_counter_key(Status::Deprecated), 1)?

        // 7. Write audit event with reason in detail
        let detail = match &reason {
            Some(r) => format!("deprecated entry #{entry_id}: {r}"),
            None => format!("deprecated entry #{entry_id}"),
        };
        let audit_with_detail = AuditEvent {
            target_ids: vec![entry_id],
            detail,
            ..audit_event
        };
        audit_log.write_in_txn(&txn, audit_with_detail)?

        // 8. Commit
        txn.commit()?
        Ok(record)
    }).await??

    Ok(record)
```

### Helper: `decrement_counter`

The store module has `increment_counter` but no `decrement_counter`.
We need a helper that decrements safely (saturating at 0).

```
fn decrement_counter(txn: &WriteTransaction, key: &str, amount: u64) -> Result<(), StoreError>:
    let mut table = txn.open_table(COUNTERS)?
    let current = match table.get(key)?:
        Some(guard) => guard.value()
        None => 0
    table.insert(key, current.saturating_sub(amount))?
    Ok(())
```

This can be a local function in server.rs since it is only used there.
