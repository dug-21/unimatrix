//! Store integration tests.
//!
//! These tests exercise all Store operations.
//!
//! Coverage: CRUD, queries, usage tracking, confidence, vector map,
//! feature entries, co-access, metrics, counters.
//! See sqlite_parity_specialized.rs for signals, sessions, injection log.

#![cfg(feature = "test-support")]

use unimatrix_store::test_helpers::{TestDb, TestEntry, assert_index_consistent};
use unimatrix_store::{QueryFilter, Status, StoreError, TimeRange};

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
        pre_quarantine_status: None,
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
    let id1 = db
        .store()
        .insert(TestEntry::new("auth", "conv").build())
        .unwrap();
    let _id2 = db
        .store()
        .insert(TestEntry::new("logging", "conv").build())
        .unwrap();
    let id3 = db
        .store()
        .insert(TestEntry::new("auth", "pattern").build())
        .unwrap();

    let results = db.store().query_by_topic("auth").unwrap();
    let ids: Vec<u64> = results.iter().map(|r| r.id).collect();
    assert!(ids.contains(&id1));
    assert!(ids.contains(&id3));
    assert_eq!(ids.len(), 2);
}

#[test]
fn test_query_by_category() {
    let db = TestDb::new();
    let id1 = db
        .store()
        .insert(TestEntry::new("a", "convention").build())
        .unwrap();
    let _id2 = db
        .store()
        .insert(TestEntry::new("b", "decision").build())
        .unwrap();

    let results = db.store().query_by_category("convention").unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, id1);
}

#[test]
fn test_query_by_tags() {
    let db = TestDb::new();
    let id1 = db
        .store()
        .insert(
            TestEntry::new("a", "b")
                .with_tags(&["rust", "async"])
                .build(),
        )
        .unwrap();
    let _id2 = db
        .store()
        .insert(TestEntry::new("c", "d").with_tags(&["rust"]).build())
        .unwrap();

    // Both have "rust"
    let results = db.store().query_by_tags(&["rust".to_string()]).unwrap();
    assert_eq!(results.len(), 2);

    // Only id1 has both "rust" AND "async"
    let results = db
        .store()
        .query_by_tags(&["rust".to_string(), "async".to_string()])
        .unwrap();
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
    let results = db
        .store()
        .query_by_time_range(TimeRange {
            start: now - 10,
            end: now + 10,
        })
        .unwrap();
    assert!(!results.is_empty());

    // Future range should not
    let results = db
        .store()
        .query_by_time_range(TimeRange {
            start: now + 1000,
            end: now + 2000,
        })
        .unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_combined_query() {
    let db = TestDb::new();
    let id1 = db
        .store()
        .insert(
            TestEntry::new("auth", "convention")
                .with_tags(&["rust"])
                .build(),
        )
        .unwrap();
    let _id2 = db
        .store()
        .insert(
            TestEntry::new("auth", "decision")
                .with_tags(&["rust"])
                .build(),
        )
        .unwrap();
    let _id3 = db
        .store()
        .insert(
            TestEntry::new("logging", "convention")
                .with_tags(&["rust"])
                .build(),
        )
        .unwrap();

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
    db.store()
        .record_usage(&[id], &[id], &[id], &[], &[], &[])
        .unwrap();
    let record = db.store().get(id).unwrap();
    assert_eq!(record.access_count, 1);
    assert_eq!(record.helpful_count, 1);
    assert!(record.last_accessed_at > 0);

    // Record usage with unhelpful vote
    db.store()
        .record_usage(&[id], &[id], &[], &[id], &[], &[])
        .unwrap();
    let record = db.store().get(id).unwrap();
    assert_eq!(record.access_count, 2);
    assert_eq!(record.unhelpful_count, 1);
}

#[test]
fn test_record_usage_with_confidence() {
    let db = TestDb::new();
    let id = db.store().insert(TestEntry::new("a", "b").build()).unwrap();

    // Use confidence function that returns a fixed value
    db.store()
        .record_usage_with_confidence(
            &[id],
            &[id],
            &[id],
            &[],
            &[],
            &[],
            Some(
                Box::new(|_record: &unimatrix_store::EntryRecord, _now: u64| 0.85_f64)
                    as Box<dyn Fn(&unimatrix_store::EntryRecord, u64) -> f64 + Send>,
            ),
        )
        .unwrap();
    let record = db.store().get(id).unwrap();
    assert_eq!(record.confidence, 0.85);
    assert_eq!(record.helpful_count, 1);
}

/// R-11 gate: Verify the store does NOT deduplicate IDs in record_usage_with_confidence.
///
/// The store loops over `all_ids` (outer loop), and checks `access_ids` via a HashSet
/// for set-membership. When `all_ids = [id, id]`, the UPDATE executes twice for the
/// same row, producing access_count += 2.
///
/// This means the flat_map repeat approach IS viable:
///   access_weight=2 -> all_ids=[id,id], access_ids=[id,id] -> access_count += 2
#[test]
fn test_r11_store_does_not_deduplicate_duplicate_all_ids() {
    let db = TestDb::new();
    let id = db.store().insert(TestEntry::new("a", "b").build()).unwrap();

    let initial = db.store().get(id).unwrap().access_count;

    // Pass the same ID twice in both all_ids and access_ids
    db.store()
        .record_usage_with_confidence(
            &[id, id], // all_ids: duplicate — store loops over this, so UPDATE runs twice
            &[id, id], // access_ids: duplicate
            &[],
            &[],
            &[],
            &[],
            None,
        )
        .unwrap();

    let after = db.store().get(id).unwrap().access_count;
    // Store does NOT deduplicate: access_count += 2
    assert_eq!(
        after,
        initial + 2,
        "store does not deduplicate all_ids: duplicate ID increments access_count by 2"
    );
}

/// R-11 fallback: increment_access_counts applies extra increments correctly.
///
/// When access_weight = 2, record_usage_with_confidence applies +1, then
/// increment_access_counts applies the additional +1. Net result: access_count += 2.
#[test]
fn test_increment_access_counts_applies_extra_increment() {
    let db = TestDb::new();
    let id = db.store().insert(TestEntry::new("a", "b").build()).unwrap();

    let initial = db.store().get(id).unwrap().access_count;

    // Step 1: normal record (dedup-compatible, increments by 1)
    db.store()
        .record_usage_with_confidence(&[id], &[id], &[], &[], &[], &[], None)
        .unwrap();

    // Step 2: extra increment for access_weight - 1 = 1
    db.store().increment_access_counts(&[id], 1).unwrap();

    let after = db.store().get(id).unwrap().access_count;
    assert_eq!(
        after,
        initial + 2,
        "record_usage (+1) + increment_access_counts (+1) = access_count += 2"
    );
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

    db.store()
        .rewrite_vector_map(&[(10, 1000), (20, 2000)])
        .unwrap();
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
    db.store()
        .record_feature_entries("col-001", &[id1, id2])
        .unwrap();
    // No panic = success (feature_entries is write-only from Store API)
}

// === CO-ACCESS ===

#[test]
fn test_co_access_roundtrip() {
    let db = TestDb::new();
    let now = now_secs();

    db.store()
        .record_co_access_pairs(&[(1, 2), (1, 3)])
        .unwrap();

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

// === OBSERVATION METRICS (nxs-009: typed API) ===

use std::collections::BTreeMap;
use unimatrix_store::{MetricVector, PhaseMetrics, UNIVERSAL_METRICS_FIELDS, UniversalMetrics};

/// Build a fully populated MetricVector for testing.
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
    }
}

#[test]
fn test_store_and_get_metrics() {
    // AC-02: Store roundtrip with 21 universal metrics, 3 phases, non-zero computed_at
    let db = TestDb::new();
    let mv = sample_metric_vector();
    db.store().store_metrics("col-001", &mv).unwrap();

    let got = db.store().get_metrics("col-001").unwrap().unwrap();
    assert_eq!(got, mv);

    assert!(db.store().get_metrics("nonexistent").unwrap().is_none());
}

#[test]
fn test_store_metrics_replace_phases() {
    // AC-03: Replace semantics — phases ["3a","3b"] -> ["3a","3c"]
    let db = TestDb::new();

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
    db.store().store_metrics("col-001", &mv1).unwrap();

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
    db.store().store_metrics("col-001", &mv2).unwrap();

    let got = db.store().get_metrics("col-001").unwrap().unwrap();
    assert_eq!(got, mv2);
    assert_eq!(got.phases.len(), 2);
    assert!(got.phases.contains_key("3a"));
    assert!(got.phases.contains_key("3c"));
    assert!(!got.phases.contains_key("3b"));
}

#[test]
fn test_store_metrics_empty_phases() {
    // AC-12: Empty phases roundtrip
    let db = TestDb::new();
    let mv = MetricVector {
        computed_at: 500,
        universal: UniversalMetrics::default(),
        phases: BTreeMap::new(),
    };
    db.store().store_metrics("empty-phases", &mv).unwrap();

    let got = db.store().get_metrics("empty-phases").unwrap().unwrap();
    assert_eq!(got, mv);
    assert!(got.phases.is_empty());
}

#[test]
fn test_list_all_metrics() {
    // AC-04: List all with correct phase attachment
    let db = TestDb::new();

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

    let mv_c = MetricVector::default(); // no phases

    db.store().store_metrics("a-feature", &mv_a).unwrap();
    db.store().store_metrics("b-feature", &mv_b).unwrap();
    db.store().store_metrics("c-feature", &mv_c).unwrap();

    let all = db.store().list_all_metrics().unwrap();
    assert_eq!(all.len(), 3);

    // Ordered by feature_cycle
    assert_eq!(all[0].0, "a-feature");
    assert_eq!(all[1].0, "b-feature");
    assert_eq!(all[2].0, "c-feature");

    // Verify phase attachment
    assert_eq!(all[0].1, mv_a);
    assert_eq!(all[1].1, mv_b);
    assert_eq!(all[2].1, mv_c);
    assert_eq!(all[0].1.phases.len(), 1);
    assert_eq!(all[1].1.phases.len(), 2);
    assert!(all[2].1.phases.is_empty());
}

#[test]
fn test_list_all_metrics_overlapping_phases() {
    // R-04: Multiple features with overlapping phase names
    let db = TestDb::new();

    for i in 0..5 {
        let mut mv = MetricVector::default();
        mv.computed_at = i as u64 * 100;
        mv.universal.total_tool_calls = i as u64 * 10;
        mv.phases.insert(
            "3a".to_string(),
            PhaseMetrics {
                duration_secs: i as u64 * 10,
                tool_call_count: i as u64,
            },
        );
        mv.phases.insert(
            "3b".to_string(),
            PhaseMetrics {
                duration_secs: i as u64 * 20,
                tool_call_count: i as u64 * 2,
            },
        );
        db.store()
            .store_metrics(&format!("feature-{i:03}"), &mv)
            .unwrap();
    }

    let all = db.store().list_all_metrics().unwrap();
    assert_eq!(all.len(), 5);

    for (i, (fc, mv)) in all.iter().enumerate() {
        assert_eq!(fc, &format!("feature-{i:03}"));
        assert_eq!(mv.phases.len(), 2);
        assert_eq!(mv.phases["3a"].duration_secs, i as u64 * 10);
        assert_eq!(mv.phases["3b"].tool_call_count, i as u64 * 2);
    }
}

#[test]
fn test_delete_cascade_phases() {
    // AC-07: Delete cascade removes phase rows
    let db = TestDb::new();

    let mut mv = sample_metric_vector();
    mv.phases.insert(
        "extra".to_string(),
        PhaseMetrics {
            duration_secs: 50,
            tool_call_count: 3,
        },
    );
    db.store().store_metrics("cascade-test", &mv).unwrap();

    // Verify phases exist
    let got = db.store().get_metrics("cascade-test").unwrap().unwrap();
    assert_eq!(got.phases.len(), 4);

    // Delete parent row directly via SQL
    {
        let conn = db.store().lock_conn();
        conn.execute(
            "DELETE FROM observation_metrics WHERE feature_cycle = ?1",
            unimatrix_store::rusqlite::params!["cascade-test"],
        )
        .unwrap();
    }

    // Verify get returns None
    assert!(db.store().get_metrics("cascade-test").unwrap().is_none());

    // Verify no orphaned phase rows
    {
        let conn = db.store().lock_conn();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM observation_phase_metrics WHERE feature_cycle = ?1",
                unimatrix_store::rusqlite::params!["cascade-test"],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "CASCADE should have removed phase rows");
    }
}

#[test]
fn test_schema_column_count() {
    // AC-01: observation_metrics has 23 columns, observation_phase_metrics exists with 4 columns
    let db = TestDb::new();
    let conn = db.store().lock_conn();

    let metric_cols: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('observation_metrics')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        metric_cols, 23,
        "observation_metrics should have 23 columns"
    );

    let phase_cols: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('observation_phase_metrics')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        phase_cols, 4,
        "observation_phase_metrics should have 4 columns"
    );

    // Verify no BLOB column
    let has_blob: bool = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('observation_metrics') WHERE type = 'BLOB'",
            [],
            |row| Ok(row.get::<_, i64>(0)? > 0),
        )
        .unwrap();
    assert!(
        !has_blob,
        "observation_metrics should not have a BLOB column"
    );
}

#[test]
fn test_column_field_alignment() {
    // R-03: SQL columns match Rust struct field names
    let db = TestDb::new();
    let conn = db.store().lock_conn();

    // Get column names from SQLite (skip feature_cycle and computed_at which are not in UniversalMetrics)
    let mut stmt = conn
        .prepare("SELECT name FROM pragma_table_info('observation_metrics') ORDER BY cid")
        .unwrap();
    let sql_columns: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();

    // Skip first two columns (feature_cycle, computed_at)
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
}

#[test]
fn test_sql_analytics_query() {
    // AC-13: SQL analytics query works without Rust-side deserialization
    let db = TestDb::new();

    let mut mv1 = MetricVector::default();
    mv1.universal.session_count = 10;
    mv1.universal.total_tool_calls = 100;
    db.store().store_metrics("feature-a", &mv1).unwrap();

    let mut mv2 = MetricVector::default();
    mv2.universal.session_count = 3;
    mv2.universal.total_tool_calls = 30;
    db.store().store_metrics("feature-b", &mv2).unwrap();

    // Raw SQL query without any Rust deserialization
    let conn = db.store().lock_conn();
    let mut stmt = conn.prepare(
        "SELECT feature_cycle, total_tool_calls FROM observation_metrics WHERE session_count > 5"
    ).unwrap();
    let results: Vec<(String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, "feature-a");
    assert_eq!(results[0].1, 100);
}

#[test]
fn test_schema_version_is_12() {
    // C-04: Schema version must be exactly 12 (col-022)
    let db = TestDb::new();
    let version = db.store().read_counter("schema_version").unwrap();
    assert_eq!(version, 12, "schema version must be 12 after col-022");
}

// === COUNTERS ===

#[test]
fn test_read_counter() {
    let db = TestDb::new();
    // schema_version should be current (9) after creation
    let version = db.store().read_counter("schema_version").unwrap();
    assert!(version >= 9, "schema_version should be >= 9, got {version}");

    // next_entry_id should be 1 initially
    let next = db.store().read_counter("next_entry_id").unwrap();
    assert_eq!(next, 1);

    // nonexistent returns 0
    let missing = db.store().read_counter("nonexistent").unwrap();
    assert_eq!(missing, 0);
}

// === SCHEMA DDL: topic_deliveries and query_log (nxs-010) ===

#[test]
fn test_create_tables_topic_deliveries_schema() {
    // AC-01: topic_deliveries has 9 columns with correct names, types, and constraints
    let db = TestDb::new();
    let conn = db.store().lock_conn();

    // Collect column info
    let mut stmt = conn.prepare(
        "SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info('topic_deliveries') ORDER BY cid"
    ).unwrap();
    let columns: Vec<(String, String, i64, Option<String>, i64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();

    assert_eq!(columns.len(), 9, "topic_deliveries should have 9 columns");

    // Verify column names in order
    let names: Vec<&str> = columns.iter().map(|c| c.0.as_str()).collect();
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
    assert_eq!(columns[0].1, "TEXT");
    assert_eq!(columns[0].4, 1, "topic must be primary key");

    // created_at is INTEGER NOT NULL
    assert_eq!(columns[1].1, "INTEGER");
    assert_eq!(columns[1].2, 1, "created_at must be NOT NULL");

    // completed_at is nullable
    assert_eq!(columns[2].2, 0, "completed_at must be nullable");

    // status has default 'active'
    assert_eq!(columns[3].2, 1, "status must be NOT NULL");
    assert_eq!(columns[3].3.as_deref(), Some("'active'"));

    // github_issue is nullable
    assert_eq!(columns[4].2, 0, "github_issue must be nullable");

    // total_sessions, total_tool_calls, total_duration_secs default to 0
    for i in 5..=7 {
        assert_eq!(columns[i].2, 1, "{} must be NOT NULL", names[i]);
        assert_eq!(
            columns[i].3.as_deref(),
            Some("0"),
            "{} must default to 0",
            names[i]
        );
    }

    // phases_completed is nullable
    assert_eq!(columns[8].2, 0, "phases_completed must be nullable");
}

#[test]
fn test_create_tables_query_log_schema() {
    // AC-02: query_log has 9 columns with correct names, types, and constraints
    let db = TestDb::new();
    let conn = db.store().lock_conn();

    let mut stmt = conn.prepare(
        "SELECT name, type, \"notnull\", dflt_value, pk FROM pragma_table_info('query_log') ORDER BY cid"
    ).unwrap();
    let columns: Vec<(String, String, i64, Option<String>, i64)> = stmt
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();

    assert_eq!(columns.len(), 9, "query_log should have 9 columns");

    let names: Vec<&str> = columns.iter().map(|c| c.0.as_str()).collect();
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
        ]
    );

    // query_id is INTEGER PRIMARY KEY
    assert_eq!(columns[0].1, "INTEGER");
    assert_eq!(columns[0].4, 1, "query_id must be primary key");

    // session_id and query_text are TEXT NOT NULL
    assert_eq!(columns[1].1, "TEXT");
    assert_eq!(columns[1].2, 1, "session_id must be NOT NULL");
    assert_eq!(columns[2].1, "TEXT");
    assert_eq!(columns[2].2, 1, "query_text must be NOT NULL");

    // source is TEXT NOT NULL
    assert_eq!(columns[8].1, "TEXT");
    assert_eq!(columns[8].2, 1, "source must be NOT NULL");

    // result_entry_ids, similarity_scores, retrieval_mode are nullable
    assert_eq!(columns[5].2, 0, "result_entry_ids must be nullable");
    assert_eq!(columns[6].2, 0, "similarity_scores must be nullable");
    assert_eq!(columns[7].2, 0, "retrieval_mode must be nullable");
}

#[test]
fn test_create_tables_query_log_indexes() {
    // AC-03: query_log has idx_query_log_session and idx_query_log_ts indexes
    let db = TestDb::new();
    let conn = db.store().lock_conn();

    let mut stmt = conn
        .prepare("SELECT name FROM pragma_index_list('query_log') WHERE origin != 'pk'")
        .unwrap();
    let index_names: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();

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
}

#[test]
fn test_create_tables_query_log_autoincrement() {
    // R-03: AUTOINCREMENT creates sqlite_sequence table
    let db = TestDb::new();
    let conn = db.store().lock_conn();

    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sqlite_sequence'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        count, 1,
        "sqlite_sequence table must exist (AUTOINCREMENT creates it)"
    );
}

#[test]
fn test_create_tables_idempotent() {
    // AC-05: Opening the same database twice causes no errors
    let dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let path = dir.path().join("test.db");

    let _store1 = unimatrix_store::Store::open(&path).expect("first open should succeed");
    drop(_store1);

    let store2 = unimatrix_store::Store::open(&path).expect("second open should succeed");

    // Verify tables still exist with correct column counts
    let conn = store2.lock_conn();

    let td_cols: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('topic_deliveries')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        td_cols, 9,
        "topic_deliveries should still have 9 columns after re-open"
    );

    let ql_cols: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('query_log')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        ql_cols, 9,
        "query_log should still have 9 columns after re-open"
    );
}
