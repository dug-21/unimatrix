//! Store integration tests: signals, sessions, injection log, persistence.
//!
//! Companion to sqlite_parity.rs. Covers specialized operations.

#![cfg(feature = "test-support")]

use unimatrix_store::test_helpers::{
    TestEntry, assert_index_consistent, open_test_store, seed_entries,
};
use unimatrix_store::{
    DELETE_THRESHOLD_SECS, InjectionLogRecord, SessionLifecycleStatus, SessionRecord, SignalRecord,
    SignalSource, SignalType, SqlxStore, TIMED_OUT_THRESHOLD_SECS,
};

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Flush the analytics queue by closing and re-opening the store.
async fn flush(store: SqlxStore, dir: &tempfile::TempDir) -> SqlxStore {
    store.close().await.expect("close");
    open_test_store(dir).await
}

// === SIGNAL QUEUE ===

#[tokio::test]
async fn test_signal_insert_and_drain() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let record = SignalRecord {
        signal_id: 0,
        session_id: "sess-1".to_string(),
        created_at: now_secs(),
        entry_ids: vec![1, 2, 3],
        signal_type: SignalType::Helpful,
        signal_source: SignalSource::ImplicitOutcome,
    };

    store.insert_signal(&record).await.unwrap();
    let store = flush(store, &dir).await;

    let len = store.signal_queue_len().await.unwrap();
    assert_eq!(len, 1);

    let drained = store.drain_signals(SignalType::Helpful).await.unwrap();
    assert_eq!(drained.len(), 1);
    assert_eq!(drained[0].session_id, "sess-1");

    let len = store.signal_queue_len().await.unwrap();
    assert_eq!(len, 0);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_signal_drain_filters_by_type() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

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

    store.insert_signal(&helpful).await.unwrap();
    store.insert_signal(&flagged).await.unwrap();
    let store = flush(store, &dir).await;

    let drained = store.drain_signals(SignalType::Helpful).await.unwrap();
    assert_eq!(drained.len(), 1);

    // Flagged still in queue
    assert_eq!(store.signal_queue_len().await.unwrap(), 1);

    store.close().await.unwrap();
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
        keywords: None,
    }
}

#[tokio::test]
async fn test_session_insert_and_get() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let now = now_secs();
    let record = make_session("sess-1", SessionLifecycleStatus::Active, now);
    store.insert_session(&record).await.unwrap();
    let store = flush(store, &dir).await;

    let got = store.get_session("sess-1").await.unwrap().unwrap();
    assert_eq!(got.session_id, "sess-1");
    assert_eq!(got.status, SessionLifecycleStatus::Active);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_session_get_missing() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    assert!(store.get_session("nonexistent").await.unwrap().is_none());

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_session_update() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let now = now_secs();
    let record = make_session("upd", SessionLifecycleStatus::Active, now);
    store.insert_session(&record).await.unwrap();
    let store = flush(store, &dir).await;

    store
        .update_session("upd", |r| {
            r.status = SessionLifecycleStatus::Completed;
            r.ended_at = Some(now + 100);
            r.outcome = Some("success".to_string());
        })
        .await
        .unwrap();

    let got = store.get_session("upd").await.unwrap().unwrap();
    assert_eq!(got.status, SessionLifecycleStatus::Completed);
    assert_eq!(got.ended_at, Some(now + 100));

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_session_update_not_found() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let result = store.update_session("ghost", |_| {}).await;
    assert!(result.is_err());

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_scan_sessions_by_feature() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let now = now_secs();
    let mut s1 = make_session("a1", SessionLifecycleStatus::Active, now);
    s1.feature_cycle = Some("fc-a".to_string());
    let mut s2 = make_session("a2", SessionLifecycleStatus::Completed, now);
    s2.feature_cycle = Some("fc-a".to_string());
    let mut s3 = make_session("b1", SessionLifecycleStatus::Active, now);
    s3.feature_cycle = Some("fc-b".to_string());

    store.insert_session(&s1).await.unwrap();
    store.insert_session(&s2).await.unwrap();
    store.insert_session(&s3).await.unwrap();
    let store = flush(store, &dir).await;

    assert_eq!(
        store.scan_sessions_by_feature("fc-a").await.unwrap().len(),
        2
    );
    assert_eq!(
        store.scan_sessions_by_feature("fc-b").await.unwrap().len(),
        1
    );
    assert_eq!(
        store.scan_sessions_by_feature("fc-c").await.unwrap().len(),
        0
    );

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_scan_sessions_with_status_filter() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let now = now_secs();
    let mut s1 = make_session("s1", SessionLifecycleStatus::Completed, now);
    s1.feature_cycle = Some("fc".to_string());
    let mut s2 = make_session("s2", SessionLifecycleStatus::Abandoned, now);
    s2.feature_cycle = Some("fc".to_string());

    store.insert_session(&s1).await.unwrap();
    store.insert_session(&s2).await.unwrap();
    let store = flush(store, &dir).await;

    let completed = store
        .scan_sessions_by_feature_with_status("fc", Some(SessionLifecycleStatus::Completed))
        .await
        .unwrap();
    assert_eq!(completed.len(), 1);

    let all = store
        .scan_sessions_by_feature_with_status("fc", None)
        .await
        .unwrap();
    assert_eq!(all.len(), 2);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_gc_sessions_timeout() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let now = now_secs();
    let old = now.saturating_sub(25 * 3600 + 60);
    let session = make_session("gc-to", SessionLifecycleStatus::Active, old);
    store.insert_session(&session).await.unwrap();
    let store = flush(store, &dir).await;

    let stats = store
        .gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS)
        .await
        .unwrap();
    assert_eq!(stats.timed_out_count, 1);

    let got = store.get_session("gc-to").await.unwrap().unwrap();
    assert_eq!(got.status, SessionLifecycleStatus::TimedOut);

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_gc_sessions_delete_with_cascade() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let now = now_secs();
    let very_old = now.saturating_sub(31 * 24 * 3600 + 60);
    let session = make_session("gc-del", SessionLifecycleStatus::Completed, very_old);
    store.insert_session(&session).await.unwrap();

    let logs = vec![
        InjectionLogRecord {
            log_id: 0,
            session_id: "gc-del".to_string(),
            entry_id: 1,
            confidence: 0.8,
            timestamp: very_old,
        },
        InjectionLogRecord {
            log_id: 0,
            session_id: "gc-del".to_string(),
            entry_id: 2,
            confidence: 0.9,
            timestamp: very_old,
        },
    ];
    store.insert_injection_log_batch(&logs);
    let store = flush(store, &dir).await;

    let stats = store
        .gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS)
        .await
        .unwrap();
    assert_eq!(stats.deleted_session_count, 1);
    assert_eq!(stats.deleted_injection_log_count, 2);

    assert!(store.get_session("gc-del").await.unwrap().is_none());
    assert!(
        store
            .scan_injection_log_by_session("gc-del")
            .await
            .unwrap()
            .is_empty()
    );

    store.close().await.unwrap();
}

// === INJECTION LOG ===

#[tokio::test]
async fn test_injection_log_batch() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let records = vec![
        InjectionLogRecord {
            log_id: 0,
            session_id: "sess".to_string(),
            entry_id: 1,
            confidence: 0.8,
            timestamp: 1000,
        },
        InjectionLogRecord {
            log_id: 0,
            session_id: "sess".to_string(),
            entry_id: 2,
            confidence: 0.9,
            timestamp: 1000,
        },
        InjectionLogRecord {
            log_id: 0,
            session_id: "sess".to_string(),
            entry_id: 3,
            confidence: 0.7,
            timestamp: 1000,
        },
    ];
    store.insert_injection_log_batch(&records);
    let store = flush(store, &dir).await;

    let got = store.scan_injection_log_by_session("sess").await.unwrap();
    assert_eq!(got.len(), 3);

    let mut ids: Vec<u64> = got.iter().map(|r| r.log_id).collect();
    ids.sort();
    // IDs are AUTOINCREMENT, so they should be 1, 2, 3 (not 0, 1, 2)
    assert_eq!(ids.len(), 3);
    // All IDs must be distinct and non-zero
    assert!(ids.iter().all(|&id| id > 0));
    assert!(ids.windows(2).all(|w| w[0] < w[1]));

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_injection_log_empty_batch() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    store.insert_injection_log_batch(&[]);
    // No panic, no records

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_injection_log_session_isolation() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let batch_a = vec![InjectionLogRecord {
        log_id: 0,
        session_id: "A".to_string(),
        entry_id: 1,
        confidence: 0.8,
        timestamp: 1000,
    }];
    let batch_b = vec![InjectionLogRecord {
        log_id: 0,
        session_id: "B".to_string(),
        entry_id: 2,
        confidence: 0.9,
        timestamp: 1000,
    }];
    store.insert_injection_log_batch(&batch_a);
    store.insert_injection_log_batch(&batch_b);
    let store = flush(store, &dir).await;

    assert_eq!(
        store
            .scan_injection_log_by_session("A")
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        store
            .scan_injection_log_by_session("B")
            .await
            .unwrap()
            .len(),
        1
    );

    store.close().await.unwrap();
}

// === INDEX CONSISTENCY (using test helpers) ===

#[tokio::test]
async fn test_seed_and_index_consistency() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let ids = seed_entries(&store, 10).await;
    assert_eq!(ids.len(), 10);
    for id in &ids {
        assert_index_consistent(&store, *id).await;
    }

    store.close().await.unwrap();
}

// === STORE REOPEN ===

#[tokio::test]
async fn test_store_reopen_persistence() {
    let dir = tempfile::TempDir::new().unwrap();

    {
        let store = open_test_store(&dir).await;
        let entry = TestEntry::new("persist", "test").build();
        store.insert(entry).await.unwrap();
        store.close().await.unwrap();
    }

    {
        let store = open_test_store(&dir).await;
        let record = store.get(1).await.unwrap();
        assert_eq!(record.topic, "persist");
        store.close().await.unwrap();
    }
}

// === WAL MODE VERIFICATION (R-07) ===

#[tokio::test]
async fn test_wal_mode_creates_wal_file() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    store
        .insert(TestEntry::new("wal", "test").build())
        .await
        .unwrap();
    store.close().await.unwrap();
    let wal_path = dir.path().join("test.db-wal");
    assert!(wal_path.exists(), "WAL file should exist after write");
}
