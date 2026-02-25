use std::collections::HashSet;

use redb::{ReadTransaction, ReadableDatabase, ReadableTable};

use crate::db::Store;
use crate::error::{Result, StoreError};
use crate::schema::{
    CATEGORY_INDEX, COUNTERS, ENTRIES, EntryRecord, STATUS_INDEX, Status, TAG_INDEX, TIME_INDEX,
    TOPIC_INDEX, TimeRange, VECTOR_MAP, deserialize_entry,
};

// -- Internal helpers shared with query.rs (C8) --

/// Collect entry IDs matching a topic via TOPIC_INDEX range scan.
pub(crate) fn collect_ids_by_topic(txn: &ReadTransaction, topic: &str) -> Result<HashSet<u64>> {
    let table = txn.open_table(TOPIC_INDEX)?;
    let mut ids = HashSet::new();
    for result in table.range((topic, 0u64)..=(topic, u64::MAX))? {
        let (key, _) = result?;
        let (_, entry_id) = key.value();
        ids.insert(entry_id);
    }
    Ok(ids)
}

/// Collect entry IDs matching a category via CATEGORY_INDEX range scan.
pub(crate) fn collect_ids_by_category(
    txn: &ReadTransaction,
    category: &str,
) -> Result<HashSet<u64>> {
    let table = txn.open_table(CATEGORY_INDEX)?;
    let mut ids = HashSet::new();
    for result in table.range((category, 0u64)..=(category, u64::MAX))? {
        let (key, _) = result?;
        ids.insert(key.value().1);
    }
    Ok(ids)
}

/// Collect entry IDs matching ALL tags via TAG_INDEX (intersection).
pub(crate) fn collect_ids_by_tags(txn: &ReadTransaction, tags: &[String]) -> Result<HashSet<u64>> {
    let table = txn.open_multimap_table(TAG_INDEX)?;
    let mut result_set: Option<HashSet<u64>> = None;

    for tag in tags {
        let mut tag_ids = HashSet::new();
        let values = table.get(tag.as_str())?;
        for result in values {
            let guard = result?;
            tag_ids.insert(guard.value());
        }

        result_set = match result_set {
            None => Some(tag_ids),
            Some(existing) => Some(existing.intersection(&tag_ids).copied().collect()),
        };
    }

    Ok(result_set.unwrap_or_default())
}

/// Collect entry IDs within a time range via TIME_INDEX range scan.
pub(crate) fn collect_ids_by_time_range(
    txn: &ReadTransaction,
    range: TimeRange,
) -> Result<HashSet<u64>> {
    let table = txn.open_table(TIME_INDEX)?;
    let mut ids = HashSet::new();
    for result in table.range((range.start, 0u64)..=(range.end, u64::MAX))? {
        let (key, _) = result?;
        ids.insert(key.value().1);
    }
    Ok(ids)
}

/// Collect entry IDs with a given status via STATUS_INDEX range scan.
pub(crate) fn collect_ids_by_status(
    txn: &ReadTransaction,
    status: Status,
) -> Result<HashSet<u64>> {
    let table = txn.open_table(STATUS_INDEX)?;
    let status_byte = status as u8;
    let mut ids = HashSet::new();
    for result in table.range((status_byte, 0u64)..=(status_byte, u64::MAX))? {
        let (key, _) = result?;
        ids.insert(key.value().1);
    }
    Ok(ids)
}

/// Fetch full EntryRecords for a set of IDs from the ENTRIES table.
pub(crate) fn fetch_entries(
    txn: &ReadTransaction,
    ids: &HashSet<u64>,
) -> Result<Vec<EntryRecord>> {
    let table = txn.open_table(ENTRIES)?;
    let mut results = Vec::with_capacity(ids.len());
    for &id in ids {
        if let Some(guard) = table.get(id)? {
            results.push(deserialize_entry(guard.value())?);
        }
        // If entry was deleted between index scan and fetch, skip silently
    }
    Ok(results)
}

// -- Public Store methods --

impl Store {
    /// Get a single entry by ID.
    ///
    /// Returns `StoreError::EntryNotFound` if the entry does not exist.
    pub fn get(&self, entry_id: u64) -> Result<EntryRecord> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(ENTRIES)?;
        match table.get(entry_id)? {
            Some(guard) => deserialize_entry(guard.value()),
            None => Err(StoreError::EntryNotFound(entry_id)),
        }
    }

    /// Check if an entry exists without deserializing it.
    pub fn exists(&self, entry_id: u64) -> Result<bool> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(ENTRIES)?;
        Ok(table.get(entry_id)?.is_some())
    }

    /// Query entries by topic.
    pub fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>> {
        let txn = self.db.begin_read()?;
        let ids = collect_ids_by_topic(&txn, topic)?;
        fetch_entries(&txn, &ids)
    }

    /// Query entries by category.
    pub fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>> {
        let txn = self.db.begin_read()?;
        let ids = collect_ids_by_category(&txn, category)?;
        fetch_entries(&txn, &ids)
    }

    /// Query entries matching ALL specified tags (intersection).
    ///
    /// Returns empty vec if tags slice is empty.
    pub fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>> {
        if tags.is_empty() {
            return Ok(vec![]);
        }
        let txn = self.db.begin_read()?;
        let ids = collect_ids_by_tags(&txn, tags)?;
        fetch_entries(&txn, &ids)
    }

    /// Query entries within a time range (inclusive on both ends).
    ///
    /// Returns empty vec if start > end (inverted range).
    pub fn query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>> {
        if range.start > range.end {
            return Ok(vec![]);
        }
        let txn = self.db.begin_read()?;
        let ids = collect_ids_by_time_range(&txn, range)?;
        fetch_entries(&txn, &ids)
    }

    /// Query entries with a given status.
    pub fn query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>> {
        let txn = self.db.begin_read()?;
        let ids = collect_ids_by_status(&txn, status)?;
        fetch_entries(&txn, &ids)
    }

    /// Look up the hnsw_data_id for an entry in VECTOR_MAP.
    ///
    /// Returns `None` if no mapping exists for this entry.
    pub fn get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(VECTOR_MAP)?;
        match table.get(entry_id)? {
            Some(guard) => Ok(Some(guard.value())),
            None => Ok(None),
        }
    }

    /// Iterate all entries in the VECTOR_MAP table.
    ///
    /// Returns all `(entry_id, hnsw_data_id)` pairs stored in VECTOR_MAP.
    /// Used by unimatrix-vector to rebuild the IdMap from the crash-safe
    /// source of truth on index load.
    ///
    /// Returns an empty `Vec` if no vector mappings exist.
    pub fn iter_vector_mappings(&self) -> Result<Vec<(u64, u64)>> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(VECTOR_MAP)?;

        let mut mappings = Vec::new();
        for result in table.iter()? {
            let (key, value) = result?;
            mappings.push((key.value(), value.value()));
        }
        Ok(mappings)
    }

    /// Read a named counter value. Returns 0 if the counter does not exist.
    pub fn read_counter(&self, name: &str) -> Result<u64> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(COUNTERS)?;
        match table.get(name)? {
            Some(guard) => Ok(guard.value()),
            None => Ok(0),
        }
    }

    /// Get all co-access partners for an entry, filtering by staleness.
    ///
    /// Per ADR-001: prefix scan for (entry, *) + full scan for (*, entry).
    /// Returns partners as (partner_entry_id, CoAccessRecord).
    pub fn get_co_access_partners(
        &self,
        entry_id: u64,
        staleness_cutoff: u64,
    ) -> Result<Vec<(u64, crate::schema::CoAccessRecord)>> {
        use crate::schema::{CO_ACCESS, deserialize_co_access};

        let txn = self.db.begin_read()?;
        let table = txn.open_table(CO_ACCESS)?;
        let mut partners = Vec::new();

        // Scan 1: pairs where entry_id is the min (prefix scan)
        for result in table.range((entry_id, 0u64)..=(entry_id, u64::MAX))? {
            let (key, value) = result?;
            let (_, partner_id) = key.value();
            if partner_id == entry_id {
                continue; // skip self-pair (shouldn't exist, but defensive)
            }
            let record = deserialize_co_access(value.value())?;
            if record.last_updated >= staleness_cutoff {
                partners.push((partner_id, record));
            }
        }

        // Scan 2: pairs where entry_id is the max (full table scan)
        for result in table.iter()? {
            let (key, value) = result?;
            let (min_id, max_id) = key.value();
            if max_id == entry_id && min_id != entry_id {
                let record = deserialize_co_access(value.value())?;
                if record.last_updated >= staleness_cutoff {
                    partners.push((min_id, record));
                }
            }
        }

        Ok(partners)
    }

    /// Get co-access statistics.
    /// Returns (total_pairs, active_pairs_after_staleness).
    pub fn co_access_stats(&self, staleness_cutoff: u64) -> Result<(u64, u64)> {
        use crate::schema::{CO_ACCESS, deserialize_co_access};

        let txn = self.db.begin_read()?;
        let table = txn.open_table(CO_ACCESS)?;
        let mut total = 0u64;
        let mut active = 0u64;

        for result in table.iter()? {
            let (_, value) = result?;
            total += 1;
            let record = deserialize_co_access(value.value())?;
            if record.last_updated >= staleness_cutoff {
                active += 1;
            }
        }

        Ok((total, active))
    }

    /// Get top N co-access pairs by count (non-stale only).
    pub fn top_co_access_pairs(
        &self,
        n: usize,
        staleness_cutoff: u64,
    ) -> Result<Vec<((u64, u64), crate::schema::CoAccessRecord)>> {
        use crate::schema::{CO_ACCESS, deserialize_co_access};

        let txn = self.db.begin_read()?;
        let table = txn.open_table(CO_ACCESS)?;
        let mut pairs = Vec::new();

        for result in table.iter()? {
            let (key, value) = result?;
            let record = deserialize_co_access(value.value())?;
            if record.last_updated >= staleness_cutoff {
                pairs.push((key.value(), record));
            }
        }

        pairs.sort_by(|a, b| b.1.count.cmp(&a.1.count));
        pairs.truncate(n);
        Ok(pairs)
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::{NewEntry, Status, TimeRange};
    use crate::test_helpers::{TestDb, TestEntry};

    // -- AC-06: Point Lookup --

    #[test]
    fn test_get_returns_inserted_entry() {
        let db = TestDb::new();
        let entry = TestEntry::new("auth", "convention")
            .with_tags(&["rust"])
            .with_content("detailed content")
            .build();
        let id = db.store().insert(entry).unwrap();

        let record = db.store().get(id).unwrap();
        assert_eq!(record.id, id);
        assert_eq!(record.topic, "auth");
        assert_eq!(record.category, "convention");
        assert_eq!(record.tags, vec!["rust".to_string()]);
        assert_eq!(record.content, "detailed content");
        assert_eq!(record.status, Status::Active);
    }

    #[test]
    fn test_get_nonexistent_returns_error() {
        let db = TestDb::new();
        let result = db.store().get(999);
        assert!(matches!(result, Err(crate::StoreError::EntryNotFound(999))));
    }

    // -- AC-07: Topic Index Query --

    #[test]
    fn test_query_by_topic_returns_matching() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("auth", "c1").build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("auth", "c2").build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("logging", "c1").build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("auth", "c3").build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("database", "c1").build())
            .unwrap();

        let results = db.store().query_by_topic("auth").unwrap();
        assert_eq!(results.len(), 3);
        for r in &results {
            assert_eq!(r.topic, "auth");
        }
    }

    #[test]
    fn test_query_by_topic_nonexistent() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("auth", "c1").build())
            .unwrap();
        let results = db.store().query_by_topic("nonexistent").unwrap();
        assert!(results.is_empty());
    }

    // -- AC-08: Category Index Query --

    #[test]
    fn test_query_by_category_returns_matching() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("t1", "convention").build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("t2", "decision").build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("t3", "convention").build())
            .unwrap();

        let results = db.store().query_by_category("convention").unwrap();
        assert_eq!(results.len(), 2);
        for r in &results {
            assert_eq!(r.category, "convention");
        }
    }

    #[test]
    fn test_query_by_category_nonexistent() {
        let db = TestDb::new();
        let results = db.store().query_by_category("nonexistent").unwrap();
        assert!(results.is_empty());
    }

    // -- R9/AC-09: Tag Intersection --

    #[test]
    fn test_query_single_tag() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("t", "c").with_tags(&["rust"]).build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("t", "c").with_tags(&["rust", "error"]).build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("t", "c").with_tags(&["python"]).build())
            .unwrap();

        let results = db
            .store()
            .query_by_tags(&["rust".to_string()])
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_query_two_tag_intersection() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("t", "c").with_tags(&["rust", "error"]).build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("t", "c").with_tags(&["rust", "async"]).build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("t", "c").with_tags(&["error", "python"]).build())
            .unwrap();

        let results = db
            .store()
            .query_by_tags(&["rust".to_string(), "error".to_string()])
            .unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_query_three_tag_intersection() {
        let db = TestDb::new();
        db.store()
            .insert(
                TestEntry::new("t", "c")
                    .with_tags(&["rust", "error", "async"])
                    .build(),
            )
            .unwrap();
        db.store()
            .insert(TestEntry::new("t", "c").with_tags(&["rust", "error"]).build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("t", "c").with_tags(&["rust", "async"]).build())
            .unwrap();

        let results = db
            .store()
            .query_by_tags(&[
                "rust".to_string(),
                "error".to_string(),
                "async".to_string(),
            ])
            .unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_query_nonexistent_tag() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("t", "c").with_tags(&["rust"]).build())
            .unwrap();
        let results = db
            .store()
            .query_by_tags(&["nonexistent".to_string()])
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_query_empty_tags() {
        let db = TestDb::new();
        let results = db.store().query_by_tags(&[]).unwrap();
        assert!(results.is_empty());
    }

    // -- AC-10: Time Range Query --

    #[test]
    fn test_time_range_inclusive() {
        let db = TestDb::new();
        // Insert entries with specific timestamps using low-level insert
        for ts in [1000u64, 2000, 3000, 4000, 5000] {
            let entry = NewEntry {
                title: format!("Entry at {ts}"),
                content: "content".to_string(),
                topic: "t".to_string(),
                category: "c".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: Status::Active,
                created_by: String::new(),
                feature_cycle: String::new(),
                trust_source: String::new(),
            };
            db.store().insert(entry).unwrap();
        }

        // All entries have the same created_at (current time), so we can't test
        // time range with insert alone. Instead, let's query all entries and check
        // that the time range query works with the actual timestamps.
        let all = db.store().query_by_status(Status::Active).unwrap();
        assert_eq!(all.len(), 5);

        // Query with a range that includes all entries' created_at
        let ts = all[0].created_at;
        let results = db
            .store()
            .query_by_time_range(TimeRange {
                start: ts,
                end: ts,
            })
            .unwrap();
        assert_eq!(results.len(), 5); // all share the same second
    }

    #[test]
    fn test_time_range_inverted() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("t", "c").build())
            .unwrap();
        let results = db
            .store()
            .query_by_time_range(TimeRange {
                start: 5000,
                end: 1000,
            })
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_time_range_empty() {
        let db = TestDb::new();
        // Query a range far in the past
        let results = db
            .store()
            .query_by_time_range(TimeRange { start: 0, end: 1 })
            .unwrap();
        assert!(results.is_empty());
    }

    // -- AC-11: Status Query --

    #[test]
    fn test_query_by_status_active() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("t", "c").build())
            .unwrap();
        db.store()
            .insert(
                TestEntry::new("t", "c")
                    .with_status(Status::Deprecated)
                    .build(),
            )
            .unwrap();
        db.store()
            .insert(TestEntry::new("t", "c").build())
            .unwrap();

        let results = db.store().query_by_status(Status::Active).unwrap();
        assert_eq!(results.len(), 2);
        for r in &results {
            assert_eq!(r.status, Status::Active);
        }
    }

    #[test]
    fn test_query_by_status_deprecated() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("t", "c").build())
            .unwrap();
        db.store()
            .insert(
                TestEntry::new("t", "c")
                    .with_status(Status::Deprecated)
                    .build(),
            )
            .unwrap();

        let results = db.store().query_by_status(Status::Deprecated).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, Status::Deprecated);
    }

    // -- Exists --

    #[test]
    fn test_exists_true() {
        let db = TestDb::new();
        let id = db
            .store()
            .insert(TestEntry::new("t", "c").build())
            .unwrap();
        assert!(db.store().exists(id).unwrap());
    }

    #[test]
    fn test_exists_false() {
        let db = TestDb::new();
        assert!(!db.store().exists(999).unwrap());
    }

    // -- Counter Read --

    #[test]
    fn test_read_counter_missing_key() {
        let db = TestDb::new();
        let val = db.store().read_counter("nonexistent").unwrap();
        assert_eq!(val, 0);
    }

    // -- iter_vector_mappings (W1 alignment: nxs-002) --

    #[test]
    fn test_iter_vector_mappings_empty() {
        let db = TestDb::new();
        let mappings = db.store().iter_vector_mappings().unwrap();
        assert!(mappings.is_empty());
    }

    #[test]
    fn test_iter_vector_mappings_populated() {
        let db = TestDb::new();
        db.store().put_vector_mapping(1, 100).unwrap();
        db.store().put_vector_mapping(2, 200).unwrap();
        db.store().put_vector_mapping(3, 300).unwrap();

        let mappings = db.store().iter_vector_mappings().unwrap();
        assert_eq!(mappings.len(), 3);
        assert!(mappings.contains(&(1, 100)));
        assert!(mappings.contains(&(2, 200)));
        assert!(mappings.contains(&(3, 300)));
    }

    #[test]
    fn test_iter_vector_mappings_after_overwrite() {
        let db = TestDb::new();
        db.store().put_vector_mapping(1, 100).unwrap();
        db.store().put_vector_mapping(1, 999).unwrap(); // overwrite
        db.store().put_vector_mapping(2, 200).unwrap();

        let mappings = db.store().iter_vector_mappings().unwrap();
        assert_eq!(mappings.len(), 2);
        assert!(mappings.contains(&(1, 999)));
        assert!(mappings.contains(&(2, 200)));
    }

    #[test]
    fn test_iter_vector_mappings_consistency_with_get() {
        let db = TestDb::new();
        for i in 1..=50 {
            db.store().put_vector_mapping(i, i * 10).unwrap();
        }

        let mappings = db.store().iter_vector_mappings().unwrap();
        assert_eq!(mappings.len(), 50);

        for (entry_id, data_id) in &mappings {
            let got = db.store().get_vector_mapping(*entry_id).unwrap();
            assert_eq!(got, Some(*data_id));
        }
    }

    #[test]
    fn test_iter_vector_mappings_after_delete() {
        let db = TestDb::new();
        let entry = TestEntry::new("t", "c").build();
        let id = db.store().insert(entry).unwrap();
        db.store().put_vector_mapping(id, 100).unwrap();

        db.store().delete(id).unwrap();

        let mappings = db.store().iter_vector_mappings().unwrap();
        assert!(mappings.is_empty());
    }

    #[test]
    fn test_read_counter_after_inserts() {
        let db = TestDb::new();
        for _ in 0..5 {
            db.store()
                .insert(TestEntry::new("t", "c").build())
                .unwrap();
        }
        let total = db.store().read_counter("total_active").unwrap();
        assert_eq!(total, 5);
    }

    // -- crt-004: Co-access read tests --

    #[test]
    fn test_get_co_access_partners_as_min() {
        use crate::schema::{CO_ACCESS, CoAccessRecord, co_access_key, serialize_co_access};
        let db = TestDb::new();
        // Insert pairs where entry 5 is the min
        {
            let txn = db.store().db.begin_write().unwrap();
            {
                let mut table = txn.open_table(CO_ACCESS).unwrap();
                let r1 = CoAccessRecord { count: 3, last_updated: 5000 };
                let r2 = CoAccessRecord { count: 1, last_updated: 5000 };
                table.insert(co_access_key(5, 10), serialize_co_access(&r1).unwrap().as_slice()).unwrap();
                table.insert(co_access_key(5, 20), serialize_co_access(&r2).unwrap().as_slice()).unwrap();
            }
            txn.commit().unwrap();
        }

        let partners = db.store().get_co_access_partners(5, 0).unwrap();
        assert_eq!(partners.len(), 2);
        let ids: Vec<u64> = partners.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&10));
        assert!(ids.contains(&20));
    }

    #[test]
    fn test_get_co_access_partners_as_max() {
        use crate::schema::{CO_ACCESS, CoAccessRecord, co_access_key, serialize_co_access};
        let db = TestDb::new();
        // Insert pairs where entry 10 is the max
        {
            let txn = db.store().db.begin_write().unwrap();
            {
                let mut table = txn.open_table(CO_ACCESS).unwrap();
                let r1 = CoAccessRecord { count: 2, last_updated: 5000 };
                let r2 = CoAccessRecord { count: 3, last_updated: 5000 };
                table.insert(co_access_key(1, 10), serialize_co_access(&r1).unwrap().as_slice()).unwrap();
                table.insert(co_access_key(5, 10), serialize_co_access(&r2).unwrap().as_slice()).unwrap();
            }
            txn.commit().unwrap();
        }

        let partners = db.store().get_co_access_partners(10, 0).unwrap();
        assert_eq!(partners.len(), 2);
        let ids: Vec<u64> = partners.iter().map(|(id, _)| *id).collect();
        assert!(ids.contains(&1));
        assert!(ids.contains(&5));
    }

    #[test]
    fn test_get_co_access_partners_staleness_filter() {
        use crate::schema::{CO_ACCESS, CoAccessRecord, co_access_key, serialize_co_access};
        let db = TestDb::new();
        {
            let txn = db.store().db.begin_write().unwrap();
            {
                let mut table = txn.open_table(CO_ACCESS).unwrap();
                let stale = CoAccessRecord { count: 5, last_updated: 1000 };
                table.insert(co_access_key(1, 2), serialize_co_access(&stale).unwrap().as_slice()).unwrap();
            }
            txn.commit().unwrap();
        }

        // staleness_cutoff=2000 means only records with last_updated >= 2000 pass
        let partners = db.store().get_co_access_partners(1, 2000).unwrap();
        assert!(partners.is_empty(), "stale pair should be filtered out");
    }

    #[test]
    fn test_get_co_access_partners_no_partners() {
        let db = TestDb::new();
        let partners = db.store().get_co_access_partners(999, 0).unwrap();
        assert!(partners.is_empty());
    }

    #[test]
    fn test_co_access_stats() {
        use crate::schema::{CO_ACCESS, CoAccessRecord, co_access_key, serialize_co_access};
        let db = TestDb::new();
        {
            let txn = db.store().db.begin_write().unwrap();
            {
                let mut table = txn.open_table(CO_ACCESS).unwrap();
                let fresh = CoAccessRecord { count: 1, last_updated: 5000 };
                let fresh2 = CoAccessRecord { count: 2, last_updated: 5000 };
                let stale = CoAccessRecord { count: 3, last_updated: 1000 };
                table.insert(co_access_key(1, 2), serialize_co_access(&fresh).unwrap().as_slice()).unwrap();
                table.insert(co_access_key(3, 4), serialize_co_access(&fresh2).unwrap().as_slice()).unwrap();
                table.insert(co_access_key(5, 6), serialize_co_access(&stale).unwrap().as_slice()).unwrap();
            }
            txn.commit().unwrap();
        }

        let (total, active) = db.store().co_access_stats(3000).unwrap();
        assert_eq!(total, 3);
        assert_eq!(active, 2);
    }

    #[test]
    fn test_top_co_access_pairs_ordering_and_limit() {
        use crate::schema::{CO_ACCESS, CoAccessRecord, co_access_key, serialize_co_access};
        let db = TestDb::new();
        {
            let txn = db.store().db.begin_write().unwrap();
            {
                let mut table = txn.open_table(CO_ACCESS).unwrap();
                for i in 1..=5 {
                    let record = CoAccessRecord { count: i * 10, last_updated: 5000 };
                    table.insert(co_access_key(i as u64, (i + 10) as u64), serialize_co_access(&record).unwrap().as_slice()).unwrap();
                }
                // Add a stale pair with highest count
                let stale = CoAccessRecord { count: 100, last_updated: 1000 };
                table.insert(co_access_key(99, 100), serialize_co_access(&stale).unwrap().as_slice()).unwrap();
            }
            txn.commit().unwrap();
        }

        let top = db.store().top_co_access_pairs(3, 3000).unwrap();
        assert_eq!(top.len(), 3);
        // Should be ordered by count descending
        assert_eq!(top[0].1.count, 50); // pair (5, 15)
        assert_eq!(top[1].1.count, 40); // pair (4, 14)
        assert_eq!(top[2].1.count, 30); // pair (3, 13)
        // Stale pair with count=100 should be excluded
    }

    #[test]
    fn test_co_access_stats_empty_table() {
        let db = TestDb::new();
        let (total, active) = db.store().co_access_stats(0).unwrap();
        assert_eq!(total, 0);
        assert_eq!(active, 0);
    }
}
