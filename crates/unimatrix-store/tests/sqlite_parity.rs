//! Store integration tests.
//!
//! These tests exercise all Store operations.
//!
//! Coverage: CRUD, queries, usage tracking, confidence, vector map,
//! feature entries, co-access, metrics, counters.
//! See sqlite_parity_specialized.rs for signals, sessions, injection log.

#![cfg(feature = "test-support")]

use unimatrix_store::test_helpers::{TestEntry, assert_index_consistent, open_test_store};
use unimatrix_store::{QueryFilter, Status, StoreError, TimeRange};

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Flush the analytics queue by closing and re-opening the store.
async fn flush(
    store: unimatrix_store::SqlxStore,
    dir: &tempfile::TempDir,
) -> unimatrix_store::SqlxStore {
    store.close().await.expect("close");
    open_test_store(dir).await
}

// === BASIC CRUD ===

#[tokio::test]
async fn test_insert_and_get() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let entry = TestEntry::new("auth", "convention").build();
    let id = store.insert(entry).await.unwrap();
    assert!(id >= 1);

    let record = store.get(id).await.unwrap();
    assert_eq!(record.id, id);
    assert_eq!(record.topic, "auth");
    assert_eq!(record.category, "convention");
    assert_eq!(record.status, Status::Active);
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_exists() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let entry = TestEntry::new("test", "pattern").build();
    let id = store.insert(entry).await.unwrap();
    assert!(store.exists(id).await.unwrap());
    assert!(!store.exists(99999).await.unwrap());
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_insert_multiple_sequential_ids() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id1 = store
        .insert(TestEntry::new("a", "b").build())
        .await
        .unwrap();
    let id2 = store
        .insert(TestEntry::new("c", "d").build())
        .await
        .unwrap();
    let id3 = store
        .insert(TestEntry::new("e", "f").build())
        .await
        .unwrap();
    assert_eq!(id2, id1 + 1);
    assert_eq!(id3, id2 + 1);
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_get_not_found() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let result = store.get(42).await;
    assert!(matches!(result, Err(StoreError::EntryNotFound(42))));
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_update() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let entry = TestEntry::new("old-topic", "old-cat")
        .with_tags(&["tag1"])
        .build();
    let id = store.insert(entry).await.unwrap();

    let mut record = store.get(id).await.unwrap();
    record.topic = "new-topic".to_string();
    record.category = "new-cat".to_string();
    record.tags = vec!["tag2".to_string(), "tag3".to_string()];
    store.update(record).await.unwrap();

    let record = store.get(id).await.unwrap();
    assert_eq!(record.topic, "new-topic");
    assert_eq!(record.category, "new-cat");
    assert_eq!(record.tags, vec!["tag2".to_string(), "tag3".to_string()]);
    assert_index_consistent(&store, id).await;
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_update_not_found() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let entry = TestEntry::new("a", "b").build();
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
        pre_quarantine_status: None,
    };
    let result = store.update(fake_record).await;
    assert!(matches!(result, Err(StoreError::EntryNotFound(999))));
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_update_status() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let entry = TestEntry::new("topic", "cat").build();
    let id = store.insert(entry).await.unwrap();

    store.update_status(id, Status::Deprecated).await.unwrap();
    let record = store.get(id).await.unwrap();
    assert_eq!(record.status, Status::Deprecated);
    assert_index_consistent(&store, id).await;
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_delete() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let entry = TestEntry::new("topic", "cat").build();
    let id = store.insert(entry).await.unwrap();
    store.delete(id).await.unwrap();
    assert!(!store.exists(id).await.unwrap());
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_delete_not_found() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let result = store.delete(999).await;
    assert!(matches!(result, Err(StoreError::EntryNotFound(999))));
    store.close().await.unwrap();
}

// === QUERY OPERATIONS ===

#[tokio::test]
async fn test_query_by_topic() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id1 = store
        .insert(TestEntry::new("auth", "conv").build())
        .await
        .unwrap();
    let _id2 = store
        .insert(TestEntry::new("logging", "conv").build())
        .await
        .unwrap();
    let id3 = store
        .insert(TestEntry::new("auth", "pattern").build())
        .await
        .unwrap();

    let results = store.query_by_topic("auth").await.unwrap();
    let ids: Vec<u64> = results.iter().map(|r| r.id).collect();
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id3));
    assert_eq!(ids.len(), 2);
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_query_by_category() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id1 = store
        .insert(TestEntry::new("a", "convention").build())
        .await
        .unwrap();
    let _id2 = store
        .insert(TestEntry::new("b", "decision").build())
        .await
        .unwrap();

    let results = store.query_by_category("convention").await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, id1);
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_query_by_tags() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id1 = store
        .insert(
            TestEntry::new("a", "b")
                .with_tags(&["rust", "async"])
                .build(),
        )
        .await
        .unwrap();
    let _id2 = store
        .insert(TestEntry::new("c", "d").with_tags(&["rust"]).build())
        .await
        .unwrap();

    let results = store.query_by_tags(&["rust".to_string()]).await.unwrap();
    assert_eq!(results.len(), 2);

    let results = store
        .query_by_tags(&["rust".to_string(), "async".to_string()])
        .await
        .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, id1);
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_query_by_status() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id1 = store
        .insert(TestEntry::new("a", "b").build())
        .await
        .unwrap();
    let id2 = store
        .insert(TestEntry::new("c", "d").build())
        .await
        .unwrap();
    store.update_status(id2, Status::Deprecated).await.unwrap();

    let active = store.query_by_status(Status::Active).await.unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].id, id1);

    let deprecated = store.query_by_status(Status::Deprecated).await.unwrap();
    assert_eq!(deprecated.len(), 1);
    assert_eq!(deprecated[0].id, id2);
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_query_by_time_range() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let _id = store
        .insert(TestEntry::new("a", "b").build())
        .await
        .unwrap();
    let now = now_secs();

    let results = store
        .query_by_time_range(TimeRange {
            start: now - 10,
            end: now + 10,
        })
        .await
        .unwrap();
    assert!(!results.is_empty());

    let results = store
        .query_by_time_range(TimeRange {
            start: now + 1000,
            end: now + 2000,
        })
        .await
        .unwrap();
    assert!(results.is_empty());
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_combined_query() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id1 = store
        .insert(
            TestEntry::new("auth", "convention")
                .with_tags(&["rust"])
                .build(),
        )
        .await
        .unwrap();
    let _id2 = store
        .insert(
            TestEntry::new("auth", "decision")
                .with_tags(&["rust"])
                .build(),
        )
        .await
        .unwrap();
    let _id3 = store
        .insert(
            TestEntry::new("logging", "convention")
                .with_tags(&["rust"])
                .build(),
        )
        .await
        .unwrap();

    let filter = QueryFilter {
        topic: Some("auth".to_string()),
        category: Some("convention".to_string()),
        tags: None,
        status: Some(Status::Active),
        time_range: None,
    };
    let results = store.query(filter).await.unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, id1);
    store.close().await.unwrap();
}

// === USAGE AND CONFIDENCE ===

#[tokio::test]
async fn test_record_usage() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = store
        .insert(TestEntry::new("a", "b").build())
        .await
        .unwrap();

    store
        .record_usage(&[id], &[id], &[id], &[], &[], &[])
        .await
        .unwrap();
    let record = store.get(id).await.unwrap();
    assert_eq!(record.access_count, 1);
    assert_eq!(record.helpful_count, 1);
    assert!(record.last_accessed_at > 0);

    store
        .record_usage(&[id], &[id], &[], &[id], &[], &[])
        .await
        .unwrap();
    let record = store.get(id).await.unwrap();
    assert_eq!(record.access_count, 2);
    assert_eq!(record.unhelpful_count, 1);
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_record_usage_with_confidence() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = store
        .insert(TestEntry::new("a", "b").build())
        .await
        .unwrap();

    store
        .record_usage_with_confidence(
            &[id],
            &[id],
            &[id],
            &[],
            &[],
            &[],
            Some(
                Box::new(|_record: &unimatrix_store::EntryRecord, _now: u64| 0.85_f64)
                    as Box<dyn Fn(&unimatrix_store::EntryRecord, u64) -> f64 + Send + Sync>,
            ),
        )
        .await
        .unwrap();
    let record = store.get(id).await.unwrap();
    assert_eq!(record.confidence, 0.85);
    assert_eq!(record.helpful_count, 1);
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_r11_store_does_not_deduplicate_duplicate_all_ids() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = store
        .insert(TestEntry::new("a", "b").build())
        .await
        .unwrap();

    let initial = store.get(id).await.unwrap().access_count;

    store
        .record_usage_with_confidence(&[id, id], &[id, id], &[], &[], &[], &[], None)
        .await
        .unwrap();

    let after = store.get(id).await.unwrap().access_count;
    assert_eq!(
        after,
        initial + 2,
        "store does not deduplicate all_ids: duplicate ID increments access_count by 2"
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_increment_access_counts_applies_extra_increment() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = store
        .insert(TestEntry::new("a", "b").build())
        .await
        .unwrap();

    let initial = store.get(id).await.unwrap().access_count;

    store
        .record_usage_with_confidence(&[id], &[id], &[], &[], &[], &[], None)
        .await
        .unwrap();
    store.increment_access_counts(&[id], 1).await.unwrap();

    let after = store.get(id).await.unwrap().access_count;
    assert_eq!(
        after,
        initial + 2,
        "record_usage (+1) + increment_access_counts (+1) = access_count += 2"
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_update_confidence() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id = store
        .insert(TestEntry::new("a", "b").build())
        .await
        .unwrap();

    store.update_confidence(id, 0.92).await.unwrap();
    let record = store.get(id).await.unwrap();
    assert_eq!(record.confidence, 0.92);
    store.close().await.unwrap();
}

// === VECTOR MAP ===

#[tokio::test]
async fn test_vector_mapping() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    store.put_vector_mapping(1, 100).await.unwrap();
    store.put_vector_mapping(2, 200).await.unwrap();

    assert_eq!(store.get_vector_mapping(1).await.unwrap(), Some(100));
    assert_eq!(store.get_vector_mapping(2).await.unwrap(), Some(200));
    assert_eq!(store.get_vector_mapping(3).await.unwrap(), None);

    let mappings = store.iter_vector_mappings().await.unwrap();
    assert_eq!(mappings.len(), 2);
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_rewrite_vector_map() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    store.put_vector_mapping(1, 100).await.unwrap();
    store.put_vector_mapping(2, 200).await.unwrap();

    store
        .rewrite_vector_map(&[(10, 1000), (20, 2000)])
        .await
        .unwrap();
    assert_eq!(store.get_vector_mapping(1).await.unwrap(), None);
    assert_eq!(store.get_vector_mapping(10).await.unwrap(), Some(1000));
    assert_eq!(store.get_vector_mapping(20).await.unwrap(), Some(2000));
    store.close().await.unwrap();
}

// === FEATURE ENTRIES ===

#[tokio::test]
async fn test_record_feature_entries() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let id1 = store
        .insert(TestEntry::new("a", "b").build())
        .await
        .unwrap();
    let id2 = store
        .insert(TestEntry::new("c", "d").build())
        .await
        .unwrap();
    store
        .record_feature_entries("col-001", &[id1, id2], None)
        .await
        .unwrap();
    store.close().await.unwrap();
}

// === CO-ACCESS ===

#[tokio::test]
async fn test_co_access_roundtrip() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    store.record_co_access_pairs(&[(1, 2), (1, 3)]);
    let store = flush(store, &dir).await;

    let partners = store.get_co_access_partners(1, 0).await.unwrap();
    assert_eq!(partners.len(), 2);

    let (total, active) = store.co_access_stats(0).await.unwrap();
    assert_eq!(total, 2);
    assert_eq!(active, 2);
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_co_access_increment() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    store.record_co_access_pairs(&[(1, 2)]);
    store.record_co_access_pairs(&[(1, 2)]);
    let store = flush(store, &dir).await;

    let partners = store.get_co_access_partners(1, 0).await.unwrap();
    assert_eq!(partners.len(), 1);
    assert_eq!(partners[0].1.count, 2);
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_co_access_self_pair_skipped() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    store.record_co_access_pairs(&[(1, 1)]);
    let store = flush(store, &dir).await;
    let (total, _) = store.co_access_stats(0).await.unwrap();
    assert_eq!(total, 0);
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_cleanup_stale_co_access() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    store.record_co_access_pairs(&[(1, 2)]);
    store.record_co_access_pairs(&[(3, 4)]);
    let store = flush(store, &dir).await;

    let future = now_secs() + 1000;
    let deleted = store.cleanup_stale_co_access(future).await.unwrap();
    assert_eq!(deleted, 2);

    let (total, _) = store.co_access_stats(0).await.unwrap();
    assert_eq!(total, 0);
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_top_co_access_pairs() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    store.record_co_access_pairs(&[(1, 2)]);
    store.record_co_access_pairs(&[(1, 2)]); // count=2
    store.record_co_access_pairs(&[(3, 4)]); // count=1
    let store = flush(store, &dir).await;

    let top = store.top_co_access_pairs(10, 0).await.unwrap();
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].1.count, 2); // highest count first
    store.close().await.unwrap();
}

// === OBSERVATION METRICS (nxs-009: typed API) ===

use std::collections::BTreeMap;
use unimatrix_store::{MetricVector, PhaseMetrics, UNIVERSAL_METRICS_FIELDS, UniversalMetrics};

fn sample_metric_vector() -> MetricVector {
    let mut phases = BTreeMap::new();
    phases.insert(
        "3a".to_string(),
        PhaseMetrics {
            duration_secs: 600,
            tool_call_count: 15,
        },
    );
    phases.insert(
        "3b".to_string(),
        PhaseMetrics {
            duration_secs: 300,
            tool_call_count: 25,
        },
    );
    phases.insert(
        "3c".to_string(),
        PhaseMetrics {
            duration_secs: 120,
            tool_call_count: 8,
        },
    );

    MetricVector {
        computed_at: 1700000000,
        universal: UniversalMetrics {
            total_tool_calls: 42,
            total_duration_secs: 1020,
            session_count: 3,
            search_miss_rate: 0.15,
            edit_bloat_total_kb: 12.5,
            edit_bloat_ratio: 1.3,
            permission_friction_events: 2,
            bash_for_search_count: 5,
            cold_restart_events: 1,
            coordinator_respawn_count: 0,
            parallel_call_rate: 0.67,
            context_load_before_first_write_kb: 45.2,
            total_context_loaded_kb: 128.0,
            post_completion_work_pct: 0.12,
            follow_up_issues_created: 3,
            knowledge_entries_stored: 7,
            sleep_workaround_count: 0,
            agent_hotspot_count: 1,
            friction_hotspot_count: 2,
            session_hotspot_count: 0,
            scope_hotspot_count: 1,
        },
        phases,
        domain_metrics: std::collections::HashMap::new(),
    }
}

#[tokio::test]
async fn test_store_and_get_metrics() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let mv = sample_metric_vector();
    store.store_metrics("col-001", &mv);
    let store = flush(store, &dir).await;

    let got = store.get_metrics("col-001").await.unwrap().unwrap();
    assert_eq!(got, mv);
    assert!(store.get_metrics("nonexistent").await.unwrap().is_none());
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_store_metrics_replace_phases() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let mut mv1 = MetricVector::default();
    mv1.computed_at = 100;
    mv1.phases.insert(
        "3a".to_string(),
        PhaseMetrics {
            duration_secs: 10,
            tool_call_count: 5,
        },
    );
    mv1.phases.insert(
        "3b".to_string(),
        PhaseMetrics {
            duration_secs: 20,
            tool_call_count: 10,
        },
    );
    store.store_metrics("col-001", &mv1);
    let store = flush(store, &dir).await;

    let mut mv2 = MetricVector::default();
    mv2.computed_at = 200;
    mv2.phases.insert(
        "3a".to_string(),
        PhaseMetrics {
            duration_secs: 15,
            tool_call_count: 7,
        },
    );
    mv2.phases.insert(
        "3c".to_string(),
        PhaseMetrics {
            duration_secs: 30,
            tool_call_count: 12,
        },
    );
    store.store_metrics("col-001", &mv2);
    let store = flush(store, &dir).await;

    let got = store.get_metrics("col-001").await.unwrap().unwrap();
    assert_eq!(got.computed_at, mv2.computed_at);
    assert_eq!(got.phases.len(), 2);
    assert!(got.phases.contains_key("3a"));
    assert!(got.phases.contains_key("3c"));
    assert!(!got.phases.contains_key("3b"));
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_store_metrics_empty_phases() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let mv = MetricVector {
        computed_at: 500,
        universal: UniversalMetrics::default(),
        phases: BTreeMap::new(),
        domain_metrics: std::collections::HashMap::new(),
    };
    store.store_metrics("empty-phases", &mv);
    let store = flush(store, &dir).await;

    let got = store.get_metrics("empty-phases").await.unwrap().unwrap();
    assert_eq!(got, mv);
    assert!(got.phases.is_empty());
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_list_all_metrics() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let mut mv_a = MetricVector::default();
    mv_a.computed_at = 100;
    mv_a.universal.total_tool_calls = 10;
    mv_a.phases.insert(
        "design".to_string(),
        PhaseMetrics {
            duration_secs: 60,
            tool_call_count: 5,
        },
    );

    let mut mv_b = MetricVector::default();
    mv_b.computed_at = 200;
    mv_b.universal.total_tool_calls = 20;
    mv_b.phases.insert(
        "impl".to_string(),
        PhaseMetrics {
            duration_secs: 120,
            tool_call_count: 15,
        },
    );
    mv_b.phases.insert(
        "test".to_string(),
        PhaseMetrics {
            duration_secs: 30,
            tool_call_count: 3,
        },
    );

    let mv_c = MetricVector::default();

    store.store_metrics("a-feature", &mv_a);
    store.store_metrics("b-feature", &mv_b);
    store.store_metrics("c-feature", &mv_c);
    let store = flush(store, &dir).await;

    let all = store.list_all_metrics().await.unwrap();
    assert_eq!(all.len(), 3);

    assert_eq!(all[0].0, "a-feature");
    assert_eq!(all[1].0, "b-feature");
    assert_eq!(all[2].0, "c-feature");

    assert_eq!(all[0].1, mv_a);
    assert_eq!(all[1].1, mv_b);
    assert_eq!(all[2].1, mv_c);
    assert_eq!(all[0].1.phases.len(), 1);
    assert_eq!(all[1].1.phases.len(), 2);
    assert!(all[2].1.phases.is_empty());
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_list_all_metrics_overlapping_phases() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    for i in 0..5_u64 {
        let mut mv = MetricVector::default();
        mv.computed_at = i * 100;
        mv.universal.total_tool_calls = i * 10;
        mv.phases.insert(
            "3a".to_string(),
            PhaseMetrics {
                duration_secs: i * 10,
                tool_call_count: i,
            },
        );
        mv.phases.insert(
            "3b".to_string(),
            PhaseMetrics {
                duration_secs: i * 20,
                tool_call_count: i * 2,
            },
        );
        store.store_metrics(&format!("feature-{i:03}"), &mv);
    }
    let store = flush(store, &dir).await;

    let all = store.list_all_metrics().await.unwrap();
    assert_eq!(all.len(), 5);

    for (i, (fc, mv)) in all.iter().enumerate() {
        assert_eq!(fc, &format!("feature-{i:03}"));
        assert_eq!(mv.phases.len(), 2);
        assert_eq!(mv.phases["3a"].duration_secs, i as u64 * 10);
        assert_eq!(mv.phases["3b"].tool_call_count, i as u64 * 2);
    }
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_delete_cascade_phases() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let mut mv = sample_metric_vector();
    mv.phases.insert(
        "extra".to_string(),
        PhaseMetrics {
            duration_secs: 50,
            tool_call_count: 3,
        },
    );
    store.store_metrics("cascade-test", &mv);
    let store = flush(store, &dir).await;

    let got = store.get_metrics("cascade-test").await.unwrap().unwrap();
    assert_eq!(got.phases.len(), 4);

    // Delete parent row via sqlx
    sqlx::query("DELETE FROM observation_metrics WHERE feature_cycle = ?1")
        .bind("cascade-test")
        .execute(store.write_pool_test())
        .await
        .unwrap();

    assert!(store.get_metrics("cascade-test").await.unwrap().is_none());

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM observation_phase_metrics WHERE feature_cycle = ?1",
    )
    .bind("cascade-test")
    .fetch_one(store.read_pool_test())
    .await
    .unwrap();
    assert_eq!(count, 0, "CASCADE should have removed phase rows");
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_schema_column_count() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let metric_cols: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('observation_metrics')")
            .fetch_one(store.read_pool_test())
            .await
            .unwrap();
    // Schema v14 (col-023): feature_cycle + computed_at + 21 typed + domain_metrics_json = 24.
    assert_eq!(
        metric_cols, 24,
        "observation_metrics should have 24 columns (schema v14, ADR-006)"
    );

    let phase_cols: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('observation_phase_metrics')")
            .fetch_one(store.read_pool_test())
            .await
            .unwrap();
    assert_eq!(
        phase_cols, 4,
        "observation_phase_metrics should have 4 columns"
    );

    let has_blob: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('observation_metrics') WHERE type = 'BLOB'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .unwrap();
    assert_eq!(
        has_blob, 0,
        "observation_metrics should not have a BLOB column"
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_column_field_alignment() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    use sqlx::Row;
    let rows =
        sqlx::query("SELECT name FROM pragma_table_info('observation_metrics') ORDER BY cid")
            .fetch_all(store.read_pool_test())
            .await
            .unwrap();

    let sql_columns: Vec<String> = rows
        .iter()
        .map(|r| r.try_get::<String, _>(0).unwrap())
        .collect();

    let universal_columns: Vec<&str> = sql_columns
        .iter()
        .skip(2) // feature_cycle, computed_at
        .map(|s| s.as_str())
        .collect();

    assert_eq!(
        universal_columns.len(),
        UNIVERSAL_METRICS_FIELDS.len(),
        "SQL column count must match UNIVERSAL_METRICS_FIELDS count"
    );

    for (sql_col, rust_field) in universal_columns
        .iter()
        .zip(UNIVERSAL_METRICS_FIELDS.iter())
    {
        assert_eq!(
            sql_col, rust_field,
            "SQL column name '{}' does not match Rust field name '{}'",
            sql_col, rust_field
        );
    }
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_sql_analytics_query() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let mut mv1 = MetricVector::default();
    mv1.universal.session_count = 10;
    mv1.universal.total_tool_calls = 100;
    store.store_metrics("feature-a", &mv1);

    let mut mv2 = MetricVector::default();
    mv2.universal.session_count = 3;
    mv2.universal.total_tool_calls = 30;
    store.store_metrics("feature-b", &mv2);
    let store = flush(store, &dir).await;

    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT feature_cycle, total_tool_calls FROM observation_metrics WHERE session_count > 5",
    )
    .fetch_all(store.read_pool_test())
    .await
    .unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].try_get::<String, _>(0).unwrap(), "feature-a");
    assert_eq!(rows[0].try_get::<i64, _>(1).unwrap(), 100);
    store.close().await.unwrap();
}

// Note: test was test_schema_version_is_13 (crt-021). Updated to 14 for col-023.
// Updated to 15 for crt-025 (cycle_events + feature_entries.phase).
// Updated to 16 for col-025 (cycle_events.goal column).
// Updated to 17 for col-028 (query_log.phase column). Uses >= per pattern #2933.
// Updated to 18 for crt-033 (cycle_review_index table).
// Updated to 19 for crt-035 (bidirectional CoAccess back-fill).
#[tokio::test]
async fn test_schema_version_is_14() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let version = store.read_counter("schema_version").await.unwrap();
    assert_eq!(
        version, 19,
        "schema version must be 19 after crt-035 (was 18 after crt-033)"
    );
    store.close().await.unwrap();
}

// === COUNTERS ===

#[tokio::test]
async fn test_read_counter() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;
    let version = store.read_counter("schema_version").await.unwrap();
    assert!(version >= 9, "schema_version should be >= 9, got {version}");

    let next = store.read_counter("next_entry_id").await.unwrap();
    assert_eq!(next, 1);

    let missing = store.read_counter("nonexistent").await.unwrap();
    assert_eq!(missing, 0);
    store.close().await.unwrap();
}

// === SCHEMA DDL: topic_deliveries and query_log (nxs-010) ===

#[tokio::test]
async fn test_create_tables_topic_deliveries_schema() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info('topic_deliveries') ORDER BY cid"
    )
    .fetch_all(store.read_pool_test())
    .await
    .unwrap();

    assert_eq!(rows.len(), 9, "topic_deliveries should have 9 columns");

    let names: Vec<String> = rows
        .iter()
        .map(|r| r.try_get::<String, _>(0).unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            "topic",
            "created_at",
            "completed_at",
            "status",
            "github_issue",
            "total_sessions",
            "total_tool_calls",
            "total_duration_secs",
            "phases_completed",
        ]
    );

    // topic is TEXT PRIMARY KEY (pk = 1)
    assert_eq!(rows[0].try_get::<String, _>(1).unwrap(), "TEXT");
    assert_eq!(
        rows[0].try_get::<i64, _>(4).unwrap(),
        1,
        "topic must be primary key"
    );

    // created_at is INTEGER NOT NULL
    assert_eq!(rows[1].try_get::<String, _>(1).unwrap(), "INTEGER");
    assert_eq!(
        rows[1].try_get::<i64, _>(2).unwrap(),
        1,
        "created_at must be NOT NULL"
    );

    // completed_at is nullable
    assert_eq!(
        rows[2].try_get::<i64, _>(2).unwrap(),
        0,
        "completed_at must be nullable"
    );

    // status has default 'active'
    assert_eq!(
        rows[3].try_get::<i64, _>(2).unwrap(),
        1,
        "status must be NOT NULL"
    );
    assert_eq!(
        rows[3].try_get::<Option<String>, _>(3).unwrap().as_deref(),
        Some("'active'")
    );

    // github_issue is nullable
    assert_eq!(
        rows[4].try_get::<i64, _>(2).unwrap(),
        0,
        "github_issue must be nullable"
    );

    // counter columns have NOT NULL + DEFAULT 0
    for i in 5..=7 {
        assert_eq!(
            rows[i].try_get::<i64, _>(2).unwrap(),
            1,
            "{} must be NOT NULL",
            names[i]
        );
        assert_eq!(
            rows[i].try_get::<Option<String>, _>(3).unwrap().as_deref(),
            Some("0"),
            "{} must default to 0",
            names[i]
        );
    }

    // phases_completed is nullable
    assert_eq!(
        rows[8].try_get::<i64, _>(2).unwrap(),
        0,
        "phases_completed must be nullable"
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_create_tables_query_log_schema() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    use sqlx::Row;
    let rows = sqlx::query(
        "SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info('query_log') ORDER BY cid"
    )
    .fetch_all(store.read_pool_test())
    .await
    .unwrap();

    assert_eq!(rows.len(), 10, "query_log should have 10 columns"); // col-028: phase added

    let names: Vec<String> = rows
        .iter()
        .map(|r| r.try_get::<String, _>(0).unwrap())
        .collect();
    assert_eq!(
        names,
        vec![
            "query_id",
            "session_id",
            "query_text",
            "ts",
            "result_count",
            "result_entry_ids",
            "similarity_scores",
            "retrieval_mode",
            "source",
            "phase", // col-028
        ]
    );

    assert_eq!(rows[0].try_get::<String, _>(1).unwrap(), "INTEGER");
    assert_eq!(
        rows[0].try_get::<i64, _>(4).unwrap(),
        1,
        "query_id must be primary key"
    );

    assert_eq!(rows[1].try_get::<String, _>(1).unwrap(), "TEXT");
    assert_eq!(
        rows[1].try_get::<i64, _>(2).unwrap(),
        1,
        "session_id must be NOT NULL"
    );
    assert_eq!(rows[2].try_get::<String, _>(1).unwrap(), "TEXT");
    assert_eq!(
        rows[2].try_get::<i64, _>(2).unwrap(),
        1,
        "query_text must be NOT NULL"
    );

    assert_eq!(rows[8].try_get::<String, _>(1).unwrap(), "TEXT");
    assert_eq!(
        rows[8].try_get::<i64, _>(2).unwrap(),
        1,
        "source must be NOT NULL"
    );

    assert_eq!(
        rows[5].try_get::<i64, _>(2).unwrap(),
        0,
        "result_entry_ids must be nullable"
    );
    assert_eq!(
        rows[6].try_get::<i64, _>(2).unwrap(),
        0,
        "similarity_scores must be nullable"
    );
    assert_eq!(
        rows[7].try_get::<i64, _>(2).unwrap(),
        0,
        "retrieval_mode must be nullable"
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_create_tables_query_log_indexes() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    use sqlx::Row;
    let rows = sqlx::query("SELECT name FROM pragma_index_list('query_log') WHERE origin != 'pk'")
        .fetch_all(store.read_pool_test())
        .await
        .unwrap();

    let index_names: Vec<String> = rows
        .iter()
        .map(|r| r.try_get::<String, _>(0).unwrap())
        .collect();

    assert!(
        index_names.len() >= 2,
        "query_log should have at least 2 non-pk indexes, got {}",
        index_names.len()
    );
    assert!(
        index_names.contains(&"idx_query_log_session".to_string()),
        "missing idx_query_log_session, found: {:?}",
        index_names
    );
    assert!(
        index_names.contains(&"idx_query_log_ts".to_string()),
        "missing idx_query_log_ts, found: {:?}",
        index_names
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_create_tables_query_log_autoincrement() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sqlite_sequence'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .unwrap();
    assert_eq!(
        count, 1,
        "sqlite_sequence table must exist (AUTOINCREMENT creates it)"
    );
    store.close().await.unwrap();
}

#[tokio::test]
async fn test_create_tables_idempotent() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("test.db");

    let store1 = unimatrix_store::SqlxStore::open(&path, Default::default())
        .await
        .expect("first open should succeed");
    store1.close().await.unwrap();

    let store2 = unimatrix_store::SqlxStore::open(&path, Default::default())
        .await
        .expect("second open should succeed");

    let td_cols: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('topic_deliveries')")
            .fetch_one(store2.read_pool_test())
            .await
            .unwrap();
    assert_eq!(
        td_cols, 9,
        "topic_deliveries should still have 9 columns after re-open"
    );

    let ql_cols: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info('query_log')")
        .fetch_one(store2.read_pool_test())
        .await
        .unwrap();
    assert_eq!(
        ql_cols, 10,
        "query_log should still have 10 columns after re-open" // col-028: phase added
    );
    store2.close().await.unwrap();
}

// === SCHEMA DDL: cycle_review_index (crt-033) ===

#[tokio::test]
async fn test_create_tables_cycle_review_index_exists() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='cycle_review_index'",
    )
    .fetch_one(store.read_pool_test())
    .await
    .unwrap();

    assert_eq!(
        count, 1,
        "cycle_review_index table must exist after fresh schema creation (crt-033)"
    );

    store.close().await.unwrap();
}

#[tokio::test]
async fn test_create_tables_cycle_review_index_schema() {
    let dir = tempfile::TempDir::new().unwrap();
    let store = open_test_store(&dir).await;

    use sqlx::Row;
    let rows = sqlx::query("SELECT name FROM pragma_table_info('cycle_review_index') ORDER BY cid")
        .fetch_all(store.read_pool_test())
        .await
        .unwrap();

    assert_eq!(
        rows.len(),
        5,
        "cycle_review_index must have exactly 5 columns (crt-033)"
    );

    let names: Vec<String> = rows
        .iter()
        .map(|r| r.try_get::<String, _>(0).unwrap())
        .collect();

    assert!(
        names.contains(&"feature_cycle".to_string()),
        "cycle_review_index must have feature_cycle column"
    );
    assert!(
        names.contains(&"schema_version".to_string()),
        "cycle_review_index must have schema_version column"
    );
    assert!(
        names.contains(&"computed_at".to_string()),
        "cycle_review_index must have computed_at column"
    );
    assert!(
        names.contains(&"raw_signals_available".to_string()),
        "cycle_review_index must have raw_signals_available column"
    );
    assert!(
        names.contains(&"summary_json".to_string()),
        "cycle_review_index must have summary_json column"
    );

    store.close().await.unwrap();
}
