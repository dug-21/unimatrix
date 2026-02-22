use std::collections::HashSet;

use redb::ReadableTable;

use crate::counter;
use crate::db::Store;
use crate::error::{Result, StoreError};
use crate::schema::{
    CATEGORY_INDEX, ENTRIES, EntryRecord, NewEntry, STATUS_INDEX, Status, TAG_INDEX, TIME_INDEX,
    TOPIC_INDEX, VECTOR_MAP, deserialize_entry, serialize_entry, status_counter_key,
};

/// Get the current unix timestamp in seconds.
fn current_unix_timestamp_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl Store {
    /// Insert a new entry. Returns the assigned entry_id.
    ///
    /// All index tables and counters are updated atomically within a single
    /// write transaction. If any step fails, the transaction is rolled back.
    pub fn insert(&self, entry: NewEntry) -> Result<u64> {
        let now = current_unix_timestamp_secs();
        let txn = self.db.begin_write()?;

        // Step 1: Generate ID
        let id = counter::next_entry_id(&txn)?;

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
        };

        // Step 3: Serialize and write to ENTRIES
        let bytes = serialize_entry(&record)?;
        {
            let mut table = txn.open_table(ENTRIES)?;
            table.insert(id, bytes.as_slice())?;
        }

        // Step 4: Write TOPIC_INDEX
        {
            let mut table = txn.open_table(TOPIC_INDEX)?;
            table.insert((record.topic.as_str(), id), ())?;
        }

        // Step 5: Write CATEGORY_INDEX
        {
            let mut table = txn.open_table(CATEGORY_INDEX)?;
            table.insert((record.category.as_str(), id), ())?;
        }

        // Step 6: Write TAG_INDEX (multimap)
        {
            let mut table = txn.open_multimap_table(TAG_INDEX)?;
            for tag in &record.tags {
                table.insert(tag.as_str(), id)?;
            }
        }

        // Step 7: Write TIME_INDEX
        {
            let mut table = txn.open_table(TIME_INDEX)?;
            table.insert((record.created_at, id), ())?;
        }

        // Step 8: Write STATUS_INDEX
        {
            let mut table = txn.open_table(STATUS_INDEX)?;
            table.insert((record.status as u8, id), ())?;
        }

        // Step 9: Increment status counter
        counter::increment_counter(&txn, status_counter_key(record.status), 1)?;

        // Step 10: Commit
        txn.commit()?;
        Ok(id)
    }

    /// Update an existing entry. Indexes are diffed and updated atomically.
    ///
    /// The caller provides the full updated `EntryRecord`. The engine reads
    /// the old record, identifies which indexed fields changed, and performs
    /// the minimal set of index removals and insertions.
    pub fn update(&self, entry: EntryRecord) -> Result<()> {
        let txn = self.db.begin_write()?;

        // Step 1: Read old record
        let old = {
            let table = txn.open_table(ENTRIES)?;
            match table.get(entry.id)? {
                Some(guard) => deserialize_entry(guard.value())?,
                None => return Err(StoreError::EntryNotFound(entry.id)),
            }
        };

        // Step 2: Diff and update TOPIC_INDEX
        if old.topic != entry.topic {
            let mut table = txn.open_table(TOPIC_INDEX)?;
            table.remove((old.topic.as_str(), entry.id))?;
            table.insert((entry.topic.as_str(), entry.id), ())?;
        }

        // Step 3: Diff and update CATEGORY_INDEX
        if old.category != entry.category {
            let mut table = txn.open_table(CATEGORY_INDEX)?;
            table.remove((old.category.as_str(), entry.id))?;
            table.insert((entry.category.as_str(), entry.id), ())?;
        }

        // Step 4: Diff and update TAG_INDEX
        if old.tags != entry.tags {
            let mut table = txn.open_multimap_table(TAG_INDEX)?;
            let old_set: HashSet<&str> = old.tags.iter().map(|s| s.as_str()).collect();
            let new_set: HashSet<&str> = entry.tags.iter().map(|s| s.as_str()).collect();

            for removed_tag in old_set.difference(&new_set) {
                table.remove(removed_tag, entry.id)?;
            }
            for added_tag in new_set.difference(&old_set) {
                table.insert(added_tag, entry.id)?;
            }
        }

        // Step 5: Diff and update TIME_INDEX
        if old.created_at != entry.created_at {
            let mut table = txn.open_table(TIME_INDEX)?;
            table.remove((old.created_at, entry.id))?;
            table.insert((entry.created_at, entry.id), ())?;
        }

        // Step 6: Diff and update STATUS_INDEX + counters
        if old.status != entry.status {
            let mut table = txn.open_table(STATUS_INDEX)?;
            table.remove((old.status as u8, entry.id))?;
            table.insert((entry.status as u8, entry.id), ())?;
            counter::decrement_counter(&txn, status_counter_key(old.status), 1)?;
            counter::increment_counter(&txn, status_counter_key(entry.status), 1)?;
        }

        // Step 7: Write updated record to ENTRIES with new updated_at
        let mut updated = entry;
        updated.updated_at = current_unix_timestamp_secs();
        let bytes = serialize_entry(&updated)?;
        {
            let mut table = txn.open_table(ENTRIES)?;
            table.insert(updated.id, bytes.as_slice())?;
        }

        txn.commit()?;
        Ok(())
    }

    /// Change the status of an entry. Migrates STATUS_INDEX atomically.
    ///
    /// This is a specialized update that only changes the status field,
    /// the STATUS_INDEX, and the status counters.
    pub fn update_status(&self, entry_id: u64, new_status: Status) -> Result<()> {
        let txn = self.db.begin_write()?;

        // Step 1: Read existing record
        let mut record = {
            let table = txn.open_table(ENTRIES)?;
            match table.get(entry_id)? {
                Some(guard) => deserialize_entry(guard.value())?,
                None => return Err(StoreError::EntryNotFound(entry_id)),
            }
        };

        let old_status = record.status;

        // Step 2: No-op if same status
        if old_status == new_status {
            return Ok(());
        }

        // Step 3: Migrate STATUS_INDEX
        {
            let mut table = txn.open_table(STATUS_INDEX)?;
            table.remove((old_status as u8, entry_id))?;
            table.insert((new_status as u8, entry_id), ())?;
        }

        // Step 4: Update record and write back
        record.status = new_status;
        record.updated_at = current_unix_timestamp_secs();
        let bytes = serialize_entry(&record)?;
        {
            let mut table = txn.open_table(ENTRIES)?;
            table.insert(entry_id, bytes.as_slice())?;
        }

        // Step 5: Adjust counters
        counter::decrement_counter(&txn, status_counter_key(old_status), 1)?;
        counter::increment_counter(&txn, status_counter_key(new_status), 1)?;

        txn.commit()?;
        Ok(())
    }

    /// Delete an entry and all its index entries.
    ///
    /// Removes the entry from ENTRIES, all 5 index tables, VECTOR_MAP
    /// (if present), and decrements the appropriate status counter.
    pub fn delete(&self, entry_id: u64) -> Result<()> {
        let txn = self.db.begin_write()?;

        // Step 1: Read existing record (need data for index cleanup)
        let record = {
            let table = txn.open_table(ENTRIES)?;
            match table.get(entry_id)? {
                Some(guard) => deserialize_entry(guard.value())?,
                None => return Err(StoreError::EntryNotFound(entry_id)),
            }
        };

        // Step 2: Remove from ENTRIES
        {
            let mut table = txn.open_table(ENTRIES)?;
            table.remove(entry_id)?;
        }

        // Step 3: Remove from TOPIC_INDEX
        {
            let mut table = txn.open_table(TOPIC_INDEX)?;
            table.remove((record.topic.as_str(), entry_id))?;
        }

        // Step 4: Remove from CATEGORY_INDEX
        {
            let mut table = txn.open_table(CATEGORY_INDEX)?;
            table.remove((record.category.as_str(), entry_id))?;
        }

        // Step 5: Remove from TAG_INDEX
        {
            let mut table = txn.open_multimap_table(TAG_INDEX)?;
            for tag in &record.tags {
                table.remove(tag.as_str(), entry_id)?;
            }
        }

        // Step 6: Remove from TIME_INDEX
        {
            let mut table = txn.open_table(TIME_INDEX)?;
            table.remove((record.created_at, entry_id))?;
        }

        // Step 7: Remove from STATUS_INDEX
        {
            let mut table = txn.open_table(STATUS_INDEX)?;
            table.remove((record.status as u8, entry_id))?;
        }

        // Step 8: Remove from VECTOR_MAP (if present)
        {
            let mut table = txn.open_table(VECTOR_MAP)?;
            table.remove(entry_id)?; // returns Option, ignore if None
        }

        // Step 9: Decrement status counter
        counter::decrement_counter(&txn, status_counter_key(record.status), 1)?;

        txn.commit()?;
        Ok(())
    }

    /// Write a vector map entry (entry_id -> hnsw_data_id).
    ///
    /// Inserts or overwrites the mapping. Used by nxs-002 (Vector Index).
    pub fn put_vector_mapping(&self, entry_id: u64, hnsw_data_id: u64) -> Result<()> {
        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(VECTOR_MAP)?;
            table.insert(entry_id, hnsw_data_id)?;
        }
        txn.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::{NewEntry, Status};
    use crate::test_helpers::{TestDb, TestEntry};

    // -- R1/AC-04: Atomic Multi-Table Insert --

    #[test]
    fn test_insert_returns_id() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention").build();
        let id = db.store().insert(entry).unwrap();
        assert_eq!(id, 1);
    }

    #[test]
    fn test_insert_populates_all_indexes() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention")
            .with_tags(&["rust", "error"])
            .build();
        let id = db.store().insert(entry).unwrap();

        crate::test_helpers::assert_index_consistent(db.store(), id);
    }

    #[test]
    fn test_insert_50_entries_all_indexed() {
        let db = TestDb::new();
        let ids = crate::test_helpers::seed_entries(db.store(), 50);
        for id in ids {
            crate::test_helpers::assert_index_consistent(db.store(), id);
        }
    }

    // -- R5/AC-05: Monotonic ID Generation --

    #[test]
    fn test_first_id_is_one() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention").build();
        let id = db.store().insert(entry).unwrap();
        assert_eq!(id, 1);
    }

    #[test]
    fn test_100_sequential_inserts_monotonic() {
        let db = TestDb::new();
        let mut prev = 0u64;
        for i in 0..100 {
            let entry =
                TestEntry::new("topic", "category").with_title(&format!("Entry {i}")).build();
            let id = db.store().insert(entry).unwrap();
            assert!(id > prev, "ID {id} not greater than previous {prev}");
            prev = id;
        }
        assert_eq!(prev, 100);
    }

    #[test]
    fn test_counter_matches_last_id() {
        let db = TestDb::new();
        for i in 0..10 {
            let entry =
                TestEntry::new("topic", "category").with_title(&format!("Entry {i}")).build();
            db.store().insert(entry).unwrap();
        }
        let counter = db.store().read_counter("next_entry_id").unwrap();
        assert_eq!(counter, 11); // last assigned was 10, next is 11
    }

    // -- R2/AC-18: Update Path Stale Index Orphaning --

    #[test]
    fn test_update_topic_migrates_index() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention").build();
        let id = db.store().insert(entry).unwrap();

        let mut record = db.store().get(id).unwrap();
        record.topic = "security".to_string();
        db.store().update(record).unwrap();

        // Old topic should not contain entry
        let old_results = db.store().query_by_topic("auth").unwrap();
        assert!(!old_results.iter().any(|r| r.id == id));

        // New topic should contain entry
        let new_results = db.store().query_by_topic("security").unwrap();
        assert!(new_results.iter().any(|r| r.id == id));
    }

    #[test]
    fn test_update_category_migrates_index() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention").build();
        let id = db.store().insert(entry).unwrap();

        let mut record = db.store().get(id).unwrap();
        record.category = "decision".to_string();
        db.store().update(record).unwrap();

        let old = db.store().query_by_category("convention").unwrap();
        assert!(!old.iter().any(|r| r.id == id));

        let new = db.store().query_by_category("decision").unwrap();
        assert!(new.iter().any(|r| r.id == id));
    }

    #[test]
    fn test_update_tags_add_remove() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention")
            .with_tags(&["rust", "error"])
            .build();
        let id = db.store().insert(entry).unwrap();

        let mut record = db.store().get(id).unwrap();
        record.tags = vec!["rust".to_string(), "async".to_string()];
        db.store().update(record).unwrap();

        // "error" tag should no longer have this entry
        let error_results = db
            .store()
            .query_by_tags(&["error".to_string()])
            .unwrap();
        assert!(!error_results.iter().any(|r| r.id == id));

        // "async" tag should now have this entry
        let async_results = db
            .store()
            .query_by_tags(&["async".to_string()])
            .unwrap();
        assert!(async_results.iter().any(|r| r.id == id));

        // "rust" tag should still have this entry
        let rust_results = db
            .store()
            .query_by_tags(&["rust".to_string()])
            .unwrap();
        assert!(rust_results.iter().any(|r| r.id == id));
    }

    #[test]
    fn test_update_multiple_fields_simultaneously() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention")
            .with_tags(&["rust"])
            .build();
        let id = db.store().insert(entry).unwrap();

        let mut record = db.store().get(id).unwrap();
        record.topic = "security".to_string();
        record.category = "decision".to_string();
        record.tags = vec!["go".to_string()];
        db.store().update(record).unwrap();

        // Verify old entries absent
        crate::test_helpers::assert_index_absent(
            db.store(),
            id,
            "auth",
            "convention",
            &["rust".to_string()],
            Status::Active,
        );

        // Verify new entries present
        crate::test_helpers::assert_index_consistent(db.store(), id);
    }

    #[test]
    fn test_update_no_change_indexes_unchanged() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention")
            .with_tags(&["rust"])
            .build();
        let id = db.store().insert(entry).unwrap();

        let record = db.store().get(id).unwrap();
        db.store().update(record).unwrap();

        crate::test_helpers::assert_index_consistent(db.store(), id);
    }

    #[test]
    fn test_update_nonexistent_returns_error() {
        let db = TestDb::new();
        let record = crate::schema::EntryRecord {
            id: 999,
            title: "Ghost".to_string(),
            content: "Does not exist".to_string(),
            topic: "none".to_string(),
            category: "none".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            confidence: 0.0,
            created_at: 0,
            updated_at: 0,
            last_accessed_at: 0,
            access_count: 0,
            supersedes: None,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 0,
        };
        let result = db.store().update(record);
        assert!(matches!(result, Err(crate::StoreError::EntryNotFound(999))));
    }

    // -- R8/AC-12: Status Transition Atomicity --

    #[test]
    fn test_status_active_to_deprecated() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention").build();
        let id = db.store().insert(entry).unwrap();

        db.store().update_status(id, Status::Deprecated).unwrap();

        let record = db.store().get(id).unwrap();
        assert_eq!(record.status, Status::Deprecated);

        // Should not appear in Active status query
        let active = db.store().query_by_status(Status::Active).unwrap();
        assert!(!active.iter().any(|r| r.id == id));

        // Should appear in Deprecated status query
        let deprecated = db.store().query_by_status(Status::Deprecated).unwrap();
        assert!(deprecated.iter().any(|r| r.id == id));

        // Counters should reflect change
        let total_active = db.store().read_counter("total_active").unwrap();
        assert_eq!(total_active, 0);
        let total_deprecated = db.store().read_counter("total_deprecated").unwrap();
        assert_eq!(total_deprecated, 1);
    }

    #[test]
    fn test_status_proposed_to_active() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention")
            .with_status(Status::Proposed)
            .build();
        let id = db.store().insert(entry).unwrap();

        db.store().update_status(id, Status::Active).unwrap();

        let record = db.store().get(id).unwrap();
        assert_eq!(record.status, Status::Active);

        let total_proposed = db.store().read_counter("total_proposed").unwrap();
        assert_eq!(total_proposed, 0);
        let total_active = db.store().read_counter("total_active").unwrap();
        assert_eq!(total_active, 1);
    }

    #[test]
    fn test_status_deprecated_to_active() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention")
            .with_status(Status::Deprecated)
            .build();
        let id = db.store().insert(entry).unwrap();

        db.store().update_status(id, Status::Active).unwrap();

        let record = db.store().get(id).unwrap();
        assert_eq!(record.status, Status::Active);
    }

    #[test]
    fn test_status_same_noop() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention").build();
        let id = db.store().insert(entry).unwrap();

        db.store().update_status(id, Status::Active).unwrap();

        let record = db.store().get(id).unwrap();
        assert_eq!(record.status, Status::Active);
    }

    #[test]
    fn test_counter_consistency_after_transitions() {
        let db = TestDb::new();

        // Insert 3 Active, 2 Deprecated, 1 Proposed
        for _ in 0..3 {
            let e = TestEntry::new("t", "c").build();
            db.store().insert(e).unwrap();
        }
        for _ in 0..2 {
            let e = TestEntry::new("t", "c")
                .with_status(Status::Deprecated)
                .build();
            db.store().insert(e).unwrap();
        }
        let proposed_entry = TestEntry::new("t", "c")
            .with_status(Status::Proposed)
            .build();
        let proposed_id = db.store().insert(proposed_entry).unwrap();

        // Change one Active to Deprecated (ID 1)
        db.store().update_status(1, Status::Deprecated).unwrap();

        assert_eq!(db.store().read_counter("total_active").unwrap(), 2);
        assert_eq!(db.store().read_counter("total_deprecated").unwrap(), 3);
        assert_eq!(db.store().read_counter("total_proposed").unwrap(), 1);

        // Change Proposed to Active
        db.store().update_status(proposed_id, Status::Active).unwrap();
        assert_eq!(db.store().read_counter("total_active").unwrap(), 3);
        assert_eq!(db.store().read_counter("total_proposed").unwrap(), 0);
    }

    #[test]
    fn test_status_update_nonexistent_returns_error() {
        let db = TestDb::new();
        let result = db.store().update_status(999, Status::Active);
        assert!(matches!(result, Err(crate::StoreError::EntryNotFound(999))));
    }

    // -- R11/AC-13: VECTOR_MAP --

    #[test]
    fn test_put_vector_mapping_and_read() {
        let db = TestDb::new();
        db.store().put_vector_mapping(42, 7).unwrap();
        let val = db.store().get_vector_mapping(42).unwrap();
        assert_eq!(val, Some(7));
    }

    #[test]
    fn test_vector_mapping_overwrite() {
        let db = TestDb::new();
        db.store().put_vector_mapping(42, 7).unwrap();
        db.store().put_vector_mapping(42, 99).unwrap();
        let val = db.store().get_vector_mapping(42).unwrap();
        assert_eq!(val, Some(99));
    }

    #[test]
    fn test_vector_mapping_nonexistent() {
        let db = TestDb::new();
        let val = db.store().get_vector_mapping(999).unwrap();
        assert_eq!(val, None);
    }

    #[test]
    fn test_vector_mapping_u64_max() {
        let db = TestDb::new();
        db.store().put_vector_mapping(1, u64::MAX).unwrap();
        let val = db.store().get_vector_mapping(1).unwrap();
        assert_eq!(val, Some(u64::MAX));
    }

    // -- Delete --

    #[test]
    fn test_delete_removes_all_indexes() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention")
            .with_tags(&["rust", "error"])
            .build();
        let id = db.store().insert(entry).unwrap();
        db.store().put_vector_mapping(id, 42).unwrap();

        db.store().delete(id).unwrap();

        // Entry should not exist
        assert!(!db.store().exists(id).unwrap());

        // No index should contain it
        assert!(db.store().query_by_topic("auth").unwrap().is_empty());
        assert!(db.store().query_by_category("convention").unwrap().is_empty());
        assert!(db
            .store()
            .query_by_tags(&["rust".to_string()])
            .unwrap()
            .is_empty());
        assert!(db
            .store()
            .query_by_status(Status::Active)
            .unwrap()
            .is_empty());
        assert_eq!(db.store().get_vector_mapping(id).unwrap(), None);
        assert_eq!(db.store().read_counter("total_active").unwrap(), 0);
    }

    #[test]
    fn test_delete_nonexistent_returns_error() {
        let db = TestDb::new();
        let result = db.store().delete(999);
        assert!(matches!(result, Err(crate::StoreError::EntryNotFound(999))));
    }

    // -- AC-14: Close and Reopen --

    #[test]
    fn test_close_and_reopen_preserves_data() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");

        // Open, insert, drop
        let id = {
            let store = crate::Store::open(&path).unwrap();
            let entry = NewEntry {
                title: "Persisted".to_string(),
                content: "Should survive reopen".to_string(),
                topic: "auth".to_string(),
                category: "convention".to_string(),
                tags: vec!["rust".to_string()],
                source: "test".to_string(),
                status: Status::Active,
            };
            store.insert(entry).unwrap()
        };

        // Reopen and verify
        let store = crate::Store::open(&path).unwrap();
        let record = store.get(id).unwrap();
        assert_eq!(record.title, "Persisted");
        assert_eq!(record.content, "Should survive reopen");
        assert_eq!(record.topic, "auth");
    }
}
