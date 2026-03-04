//! Parity tests for the SQLite backend.
//!
//! These tests exercise all Store operations to verify behavioral parity
//! with the redb backend. They run ONLY under `--features backend-sqlite`.
//!
//! Coverage: insert, update, update_status, delete, record_usage,
//! record_usage_with_confidence, update_confidence, put_vector_mapping,
//! rewrite_vector_map, record_feature_entries, record_co_access_pairs,
//! cleanup_stale_co_access, store_metrics, get, exists, query_by_*,
//! query (combined), get_vector_mapping, iter_vector_mappings,
//! read_counter, get_co_access_partners, co_access_stats,
//! top_co_access_pairs, get_metrics, list_all_metrics,
//! signal queue ops, session ops, injection log ops.

#![cfg(feature = "backend-sqlite")]

use unimatrix_store::{
    QueryFilter, Status, Store, StoreError, TimeRange,
};
use unimatrix_store::test_helpers::{TestDb, TestEntry, assert_index_consistent, seed_entries};
use unimatrix_store::{
    InjectionLogRecord, SessionLifecycleStatus, SessionRecord,
    SignalRecord, SignalSource, SignalType,
    TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS,
};

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

    let updated = TestEntry::new("new-topic", "new-cat")
        .with_tags(&["tag2", "tag3"])
        .build();
    db.store().update(id, updated).unwrap();

    let record = db.store().get(id).unwrap();
    assert_eq!(record.topic, "new-topic");
    assert_eq!(record.category, "new-cat");
    assert_eq!(record.tags, vec!["tag2".to_string(), "tag3".to_string()]);
    assert_eq!(record.version, 2);
    assert_index_consistent(db.store(), id);
}

#[test]
fn test_update_not_found() {
    let db = TestDb::new();
    let result = db.store().update(999, TestEntry::new("a", "b").build());
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
    let now = now_secs();

    db.store().record_usage(id, true, now).unwrap();
    let record = db.store().get(id).unwrap();
    assert_eq!(record.access_count, 1);
    assert_eq!(record.helpful_count, 1);
    assert_eq!(record.last_accessed_at, now);

    db.store().record_usage(id, false, now + 1).unwrap();
    let record = db.store().get(id).unwrap();
    assert_eq!(record.access_count, 2);
    assert_eq!(record.unhelpful_count, 1);
}

#[test]
fn test_record_usage_with_confidence() {
    let db = TestDb::new();
    let id = db.store().insert(TestEntry::new("a", "b").build()).unwrap();
    let now = now_secs();

    db.store().record_usage_with_confidence(id, true, 0.85, now).unwrap();
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

    db.store().record_co_access_pairs(&[(1, 2), (1, 3)], now).unwrap();

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

    db.store().record_co_access_pairs(&[(1, 2)], now).unwrap();
    db.store().record_co_access_pairs(&[(1, 2)], now + 1).unwrap();

    let partners = db.store().get_co_access_partners(1, 0).unwrap();
    assert_eq!(partners.len(), 1);
    assert_eq!(partners[0].1.count, 2);
}

#[test]
fn test_co_access_self_pair_skipped() {
    let db = TestDb::new();
    db.store().record_co_access_pairs(&[(1, 1)], now_secs()).unwrap();
    let (total, _) = db.store().co_access_stats(0).unwrap();
    assert_eq!(total, 0);
}

#[test]
fn test_cleanup_stale_co_access() {
    let db = TestDb::new();
    let now = now_secs();

    db.store().record_co_access_pairs(&[(1, 2)], now - 1000).unwrap();
    db.store().record_co_access_pairs(&[(3, 4)], now).unwrap();

    let deleted = db.store().cleanup_stale_co_access(now - 500).unwrap();
    assert_eq!(deleted, 1);

    let (total, _) = db.store().co_access_stats(0).unwrap();
    assert_eq!(total, 1);
}

#[test]
fn test_top_co_access_pairs() {
    let db = TestDb::new();
    let now = now_secs();

    db.store().record_co_access_pairs(&[(1, 2)], now).unwrap();
    db.store().record_co_access_pairs(&[(1, 2)], now).unwrap(); // count=2
    db.store().record_co_access_pairs(&[(3, 4)], now).unwrap(); // count=1

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

// === SIGNAL QUEUE ===

#[test]
fn test_signal_insert_and_drain() {
    let db = TestDb::new();
    let record = SignalRecord {
        signal_id: 0,
        session_id: "sess-1".to_string(),
        created_at: now_secs(),
        entry_ids: vec![1, 2, 3],
        signal_type: SignalType::Helpful,
        signal_source: SignalSource::ImplicitOutcome,
    };

    let id = db.store().insert_signal(&record).unwrap();
    assert_eq!(id, 0);

    let len = db.store().signal_queue_len().unwrap();
    assert_eq!(len, 1);

    let drained = db.store().drain_signals(SignalType::Helpful).unwrap();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].session_id, "sess-1");

    let len = db.store().signal_queue_len().unwrap();
    assert_eq!(len, 0);
}

#[test]
fn test_signal_drain_filters_by_type() {
    let db = TestDb::new();
    let helpful = SignalRecord {
        signal_id: 0,
        session_id: "s1".to_string(),
        created_at: 1,
        entry_ids: vec![1],
        signal_type: SignalType::Helpful,
        signal_source: SignalSource::ImplicitOutcome,
    };
    let flagged = SignalRecord {
        signal_id: 0,
        session_id: "s2".to_string(),
        created_at: 2,
        entry_ids: vec![2],
        signal_type: SignalType::Flagged,
        signal_source: SignalSource::ImplicitRework,
    };

    db.store().insert_signal(&helpful).unwrap();
    db.store().insert_signal(&flagged).unwrap();

    let drained = db.store().drain_signals(SignalType::Helpful).unwrap();
    assert_eq!(drained.len(), 1);

    // Flagged still in queue
    assert_eq!(db.store().signal_queue_len().unwrap(), 1);
}

// === SESSIONS ===

fn make_session(id: &str, status: SessionLifecycleStatus, started_at: u64) -> SessionRecord {
    SessionRecord {
        session_id: id.to_string(),
        feature_cycle: Some("fc-test".to_string()),
        agent_role: Some("dev".to_string()),
        started_at,
        ended_at: None,
        status,
        compaction_count: 0,
        outcome: None,
        total_injections: 0,
    }
}

#[test]
fn test_session_insert_and_get() {
    let db = TestDb::new();
    let now = now_secs();
    let record = make_session("sess-1", SessionLifecycleStatus::Active, now);
    db.store().insert_session(&record).unwrap();

    let got = db.store().get_session("sess-1").unwrap().unwrap();
    assert_eq!(got.session_id, "sess-1");
    assert_eq!(got.status, SessionLifecycleStatus::Active);
}

#[test]
fn test_session_get_missing() {
    let db = TestDb::new();
    assert!(db.store().get_session("nonexistent").unwrap().is_none());
}

#[test]
fn test_session_update() {
    let db = TestDb::new();
    let now = now_secs();
    let record = make_session("upd", SessionLifecycleStatus::Active, now);
    db.store().insert_session(&record).unwrap();

    db.store().update_session("upd", |r| {
        r.status = SessionLifecycleStatus::Completed;
        r.ended_at = Some(now + 100);
        r.outcome = Some("success".to_string());
    }).unwrap();

    let got = db.store().get_session("upd").unwrap().unwrap();
    assert_eq!(got.status, SessionLifecycleStatus::Completed);
    assert_eq!(got.ended_at, Some(now + 100));
}

#[test]
fn test_session_update_not_found() {
    let db = TestDb::new();
    let result = db.store().update_session("ghost", |_| {});
    assert!(result.is_err());
}

#[test]
fn test_scan_sessions_by_feature() {
    let db = TestDb::new();
    let now = now_secs();

    let mut s1 = make_session("a1", SessionLifecycleStatus::Active, now);
    s1.feature_cycle = Some("fc-a".to_string());
    let mut s2 = make_session("a2", SessionLifecycleStatus::Completed, now);
    s2.feature_cycle = Some("fc-a".to_string());
    let mut s3 = make_session("b1", SessionLifecycleStatus::Active, now);
    s3.feature_cycle = Some("fc-b".to_string());

    db.store().insert_session(&s1).unwrap();
    db.store().insert_session(&s2).unwrap();
    db.store().insert_session(&s3).unwrap();

    assert_eq!(db.store().scan_sessions_by_feature("fc-a").unwrap().len(), 2);
    assert_eq!(db.store().scan_sessions_by_feature("fc-b").unwrap().len(), 1);
    assert_eq!(db.store().scan_sessions_by_feature("fc-c").unwrap().len(), 0);
}

#[test]
fn test_scan_sessions_with_status_filter() {
    let db = TestDb::new();
    let now = now_secs();

    let mut s1 = make_session("s1", SessionLifecycleStatus::Completed, now);
    s1.feature_cycle = Some("fc".to_string());
    let mut s2 = make_session("s2", SessionLifecycleStatus::Abandoned, now);
    s2.feature_cycle = Some("fc".to_string());

    db.store().insert_session(&s1).unwrap();
    db.store().insert_session(&s2).unwrap();

    let completed = db.store().scan_sessions_by_feature_with_status(
        "fc", Some(SessionLifecycleStatus::Completed)
    ).unwrap();
    assert_eq!(completed.len(), 1);
    let all = db.store().scan_sessions_by_feature_with_status("fc", None).unwrap();
    assert_eq!(all.len(), 2);
}

#[test]
fn test_gc_sessions_timeout() {
    let db = TestDb::new();
    let now = now_secs();
    let old = now.saturating_sub(25 * 3600 + 60);
    let session = make_session("gc-to", SessionLifecycleStatus::Active, old);
    db.store().insert_session(&session).unwrap();

    let stats = db.store().gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS).unwrap();
    assert_eq!(stats.timed_out_count, 1);

    let got = db.store().get_session("gc-to").unwrap().unwrap();
    assert_eq!(got.status, SessionLifecycleStatus::TimedOut);
}

#[test]
fn test_gc_sessions_delete_with_cascade() {
    let db = TestDb::new();
    let now = now_secs();
    let very_old = now.saturating_sub(31 * 24 * 3600 + 60);
    let session = make_session("gc-del", SessionLifecycleStatus::Completed, very_old);
    db.store().insert_session(&session).unwrap();

    let logs = vec![
        InjectionLogRecord { log_id: 0, session_id: "gc-del".to_string(), entry_id: 1, confidence: 0.8, timestamp: very_old },
        InjectionLogRecord { log_id: 0, session_id: "gc-del".to_string(), entry_id: 2, confidence: 0.9, timestamp: very_old },
    ];
    db.store().insert_injection_log_batch(&logs).unwrap();

    let stats = db.store().gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS).unwrap();
    assert_eq!(stats.deleted_session_count, 1);
    assert_eq!(stats.deleted_injection_log_count, 2);

    assert!(db.store().get_session("gc-del").unwrap().is_none());
    assert!(db.store().scan_injection_log_by_session("gc-del").unwrap().is_empty());
}

// === INJECTION LOG ===

#[test]
fn test_injection_log_batch() {
    let db = TestDb::new();
    let records = vec![
        InjectionLogRecord { log_id: 0, session_id: "sess".to_string(), entry_id: 1, confidence: 0.8, timestamp: 1000 },
        InjectionLogRecord { log_id: 0, session_id: "sess".to_string(), entry_id: 2, confidence: 0.9, timestamp: 1000 },
        InjectionLogRecord { log_id: 0, session_id: "sess".to_string(), entry_id: 3, confidence: 0.7, timestamp: 1000 },
    ];
    db.store().insert_injection_log_batch(&records).unwrap();

    let got = db.store().scan_injection_log_by_session("sess").unwrap();
    assert_eq!(got.len(), 3);

    let mut ids: Vec<u64> = got.iter().map(|r| r.log_id).collect();
    ids.sort();
    assert_eq!(ids, vec![0, 1, 2]);
}

#[test]
fn test_injection_log_empty_batch() {
    let db = TestDb::new();
    db.store().insert_injection_log_batch(&[]).unwrap();
    // No panic, no records
}

#[test]
fn test_injection_log_session_isolation() {
    let db = TestDb::new();
    let batch_a = vec![
        InjectionLogRecord { log_id: 0, session_id: "A".to_string(), entry_id: 1, confidence: 0.8, timestamp: 1000 },
    ];
    let batch_b = vec![
        InjectionLogRecord { log_id: 0, session_id: "B".to_string(), entry_id: 2, confidence: 0.9, timestamp: 1000 },
    ];
    db.store().insert_injection_log_batch(&batch_a).unwrap();
    db.store().insert_injection_log_batch(&batch_b).unwrap();

    assert_eq!(db.store().scan_injection_log_by_session("A").unwrap().len(), 1);
    assert_eq!(db.store().scan_injection_log_by_session("B").unwrap().len(), 1);
}

// === INDEX CONSISTENCY (using test helpers) ===

#[test]
fn test_seed_and_index_consistency() {
    let db = TestDb::new();
    let ids = seed_entries(db.store(), 10);
    assert_eq!(ids.len(), 10);
    for id in &ids {
        assert_index_consistent(db.store(), *id);
    }
}

// === STORE REOPEN ===

#[test]
fn test_store_reopen_persistence() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test.db");

    {
        let store = Store::open(&path).unwrap();
        let entry = TestEntry::new("persist", "test").build();
        store.insert(entry).unwrap();
    }

    {
        let store = Store::open(&path).unwrap();
        let record = store.get(1).unwrap();
        assert_eq!(record.topic, "persist");
    }
}

// === COMPACT (no-op) ===

#[test]
fn test_compact_is_noop() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test.db");
    let mut store = Store::open(&path).unwrap();
    store.compact().unwrap(); // Should not error
}

// === WAL MODE VERIFICATION (R-07) ===
// WAL mode is verified via the -wal file created on disk after first write.

#[test]
fn test_wal_mode_creates_wal_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test.db");
    let store = Store::open(&path).unwrap();
    // Perform a write to trigger WAL file creation
    store.insert(TestEntry::new("wal", "test").build()).unwrap();
    let wal_path = dir.path().join("test.db-wal");
    assert!(wal_path.exists(), "WAL file should exist after write");
}
