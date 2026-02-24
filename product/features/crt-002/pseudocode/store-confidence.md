# Pseudocode: store-confidence (C2)

## File: `crates/unimatrix-store/src/write.rs`

### update_confidence(entry_id: u64, confidence: f32) -> Result<()>

```
function update_confidence(self, entry_id, confidence):
    txn = self.db.begin_write()

    // Read existing entry
    old_bytes = txn.open_table(ENTRIES).get(entry_id)
    if old_bytes is None:
        return Err(StoreError::NotFound(entry_id))

    record = deserialize_entry(old_bytes)
    record.confidence = confidence
    new_bytes = serialize_entry(record)

    // Write back to ENTRIES only -- no index tables touched
    txn.open_table(ENTRIES).insert(entry_id, new_bytes)

    txn.commit()
    return Ok(())
```

Key: This method ONLY touches the ENTRIES table. No TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, or VECTOR_MAP operations. This is the critical difference from Store::update() which performs full index diffs.

### record_usage_with_confidence(...)

Extend the existing `record_usage()` to accept an optional confidence function.

```
function record_usage_with_confidence(
    self,
    all_ids: &[u64],
    access_ids: &[u64],
    helpful_ids: &[u64],
    unhelpful_ids: &[u64],
    decrement_helpful_ids: &[u64],
    decrement_unhelpful_ids: &[u64],
    confidence_fn: Option<&dyn Fn(&EntryRecord, u64) -> f32>,
):
    if all_ids is empty:
        return Ok(())

    now = current_unix_timestamp_secs()
    txn = self.db.begin_write()

    // Build HashSets for O(1) lookup (same as existing record_usage)
    access_set = HashSet from access_ids
    helpful_set = HashSet from helpful_ids
    unhelpful_set = HashSet from unhelpful_ids
    dec_helpful_set = HashSet from decrement_helpful_ids
    dec_unhelpful_set = HashSet from decrement_unhelpful_ids

    for each id in all_ids:
        old_bytes = txn.open_table(ENTRIES).get(id)
        if old_bytes is None:
            continue  // Entry deleted between retrieval and recording

        record = deserialize_entry(old_bytes)

        // Update counters (existing logic, unchanged)
        record.last_accessed_at = now
        if id in access_set: record.access_count += 1
        if id in helpful_set: record.helpful_count += 1
        if id in unhelpful_set: record.unhelpful_count += 1
        if id in dec_helpful_set: record.helpful_count = saturating_sub(1)
        if id in dec_unhelpful_set: record.unhelpful_count = saturating_sub(1)

        // NEW: Compute and update confidence inline
        if confidence_fn is Some(f):
            record.confidence = f(&record, now)

        new_bytes = serialize_entry(record)
        txn.open_table(ENTRIES).insert(id, new_bytes)

    txn.commit()
    return Ok(())
```

### Preserve existing record_usage()

The existing `record_usage()` method is preserved for backward compatibility (used by `EntryStore::record_access` trait implementation). It delegates to the new method:

```
function record_usage(self, all_ids, access_ids, helpful_ids, unhelpful_ids, dec_helpful, dec_unhelpful):
    return self.record_usage_with_confidence(
        all_ids, access_ids, helpful_ids, unhelpful_ids,
        dec_helpful, dec_unhelpful,
        None  // no confidence computation
    )
```

## Dependencies

- `unimatrix_store::schema::{ENTRIES, EntryRecord, serialize_entry, deserialize_entry}`
- `unimatrix_store::error::StoreError`
- `std::collections::HashSet`
