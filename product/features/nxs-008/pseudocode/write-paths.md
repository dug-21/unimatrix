# Component: write-paths (Wave 1)

## File: `crates/unimatrix-store/src/write.rs`

**Action**: REWRITE
**Risk**: HIGH (RISK-02 CRITICAL, RISK-04 CRITICAL)
**ADR**: ADR-004 (named params), ADR-006 (entry_tags CASCADE)

## Purpose

Rewrite insert/update/delete/update_status to use 24-column SQL with `named_params!{}`. Remove index table writes. Remove bincode serialize/deserialize from runtime paths. Use `crate::counters::*` for counter operations.

## write.rs: Remove Private Counter Functions

Delete `read_counter`, `set_counter`, `increment_counter`, `decrement_counter` (lines 22-54). Replace all calls with `crate::counters::*`.

## write.rs: Keep `current_unix_timestamp_secs`

Stays as-is (utility function).

## Store::insert Rewrite

```rust
pub fn insert(&self, entry: NewEntry) -> Result<u64> {
    let now = current_unix_timestamp_secs();
    let conn = self.lock_conn();
    conn.execute_batch("BEGIN IMMEDIATE").map_err(StoreError::Sqlite)?;

    let result = (|| -> Result<u64> {
        // Step 1: Generate ID via counters module
        let id = crate::counters::next_entry_id(&conn)?;

        // Step 2: Compute content hash
        let content_hash = crate::hash::compute_content_hash(&entry.title, &entry.content);

        // Step 3: INSERT into entries with named params (ADR-004)
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
                ":title": &entry.title,
                ":content": &entry.content,
                ":topic": &entry.topic,
                ":category": &entry.category,
                ":source": &entry.source,
                ":status": entry.status as u8 as i64,
                ":confidence": 0.0_f64,
                ":created_at": now as i64,
                ":updated_at": now as i64,
                ":last_accessed_at": 0_i64,
                ":access_count": 0_i64,
                ":supersedes": Option::<i64>::None,
                ":superseded_by": Option::<i64>::None,
                ":correction_count": 0_i64,
                ":embedding_dim": 0_i64,
                ":created_by": &entry.created_by,
                ":modified_by": "",
                ":content_hash": &content_hash,
                ":previous_hash": "",
                ":version": 1_i64,
                ":feature_cycle": &entry.feature_cycle,
                ":trust_source": &entry.trust_source,
                ":helpful_count": 0_i64,
                ":unhelpful_count": 0_i64,
            },
        ).map_err(StoreError::Sqlite)?;

        // Step 4: Insert tags into entry_tags
        for tag in &entry.tags {
            conn.execute(
                "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                rusqlite::params![id as i64, tag],
            ).map_err(StoreError::Sqlite)?;
        }

        // Step 5: Update status counter
        crate::counters::increment_counter(&conn, status_counter_key(entry.status), 1)?;

        // NO MORE: topic_index, category_index, tag_index, time_index, status_index

        Ok(id)
    })();

    match result {
        Ok(id) => { conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?; Ok(id) }
        Err(e) => { let _ = conn.execute_batch("ROLLBACK"); Err(e) }
    }
}
```

## Store::update Rewrite

```rust
pub fn update(&self, entry: EntryRecord) -> Result<()> {
    let entry_id = entry.id;
    let conn = self.lock_conn();
    conn.execute_batch("BEGIN IMMEDIATE").map_err(StoreError::Sqlite)?;

    let result = (|| -> Result<()> {
        // Read old status for counter adjustment (no blob deserialize!)
        let old_status: i64 = conn.query_row(
            "SELECT status FROM entries WHERE id = ?1",
            rusqlite::params![entry_id as i64],
            |row| row.get(0),
        ).optional().map_err(StoreError::Sqlite)?
         .ok_or(StoreError::EntryNotFound(entry_id))?;

        // UPDATE all 24 columns with named params
        conn.execute(
            "UPDATE entries SET
                title = :title, content = :content, topic = :topic,
                category = :category, source = :source, status = :status,
                confidence = :confidence, created_at = :created_at,
                updated_at = :updated_at, last_accessed_at = :last_accessed_at,
                access_count = :access_count, supersedes = :supersedes,
                superseded_by = :superseded_by, correction_count = :correction_count,
                embedding_dim = :embedding_dim, created_by = :created_by,
                modified_by = :modified_by, content_hash = :content_hash,
                previous_hash = :previous_hash, version = :version,
                feature_cycle = :feature_cycle, trust_source = :trust_source,
                helpful_count = :helpful_count, unhelpful_count = :unhelpful_count
             WHERE id = :id",
            rusqlite::named_params! {
                ":id": entry.id as i64,
                ":title": &entry.title,
                // ... all 24 fields ...
                ":unhelpful_count": entry.unhelpful_count as i64,
            },
        ).map_err(StoreError::Sqlite)?;

        // Replace tags: delete all, re-insert (ADR-006)
        conn.execute(
            "DELETE FROM entry_tags WHERE entry_id = ?1",
            rusqlite::params![entry_id as i64],
        ).map_err(StoreError::Sqlite)?;

        for tag in &entry.tags {
            conn.execute(
                "INSERT INTO entry_tags (entry_id, tag) VALUES (?1, ?2)",
                rusqlite::params![entry_id as i64, tag],
            ).map_err(StoreError::Sqlite)?;
        }

        // Status counter adjustment
        let new_status = entry.status as u8 as i64;
        if new_status != old_status {
            let old = Status::try_from(old_status as u8).unwrap_or(Status::Active);
            crate::counters::decrement_counter(&conn, status_counter_key(old), 1)?;
            crate::counters::increment_counter(&conn, status_counter_key(entry.status), 1)?;
        }

        // NO MORE: index table diffing

        Ok(())
    })();

    match result {
        Ok(()) => { conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?; Ok(()) }
        Err(e) => { let _ = conn.execute_batch("ROLLBACK"); Err(e) }
    }
}
```

## Store::update_status Rewrite

```rust
pub fn update_status(&self, entry_id: u64, new_status: Status) -> Result<()> {
    let now = current_unix_timestamp_secs();
    let conn = self.lock_conn();
    conn.execute_batch("BEGIN IMMEDIATE").map_err(StoreError::Sqlite)?;

    let result = (|| -> Result<()> {
        // Read old status (single column, no blob deserialize)
        let old_status_val: i64 = conn.query_row(
            "SELECT status FROM entries WHERE id = ?1",
            rusqlite::params![entry_id as i64],
            |row| row.get(0),
        ).optional().map_err(StoreError::Sqlite)?
         .ok_or(StoreError::EntryNotFound(entry_id))?;

        // Update status and updated_at directly
        conn.execute(
            "UPDATE entries SET status = ?1, updated_at = ?2 WHERE id = ?3",
            rusqlite::params![new_status as u8 as i64, now as i64, entry_id as i64],
        ).map_err(StoreError::Sqlite)?;

        // Counter adjustment
        let old_status = Status::try_from(old_status_val as u8).unwrap_or(Status::Active);
        crate::counters::decrement_counter(&conn, status_counter_key(old_status), 1)?;
        crate::counters::increment_counter(&conn, status_counter_key(new_status), 1)?;

        Ok(())
    })();

    match result {
        Ok(()) => { conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?; Ok(()) }
        Err(e) => { let _ = conn.execute_batch("ROLLBACK"); Err(e) }
    }
}
```

## Store::delete Rewrite

```rust
pub fn delete(&self, entry_id: u64) -> Result<()> {
    let conn = self.lock_conn();
    conn.execute_batch("BEGIN IMMEDIATE").map_err(StoreError::Sqlite)?;

    let result = (|| -> Result<()> {
        // Read status for counter adjustment (single column)
        let old_status_val: i64 = conn.query_row(
            "SELECT status FROM entries WHERE id = ?1",
            rusqlite::params![entry_id as i64],
            |row| row.get(0),
        ).optional().map_err(StoreError::Sqlite)?
         .ok_or(StoreError::EntryNotFound(entry_id))?;

        // Delete from entries (CASCADE deletes entry_tags automatically)
        conn.execute(
            "DELETE FROM entries WHERE id = ?1",
            rusqlite::params![entry_id as i64],
        ).map_err(StoreError::Sqlite)?;

        // Delete from vector_map (no FK, manual)
        conn.execute(
            "DELETE FROM vector_map WHERE entry_id = ?1",
            rusqlite::params![entry_id as i64],
        ).map_err(StoreError::Sqlite)?;

        // Decrement status counter
        let old_status = Status::try_from(old_status_val as u8).unwrap_or(Status::Active);
        crate::counters::decrement_counter(&conn, status_counter_key(old_status), 1)?;

        // NO MORE: topic_index, category_index, tag_index, time_index, status_index deletes

        Ok(())
    })();

    match result {
        Ok(()) => { conn.execute_batch("COMMIT").map_err(StoreError::Sqlite)?; Ok(()) }
        Err(e) => { let _ = conn.execute_batch("ROLLBACK"); Err(e) }
    }
}
```

## write_ext.rs Changes

### record_usage_with_confidence Rewrite

Replace blob read/deserialize/modify/serialize/update with direct SQL column updates:

```rust
// Instead of: SELECT data -> deserialize -> modify -> serialize -> UPDATE data
// Use: UPDATE entries SET last_accessed_at = ?, access_count = access_count + 1, ... WHERE id = ?

for &id in all_ids {
    // Build dynamic SET clause based on which sets contain this id
    let mut sets: Vec<String> = vec![format!("last_accessed_at = {}", now)];
    if access_set.contains(&id) { sets.push("access_count = access_count + 1".to_string()); }
    if helpful_set.contains(&id) { sets.push("helpful_count = helpful_count + 1".to_string()); }
    if unhelpful_set.contains(&id) { sets.push("unhelpful_count = unhelpful_count + 1".to_string()); }
    if dec_helpful_set.contains(&id) { sets.push("helpful_count = MAX(0, helpful_count - 1)".to_string()); }
    if dec_unhelpful_set.contains(&id) { sets.push("unhelpful_count = MAX(0, unhelpful_count - 1)".to_string()); }

    let sql = format!("UPDATE entries SET {} WHERE id = ?1", sets.join(", "));
    conn.execute(&sql, rusqlite::params![id as i64])?;

    // If confidence_fn provided, read back fields needed for computation
    if let Some(f) = &confidence_fn {
        // Read full record via entry_from_row + load_tags
        // Compute new confidence, UPDATE confidence column only
    }
}
```

### update_confidence Rewrite

```rust
// Direct column update - no blob read/write
conn.execute(
    "UPDATE entries SET confidence = ?1 WHERE id = ?2",
    rusqlite::params![confidence, entry_id as i64],
)?;
```

### record_co_access_pairs Rewrite

```rust
// Replace blob read/deserialize/modify/serialize with SQL columns
let existing: Option<(i64, i64)> = conn.query_row(
    "SELECT count, last_updated FROM co_access WHERE entry_id_a = ?1 AND entry_id_b = ?2",
    rusqlite::params![min_id as i64, max_id as i64],
    |row| Ok((row.get(0)?, row.get(1)?)),
).optional()?;

match existing {
    Some((count, _)) => {
        conn.execute(
            "UPDATE co_access SET count = ?1, last_updated = ?2 WHERE entry_id_a = ?3 AND entry_id_b = ?4",
            rusqlite::params![count + 1, now as i64, min_id as i64, max_id as i64],
        )?;
    }
    None => {
        conn.execute(
            "INSERT INTO co_access (entry_id_a, entry_id_b, count, last_updated) VALUES (?1, ?2, 1, ?3)",
            rusqlite::params![min_id as i64, max_id as i64, now as i64],
        )?;
    }
}
```

### cleanup_stale_co_access Rewrite

```rust
// Direct SQL filter instead of scan + deserialize
let deleted = conn.execute(
    "DELETE FROM co_access WHERE last_updated < ?1",
    rusqlite::params![staleness_cutoff as i64],
)?;
Ok(deleted as u64)
```
