# Pseudocode: Store::insert_in_txn (unimatrix-store/src/write.rs)

## Method Signature

```
impl Store {
    /// Insert an entry within an externally-managed write transaction.
    /// Caller is responsible for committing the transaction.
    /// Performs all index writes: ENTRIES, TOPIC_INDEX, CATEGORY_INDEX,
    /// TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP, OUTCOME_INDEX, COUNTERS.
    pub(crate) fn insert_in_txn(
        &self,
        txn: &WriteTransaction,
        entry: NewEntry,
        embedding_dim: u16,
        data_id: u64,
        now: u64,
    ) -> Result<EntryRecord, StoreError>
}
```

## Implementation

```
fn insert_in_txn(&self, txn: &WriteTransaction, entry: NewEntry,
    embedding_dim: u16, data_id: u64, now: u64) -> Result<EntryRecord, StoreError>:

    // 1. Generate entry ID
    let id = next_entry_id(txn)?

    // 2. Compute content hash
    let content_hash = compute_content_hash(&entry.title, &entry.content)

    // 3. Build EntryRecord
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
        embedding_dim,
        created_by: entry.created_by.clone(),
        modified_by: entry.created_by,
        content_hash,
        previous_hash: String::new(),
        version: 1,
        feature_cycle: entry.feature_cycle,
        trust_source: entry.trust_source,
        helpful_count: 0,
        unhelpful_count: 0,
    }

    // 4. Serialize and write ENTRIES
    let bytes = serialize_entry(&record)?
    let mut table = txn.open_table(ENTRIES)?
    table.insert(id, bytes.as_slice())?

    // 5. Write TOPIC_INDEX
    let mut table = txn.open_table(TOPIC_INDEX)?
    table.insert((record.topic.as_str(), id), ())?

    // 6. Write CATEGORY_INDEX
    let mut table = txn.open_table(CATEGORY_INDEX)?
    table.insert((record.category.as_str(), id), ())?

    // 7. Write TAG_INDEX (multimap)
    let mut table = txn.open_multimap_table(TAG_INDEX)?
    for tag in &record.tags:
        table.insert(tag.as_str(), id)?

    // 8. Write TIME_INDEX
    let mut table = txn.open_table(TIME_INDEX)?
    table.insert((record.created_at, id), ())?

    // 9. Write STATUS_INDEX
    let mut table = txn.open_table(STATUS_INDEX)?
    table.insert((record.status as u8, id), ())?

    // 10. Write VECTOR_MAP
    let mut table = txn.open_table(VECTOR_MAP)?
    table.insert(id, data_id)?

    // 11. Write OUTCOME_INDEX (if outcome with non-empty feature_cycle)
    if record.category == "outcome" && !record.feature_cycle.is_empty():
        let mut table = txn.open_table(OUTCOME_INDEX)?
        table.insert((record.feature_cycle.as_str(), id), ())?

    // 12. Increment status counter
    increment_counter(txn, status_counter_key(record.status), 1)?

    // 13. Write FEATURE_ENTRIES index (if feature_cycle is non-empty)
    if !record.feature_cycle.is_empty():
        let mut table = txn.open_table(FEATURE_ENTRIES)?
        table.insert((record.feature_cycle.as_str(), id), ())?

    Ok(record)
```

## Notes

- This is an extraction of the write logic from `UnimatrixServer::insert_with_audit` in server.rs (lines 209-336).
- The method does NOT commit the transaction -- the caller does.
- It takes `embedding_dim` and `data_id` as parameters because these are allocated before the transaction in the caller.
- The existing `Store::insert()` method is preserved unchanged.
- After implementing `insert_in_txn`, `insert_with_audit` in server.rs can optionally be refactored to call it, but that refactoring is not required for vnc-006 scope if the StoreService directly uses `insert_in_txn`.
- `pub(crate)` visibility ensures WriteTransaction details don't leak outside the crate.
- Note: check if FEATURE_ENTRIES index is written by the current insert_with_audit. If not, match existing behavior exactly.
