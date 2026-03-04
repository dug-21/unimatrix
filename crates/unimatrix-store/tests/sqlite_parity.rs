//! Parity tests for the SQLite backend.
//!
//! These tests exercise all Store operations to verify behavioral parity
//! with the redb backend. They run ONLY under `--features backend-sqlite`.
//!
//! Coverage: CRUD, queries, usage tracking, confidence, vector map,
//! feature entries, co-access, metrics, counters.
//! See sqlite_parity_specialized.rs for signals, sessions, injection log.

#![cfg(feature = "backend-sqlite")]

use unimatrix_store::{QueryFilter, Status, StoreError, TimeRange};
use unimatrix_store::test_helpers::{TestDb, TestEntry, assert_index_consistent};

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// === BASIC CRUD ===

#[test]
fn test_insert_and_get() {
    let db = TestDb::new();
    let entry = TestEntry::new("auth", "convention").build();
    let id = db.store().insert(entry).unwrap();
    assert!(id >= 1);

    let record = db.store().get(id).unwrap();
    assert_eq!(record.id, id);
    assert_eq!(record.topic, "auth");
    assert_eq!(record.category, "convention");
    assert_eq!(record.status, Status::Active);
}

#[test]
fn test_exists() {
    let db = TestDb::new();
    let entry = TestEntry::new("test", "pattern").build();
    let id = db.store().insert(entry).unwrap();
    assert!(db.store().exists(id).unwrap());
    assert!(!db.store().exists(99999).unwrap());
}

#[test]
fn test_insert_multiple_sequential_ids() {
    let db = TestDb::new();
    let id1 = db.store().insert(TestEntry::new("a", "b").build()).unwrap();
    let id2 = db.store().insert(TestEntry::new("c", "d").build()).unwrap();
    let id3 = db.store().insert(TestEntry::new("e", "f").build()).unwrap();
    assert_eq!(id2, id1 + 1);
    assert_eq!(id3, id2 + 1);
}

#[test]
fn test_get_not_found() {
    let db = TestDb::new();
    let result = db.store().get(42);
    assert!(matches!(result, Err(StoreError::EntryNotFound(42))));
}

#[test]
fn test_update() {
    let db = TestDb::new();
    let entry = TestEntry::new("old-topic", "old-cat")
        .with_tags(&["tag1"])
        .build();
    let id = db.store().insert(entry).unwrap();

    let mut record = db.store().get(id).unwrap();
    record.topic = "new-topic".to_string();
    record.category = "new-cat".to_string();
    record.tags = vec!["tag2".to_string(), "tag3".to_string()];
    db.store().update(record).unwrap();

    let record = db.store().get(id).unwrap();
    assert_eq!(record.topic, "new-topic");
    assert_eq!(record.category, "new-cat");
    assert_eq!(record.tags, vec!["tag2".to_string(), "tag3".to_string()]);
    assert_index_consistent(db.store(), id);
}

#[test]
fn test_update_not_found() {
    let db = TestDb::new();
    let entry = TestEntry::new("a", "b").build();
    // Create a fake EntryRecord with nonexistent id
    let fake_record = unimatrix_store::EntryRecord {
        id: 999,
        title: entry.title,
        content: entry.content,
        topic: entry.topic,
        category: entry.category,
        tags: entry.tags,
        source: entry.source,
        status: entry.status,
        confidence: 0.0,
        created_at: 0,
        updated_at: 0,
        last_accessed_at: 0,
        access_count: 0,
        supersedes: None,
        superseded_by: None,
        correction_count: 0,
        embedding_dim: 0,
        created_by: String::new(),
        modified_by: String::new(),
        content_hash: String::new(),
        previous_hash: String::new(),
        version: 1,
        feature_cycle: String::new(),
        trust_source: String::new(),
        helpful_count: 0,
        unhelpful_count: 0,
    };
    let result = db.store().update(fake_record);
    assert!(matches!(result, Err(StoreError::EntryNotFound(999))));
}

#[test]
fn test_update_status() {
    let db = TestDb::new();
    let entry = TestEntry::new("topic", "cat").build();
    let id = db.store().insert(entry).unwrap();

    db.store().update_status(id, Status::Deprecated).unwrap();
    let record = db.store().get(id).unwrap();
    assert_eq!(record.status, Status::Deprecated);
    assert_index_consistent(db.store(), id);
}

#[test]
fn test_delete() {
    let db = TestDb::new();
    let entry = TestEntry::new("topic", "cat").build();
    let id = db.store().insert(entry).unwrap();
    db.store().delete(id).unwrap();
    assert!(!db.store().exists(id).unwrap());
}

#[test]
fn test_delete_not_found() {
    let db = TestDb::new();
    let result = db.store().delete(999);
    assert!(matches!(result, Err(StoreError::EntryNotFound(999))));
}

// === QUERY OPERATIONS ===

#[test]
fn test_query_by_topic() {
    let db = TestDb::new();
    let id1 = db.store().insert(TestEntry::new("auth", "conv").build()).unwrap();
    let _id2 = db.store().insert(TestEntry::new("logging", "conv").build()).unwrap();
    let id3 = db.store().insert(TestEntry::new("auth", "pattern").build()).unwrap();

    let results = db.store().query_by_topic("auth").unwrap();
    let ids: Vec<u64> = results.iter().map(|r| r.id).collect();
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id3));
    assert_eq!(ids.len(), 2);
}

#[test]
fn test_query_by_category() {
    let db = TestDb::new();
    let id1 = db.store().insert(TestEntry::new("a", "convention").build()).unwrap();
    let _id2 = db.store().insert(TestEntry::new("b", "decision").build()).unwrap();

    let results = db.store().query_by_category("convention").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, id1);
}

#[test]
fn test_query_by_tags() {
    let db = TestDb::new();
    let id1 = db.store().insert(
        TestEntry::new("a", "b").with_tags(&["rust", "async"]).build()
    ).unwrap();
    let _id2 = db.store().insert(
        TestEntry::new("c", "d").with_tags(&["rust"]).build()
    ).unwrap();

    // Both have "rust"
    let results = db.store().query_by_tags(&["rust".to_string()]).unwrap();
    assert_eq!(results.len(), 2);

    // Only id1 has both "rust" AND "async"
    let results = db.store().query_by_tags(&["rust".to_string(), "async".to_string()]).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, id1);
}

#[test]
fn test_query_by_status() {
    let db = TestDb::new();
    let id1 = db.store().insert(TestEntry::new("a", "b").build()).unwrap();
    let id2 = db.store().insert(TestEntry::new("c", "d").build()).unwrap();
    db.store().update_status(id2, Status::Deprecated).unwrap();

    let active = db.store().query_by_status(Status::Active).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, id1);

    let deprecated = db.store().query_by_status(Status::Deprecated).unwrap();
    assert_eq!(deprecated.len(), 1);
    assert_eq!(deprecated[0].id, id2);
}

#[test]
fn test_query_by_time_range() {
    let db = TestDb::new();
    let _id = db.store().insert(TestEntry::new("a", "b").build()).unwrap();
    let now = now_secs();

    // Wide range should find it
    let results = db.store().query_by_time_range(TimeRange {
        start: now - 10,
        end: now + 10,
    }).unwrap();
    assert!(!results.is_empty());

    // Future range should not
    let results = db.store().query_by_time_range(TimeRange {
        start: now + 1000,
        end: now + 2000,
    }).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_combined_query() {
    let db = TestDb::new();
    let id1 = db.store().insert(
        TestEntry::new("auth", "convention").with_tags(&["rust"]).build()
    ).unwrap();
    let _id2 = db.store().insert(
        TestEntry::new("auth", "decision").with_tags(&["rust"]).build()
    ).unwrap();
    let _id3 = db.store().insert(
        TestEntry::new("logging", "convention").with_tags(&["rust"]).build()
    ).unwrap();

    let filter = QueryFilter {
        topic: Some("auth".to_string()),
        category: Some("convention".to_string()),
        tags: None,
        status: Some(Status::Active),
        time_range: None,
    };
    let results = db.store().query(filter).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, id1);
}

// === USAGE AND CONFIDENCE ===

#[test]
fn test_record_usage() {
    let db = TestDb::new();
    let id = db.store().insert(TestEntry::new("a", "b").build()).unwrap();

    // Record usage with helpful vote
    db.store().record_usage(&[id], &[id], &[id], &[], &[], &[]).unwrap();
    let record = db.store().get(id).unwrap();
    assert_eq!(record.access_count, 1);
    assert_eq!(record.helpful_count, 1);
    assert!(record.last_accessed_at > 0);

    // Record usage with unhelpful vote
    db.store().record_usage(&[id], &[id], &[], &[id], &[], &[]).unwrap();
    let record = db.store().get(id).unwrap();
    assert_eq!(record.access_count, 2);
    assert_eq!(record.unhelpful_count, 1);
}

#[test]
fn test_record_usage_with_confidence() {
    let db = TestDb::new();
    let id = db.store().insert(TestEntry::new("a", "b").build()).unwrap();

    // Use confidence function that returns a fixed value
    db.store().record_usage_with_confidence(
        &[id], &[id], &[id], &[], &[], &[],
        Some(&|_record, _now| 0.85),
    ).unwrap();
    let record = db.store().get(id).unwrap();
    assert_eq!(record.confidence, 0.85);
    assert_eq!(record.helpful_count, 1);
}

#[test]
fn test_update_confidence() {
    let db = TestDb::new();
    let id = db.store().insert(TestEntry::new("a", "b").build()).unwrap();

    db.store().update_confidence(id, 0.92).unwrap();
    let record = db.store().get(id).unwrap();
    assert_eq!(record.confidence, 0.92);
}

// === VECTOR MAP ===

#[test]
fn test_vector_mapping() {
    let db = TestDb::new();
    db.store().put_vector_mapping(1, 100).unwrap();
    db.store().put_vector_mapping(2, 200).unwrap();

    assert_eq!(db.store().get_vector_mapping(1).unwrap(), Some(100));
    assert_eq!(db.store().get_vector_mapping(2).unwrap(), Some(200));
    assert_eq!(db.store().get_vector_mapping(3).unwrap(), None);

    let mappings = db.store().iter_vector_mappings().unwrap();
    assert_eq!(mappings.len(), 2);
}

#[test]
fn test_rewrite_vector_map() {
    let db = TestDb::new();
    db.store().put_vector_mapping(1, 100).unwrap();
    db.store().put_vector_mapping(2, 200).unwrap();

    db.store().rewrite_vector_map(&[(10, 1000), (20, 2000)]).unwrap();
    assert_eq!(db.store().get_vector_mapping(1).unwrap(), None);
    assert_eq!(db.store().get_vector_mapping(10).unwrap(), Some(1000));
    assert_eq!(db.store().get_vector_mapping(20).unwrap(), Some(2000));
}

// === FEATURE ENTRIES ===

#[test]
fn test_record_feature_entries() {
    let db = TestDb::new();
    let id1 = db.store().insert(TestEntry::new("a", "b").build()).unwrap();
    let id2 = db.store().insert(TestEntry::new("c", "d").build()).unwrap();
    db.store().record_feature_entries("col-001", &[id1, id2]).unwrap();
    // No panic = success (feature_entries is write-only from Store API)
}

// === CO-ACCESS ===

#[test]
fn test_co_access_roundtrip() {
    let db = TestDb::new();
    let now = now_secs();

    db.store().record_co_access_pairs(&[(1, 2), (1, 3)]).unwrap();

    let partners = db.store().get_co_access_partners(1, 0).unwrap();
    assert_eq!(partners.len(), 2);

    let (total, active) = db.store().co_access_stats(0).unwrap();
    assert_eq!(total, 2);
    assert_eq!(active, 2);
}

#[test]
fn test_co_access_increment() {
    let db = TestDb::new();
    let now = now_secs();

    db.store().record_co_access_pairs(&[(1, 2)]).unwrap();
    db.store().record_co_access_pairs(&[(1, 2)]).unwrap();

    let partners = db.store().get_co_access_partners(1, 0).unwrap();
    assert_eq!(partners.len(), 1);
    assert_eq!(partners[0].1.count, 2);
}

#[test]
fn test_co_access_self_pair_skipped() {
    let db = TestDb::new();
    db.store().record_co_access_pairs(&[(1, 1)]).unwrap();
    let (total, _) = db.store().co_access_stats(0).unwrap();
    assert_eq!(total, 0);
}

#[test]
fn test_cleanup_stale_co_access() {
    let db = TestDb::new();

    db.store().record_co_access_pairs(&[(1, 2)]).unwrap();
    db.store().record_co_access_pairs(&[(3, 4)]).unwrap();

    // Cleanup with a future cutoff removes all pairs
    let future = now_secs() + 1000;
    let deleted = db.store().cleanup_stale_co_access(future).unwrap();
    assert_eq!(deleted, 2);

    let (total, _) = db.store().co_access_stats(0).unwrap();
    assert_eq!(total, 0);
}

#[test]
fn test_top_co_access_pairs() {
    let db = TestDb::new();
    let now = now_secs();

    db.store().record_co_access_pairs(&[(1, 2)]).unwrap();
    db.store().record_co_access_pairs(&[(1, 2)]).unwrap(); // count=2
    db.store().record_co_access_pairs(&[(3, 4)]).unwrap(); // count=1

    let top = db.store().top_co_access_pairs(10, 0).unwrap();
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].1.count, 2); // highest count first
}

// === OBSERVATION METRICS ===

#[test]
fn test_store_and_get_metrics() {
    let db = TestDb::new();
    let data = b"metric data here";
    db.store().store_metrics("col-001", data).unwrap();

    let got = db.store().get_metrics("col-001").unwrap().unwrap();
    assert_eq!(got, data);

    assert!(db.store().get_metrics("nonexistent").unwrap().is_none());
}

#[test]
fn test_list_all_metrics() {
    let db = TestDb::new();
    db.store().store_metrics("a", b"data-a").unwrap();
    db.store().store_metrics("b", b"data-b").unwrap();

    let all = db.store().list_all_metrics().unwrap();
    assert_eq!(all.len(), 2);
}

// === COUNTERS ===

#[test]
fn test_read_counter() {
    let db = TestDb::new();
    // schema_version should be 5 after creation
    let version = db.store().read_counter("schema_version").unwrap();
    assert_eq!(version, 5);

    // next_entry_id should be 1 initially
    let next = db.store().read_counter("next_entry_id").unwrap();
    assert_eq!(next, 1);

    // nonexistent returns 0
    let missing = db.store().read_counter("nonexistent").unwrap();
    assert_eq!(missing, 0);
}

