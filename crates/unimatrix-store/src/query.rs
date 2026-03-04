use redb::ReadableDatabase;

use crate::db::Store;
use crate::error::Result;
use crate::read;
use crate::schema::{EntryRecord, QueryFilter, Status};

impl Store {
    /// Combined query with set intersection across all specified filters.
    ///
    /// When all fields are `None`, returns all entries with `Status::Active`.
    /// When one or more fields are set, results are the intersection of all
    /// individual index queries.
    pub fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>> {
        let txn = self.db.begin_read()?;

        // Determine effective filter: empty filter defaults to all active
        let is_empty = filter.topic.is_none()
            && filter.category.is_none()
            && filter.tags.is_none()
            && filter.status.is_none()
            && filter.time_range.is_none();

        let effective_status = if is_empty {
            Some(Status::Active)
        } else {
            filter.status
        };

        // Collect ID sets for each present filter field
        let mut sets: Vec<std::collections::HashSet<u64>> = Vec::new();

        if let Some(ref topic) = filter.topic {
            sets.push(read::collect_ids_by_topic(&txn, topic)?);
        }

        if let Some(ref category) = filter.category {
            sets.push(read::collect_ids_by_category(&txn, category)?);
        }

        if let Some(ref tags) = filter.tags
            && !tags.is_empty()
        {
            sets.push(read::collect_ids_by_tags(&txn, tags)?);
        }

        if let Some(status) = effective_status {
            sets.push(read::collect_ids_by_status(&txn, status)?);
        }

        if let Some(range) = filter.time_range
            && range.start <= range.end
        {
            sets.push(read::collect_ids_by_time_range(&txn, range)?);
        }

        // Intersect all sets
        if sets.is_empty() {
            // No filters at all and no default -- return all active
            let ids = read::collect_ids_by_status(&txn, Status::Active)?;
            return read::fetch_entries(&txn, &ids);
        }

        let mut result_ids = sets.remove(0);
        for set in sets {
            result_ids = result_ids.intersection(&set).copied().collect();
        }

        read::fetch_entries(&txn, &result_ids)
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::{QueryFilter, Status, TimeRange};
    use crate::test_helpers::{TestDb, TestEntry};

    // -- R7/AC-17: QueryFilter Combined Query --

    #[test]
    fn test_empty_filter_returns_all_active() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("t1", "c1").build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("t2", "c2").build())
            .unwrap();
        db.store()
            .insert(
                TestEntry::new("t3", "c3")
                    .with_status(Status::Deprecated)
                    .build(),
            )
            .unwrap();

        let results = db.store().query(QueryFilter::default()).unwrap();
        assert_eq!(results.len(), 2);
        for r in &results {
            assert_eq!(r.status, Status::Active);
        }
    }

    #[test]
    fn test_single_field_topic() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("auth", "c1").build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("logging", "c2").build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("auth", "c3").build())
            .unwrap();

        let filter = QueryFilter {
            topic: Some("auth".to_string()),
            ..Default::default()
        };
        let results = db.store().query(filter).unwrap();
        assert_eq!(results.len(), 2);
        for r in &results {
            assert_eq!(r.topic, "auth");
        }
    }

    #[test]
    fn test_single_field_status() {
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

        let filter = QueryFilter {
            status: Some(Status::Deprecated),
            ..Default::default()
        };
        let results = db.store().query(filter).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].status, Status::Deprecated);
    }

    #[test]
    fn test_two_fields_topic_and_status() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("auth", "c1").build())
            .unwrap(); // Active
        db.store()
            .insert(
                TestEntry::new("auth", "c2")
                    .with_status(Status::Deprecated)
                    .build(),
            )
            .unwrap();
        db.store()
            .insert(TestEntry::new("logging", "c3").build())
            .unwrap(); // Active

        let filter = QueryFilter {
            topic: Some("auth".to_string()),
            status: Some(Status::Active),
            ..Default::default()
        };
        let results = db.store().query(filter).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].topic, "auth");
        assert_eq!(results[0].status, Status::Active);
    }

    #[test]
    fn test_two_fields_tags_and_status() {
        let db = TestDb::new();
        db.store()
            .insert(
                TestEntry::new("t", "c")
                    .with_tags(&["rust"])
                    .build(),
            )
            .unwrap(); // Active
        db.store()
            .insert(
                TestEntry::new("t", "c")
                    .with_tags(&["rust"])
                    .with_status(Status::Deprecated)
                    .build(),
            )
            .unwrap();
        db.store()
            .insert(
                TestEntry::new("t", "c")
                    .with_tags(&["python"])
                    .build(),
            )
            .unwrap(); // Active

        let filter = QueryFilter {
            tags: Some(vec!["rust".to_string()]),
            status: Some(Status::Active),
            ..Default::default()
        };
        let results = db.store().query(filter).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tags, vec!["rust".to_string()]);
        assert_eq!(results[0].status, Status::Active);
    }

    #[test]
    fn test_disjoint_filters_empty_result() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("auth", "convention").build())
            .unwrap();
        db.store()
            .insert(TestEntry::new("logging", "decision").build())
            .unwrap();

        // auth + decision: no entry matches both
        let filter = QueryFilter {
            topic: Some("auth".to_string()),
            category: Some("decision".to_string()),
            ..Default::default()
        };
        let results = db.store().query(filter).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_nonexistent_topic_filter() {
        let db = TestDb::new();
        db.store()
            .insert(TestEntry::new("auth", "c").build())
            .unwrap();

        let filter = QueryFilter {
            topic: Some("nonexistent".to_string()),
            ..Default::default()
        };
        let results = db.store().query(filter).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_all_fields_populated() {
        let db = TestDb::new();
        // Insert a single entry that matches all criteria
        db.store()
            .insert(
                TestEntry::new("auth", "convention")
                    .with_tags(&["jwt"])
                    .build(),
            )
            .unwrap();
        // Insert entries that match some but not all criteria
        db.store()
            .insert(TestEntry::new("auth", "decision").build())
            .unwrap();
        db.store()
            .insert(
                TestEntry::new("logging", "convention")
                    .with_tags(&["jwt"])
                    .build(),
            )
            .unwrap();

        let all_active = db.store().query_by_status(Status::Active).unwrap();
        let ts = all_active[0].created_at;

        let filter = QueryFilter {
            topic: Some("auth".to_string()),
            category: Some("convention".to_string()),
            tags: Some(vec!["jwt".to_string()]),
            status: Some(Status::Active),
            time_range: Some(TimeRange {
                start: ts,
                end: ts,
            }),
        };
        let results = db.store().query(filter).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].topic, "auth");
        assert_eq!(results[0].category, "convention");
        assert!(results[0].tags.contains(&"jwt".to_string()));
    }

    #[test]
    fn test_50_entries_varied_subsets() {
        let db = TestDb::new();
        let ids = crate::test_helpers::seed_entries(db.store(), 50);
        assert_eq!(ids.len(), 50);

        // Query by topic "auth" (should be ~10 entries since 50/5 = 10)
        let results = db.store().query_by_topic("auth").unwrap();
        assert_eq!(results.len(), 10);

        // Query by category "convention" (should be ~17 since 50/3 ~ 17)
        let results = db.store().query_by_category("convention").unwrap();
        assert_eq!(results.len(), 17);

        // Combined: topic="auth" + status=Active
        let filter = QueryFilter {
            topic: Some("auth".to_string()),
            status: Some(Status::Active),
            ..Default::default()
        };
        let results = db.store().query(filter).unwrap();
        // All seed entries are Active, so should get all 10 auth entries
        assert_eq!(results.len(), 10);
    }
}
