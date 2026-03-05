//! Store integration tests: signals, sessions, injection log, persistence.
//!
//! Companion to sqlite_parity.rs. Covers specialized operations.

#![cfg(feature = "test-support")]

use unimatrix_store::Store;
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

#[test]
fn test_wal_mode_creates_wal_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test.db");
    let store = Store::open(&path).unwrap();
    store.insert(TestEntry::new("wal", "test").build()).unwrap();
    let wal_path = dir.path().join("test.db-wal");
    assert!(wal_path.exists(), "WAL file should exist after write");
}
