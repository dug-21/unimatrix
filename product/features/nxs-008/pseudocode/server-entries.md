# Component: server-entries (Wave 1)

## Files Modified

- `crates/unimatrix-server/src/services/store_ops.rs` - Direct SQL insert
- `crates/unimatrix-server/src/services/store_correct.rs` - Direct SQL deprecate + insert
- `crates/unimatrix-server/src/services/status.rs` - Query entries table directly
- `crates/unimatrix-server/src/infra/contradiction.rs` - Query entries table directly

**Risk**: HIGH (RISK-02, RISK-06)
**ADR**: ADR-004, ADR-008

## Purpose

Rewrite server write paths to use direct SQL with named params. Remove `open_table`/`open_multimap_table` dispatch calls. Use `&*txn.guard` for direct connection access. Remove index table writes.

## store_ops.rs: Insert Rewrite

### Import Changes

```rust
// REMOVE these imports:
// use unimatrix_store::{CATEGORY_INDEX, ENTRIES, ...};
// use unimatrix_store::{serialize_entry, ...};

// KEEP/ADD these imports:
use unimatrix_store::{compute_content_hash, status_counter_key};
```

### Atomic Insert Transaction

```rust
let (entry_id, record) = tokio::task::spawn_blocking(move || -> Result<(u64, EntryRecord), ServerError> {
    let txn = store.begin_write()
        .map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
    let conn = &*txn.guard;

    // 1. Allocate entry ID via counters module
    let id = unimatrix_store::counters::next_entry_id(conn)
        .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

    let content_hash = compute_content_hash(&entry.title, &entry.content);
    let now = current_unix_secs();

    let record = EntryRecord { id, /* ... all 24 fields ... */ };

    // 2. INSERT into entries with named params (ADR-004)
    conn.execute(
        "INSERT INTO entries (id, title, content, topic, category, source,
            status, confidence, created_at, updated_at, last_accessed_at,
            access_count, supersedes, superseded_by, correction_count,
            embedding_dim, created_by, modified_by, content_hash,
            previous_hash, version, feature_cycle, trust_source,
            helpful_count, unhelpful_count)
         VALUES (:id, :title, :content, :topic, :category, :source,
            :status, :confidence, :created_at, :updated_at, :last_accessed_at,
            :access_count, :supersedes, :superseded_by, :correction_count,
            :embedding_dim, :created_by, :modified_by, :content_hash,
            :previous_hash, :version, :feature_cycle, :trust_source,
            :helpful_count, :unhelpful_count)",
        rusqlite::named_params! {
            ":id": id as i64,
            // ... all 24 named params matching write.rs ...
        },
    ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

    // 3. Insert tags into entry_tags
    for tag in &record.tags {
        conn.execute(
            "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
            rusqlite::params![id as i64, tag],
        ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
    }

    // 4. Insert vector mapping (still simple KV)
    conn.execute(
        "INSERT OR REPLACE INTO vector_map (entry_id, hnsw_data_id) VALUES (?1, ?2)",
        rusqlite::params![id as i64, data_id as i64],
    ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;

    // 5. Outcome index (if applicable)
    if record.category == "outcome" && !record.feature_cycle.is_empty() {
        conn.execute(
            "INSERT OR IGNORE INTO outcome_index (feature_cycle, entry_id) VALUES (?1, ?2)",
            rusqlite::params![&record.feature_cycle, id as i64],
        ).map_err(|e| ServerError::Core(CoreError::Store(StoreError::Sqlite(e))))?;
    }

    // 6. Status counter
    unimatrix_store::counters::increment_counter(conn, status_counter_key(record.status), 1)
        .map_err(|e| ServerError::Core(CoreError::Store(e)))?;

    // 7. Audit
    audit_log.write_in_txn(&txn, audit_event_with_target)?;

    // 8. Commit
    txn.commit().map_err(|e| ServerError::Core(CoreError::Store(e.into())))?;
    Ok((id, record))
}).await??;
```

## store_correct.rs: Correct Rewrite

### Import Changes

Remove all `ENTRIES`, `TOPIC_INDEX`, etc. imports. Remove `serialize_entry`, `deserialize_entry`.

### Atomic Correct Transaction

```rust
let (deprecated_original, new_correction) = tokio::task::spawn_blocking(
    move || -> Result<(EntryRecord, EntryRecord), ServerError> {
        let txn = store.begin_write()?;
        let conn = &*txn.guard;

        // 1. Read original entry via entry_from_row pattern
        let original_entry = conn.query_row(
            &format!("SELECT {} FROM entries WHERE id = ?1", ENTRY_COLUMNS),
            rusqlite::params![original_id as i64],
            |row| {
                // Inline entry_from_row or use a shared helper
                // Return EntryRecord with tags=vec![]
            },
        ).optional()?.ok_or(StoreError::EntryNotFound(original_id))?;

        // Load tags for original
        let tag_map = load_tags_for_entries(conn, &[original_id])?;
        let mut original = original_entry;
        if let Some(tags) = tag_map.get(&original_id) {
            original.tags = tags.clone();
        }

        // 2. Validate not deprecated/quarantined
        if original.status == Status::Deprecated { return Err(InvalidInput...) }
        if original.status == Status::Quarantined { return Err(InvalidInput...) }

        // 3. Generate new ID
        let new_id = counters::next_entry_id(conn)?;

        // 4. Deprecate original (direct column UPDATE)
        let old_status = original.status;
        conn.execute(
            "UPDATE entries SET status = ?1, superseded_by = ?2,
             correction_count = correction_count + 1, updated_at = ?3
             WHERE id = ?4",
            rusqlite::params![
                Status::Deprecated as u8 as i64,
                new_id as i64,
                now as i64,
                original_id as i64
            ],
        )?;

        // Update counters
        counters::decrement_counter(conn, status_counter_key(old_status), 1)?;
        counters::increment_counter(conn, status_counter_key(Status::Deprecated), 1)?;

        // 5. Build correction record + INSERT (same as store_ops insert)
        // ... 24-column named_params INSERT ...

        // 6. Insert tags for correction
        // 7. Insert vector_map for correction
        // 8. Increment status counter for correction
        // 9. Write audit event
        // 10. Commit

        txn.commit()?;
        Ok((original, correction))
    },
).await??;
```

## status.rs Changes

Replace index-table scans with direct column queries:

```rust
// BEFORE: scan status_index table
// AFTER:
let count: i64 = conn.query_row(
    "SELECT COUNT(*) FROM entries WHERE status = ?1",
    rusqlite::params![Status::Active as u8 as i64],
    |row| row.get(0),
)?;
```

## contradiction.rs Changes

Replace STATUS_INDEX scan + blob deserialize with:

```rust
// Get all active entries directly
let mut stmt = conn.prepare(
    &format!("SELECT {} FROM entries WHERE status = ?1", ENTRY_COLUMNS)
)?;
let mut entries: Vec<EntryRecord> = stmt.query_map(
    rusqlite::params![Status::Active as u8 as i64],
    entry_from_row,
)?.collect::<rusqlite::Result<Vec<_>>>()?;

let ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
let tag_map = load_tags_for_entries(&conn, &ids)?;
apply_tags(&mut entries, &tag_map);
```

## Shared Helper Access

`entry_from_row`, `load_tags_for_entries`, and `ENTRY_COLUMNS` need to be accessible from the server crate. Options:

1. **Make public in store crate**: `pub fn entry_from_row(...)` in `read.rs`, re-export from `lib.rs`
2. **Move to schema.rs**: Since they're data-construction helpers

Decision: Make `entry_from_row` and `load_tags_for_entries` `pub` in store crate, re-exported from `lib.rs`. Also export `ENTRY_COLUMNS`.

## Import Pattern for Server Code

```rust
use unimatrix_store::{
    Store, EntryRecord, NewEntry, Status, StoreError,
    compute_content_hash, status_counter_key,
    entry_from_row, load_tags_for_entries, apply_tags, ENTRY_COLUMNS,
    counters,
};
```
