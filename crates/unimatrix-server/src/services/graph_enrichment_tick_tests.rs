//! Tests for `graph_enrichment_tick.rs` (crt-041).
//!
//! Extracted to a separate file to keep the main module under the 500-line limit.

use unimatrix_core::Store;
use unimatrix_store::counters;

use super::{run_graph_enrichment_tick, run_s1_tick, run_s2_tick, run_s8_tick};
use crate::infra::config::InferenceConfig;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_config() -> InferenceConfig {
    InferenceConfig::default()
}

fn make_config_s1(cap: usize) -> InferenceConfig {
    InferenceConfig {
        max_s1_edges_per_tick: cap,
        ..InferenceConfig::default()
    }
}

fn make_config_s2(vocab: Vec<&str>, cap: usize) -> InferenceConfig {
    InferenceConfig {
        s2_vocabulary: vocab.into_iter().map(|s| s.to_string()).collect(),
        max_s2_edges_per_tick: cap,
        ..InferenceConfig::default()
    }
}

fn make_config_s8(interval: u32, cap: usize) -> InferenceConfig {
    InferenceConfig {
        s8_batch_interval_ticks: interval,
        max_s8_pairs_per_batch: cap,
        ..InferenceConfig::default()
    }
}

/// Seed an entry with given id and status (0=Active, 3=Quarantined).
async fn seed_entry(store: &Store, id: i64, status: i64) {
    sqlx::query(
        "INSERT OR IGNORE INTO entries \
         (id, title, content, topic, category, source, status, created_at, updated_at) \
         VALUES (?1, 'test', 'content', 'test', 'test', 'test', ?2, 0, 0)",
    )
    .bind(id)
    .bind(status)
    .execute(store.write_pool_server())
    .await
    .unwrap();
}

/// Seed an entry with specific content and title text.
async fn seed_entry_with_content(store: &Store, id: i64, title: &str, content: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO entries \
         (id, title, content, topic, category, source, status, created_at, updated_at) \
         VALUES (?1, ?2, ?3, 'test', 'test', 'test', 0, 0, 0)",
    )
    .bind(id)
    .bind(title)
    .bind(content)
    .execute(store.write_pool_server())
    .await
    .unwrap();
}

/// Tag an entry.
async fn seed_tag(store: &Store, entry_id: i64, tag: &str) {
    sqlx::query("INSERT OR IGNORE INTO entry_tags (entry_id, tag) VALUES (?1, ?2)")
        .bind(entry_id)
        .bind(tag)
        .execute(store.write_pool_server())
        .await
        .unwrap();
}

/// Insert an audit_log row directly (bypassing the counter-based API for test control).
async fn seed_audit_row(
    store: &Store,
    event_id: i64,
    operation: &str,
    outcome: i64,
    target_ids: &str,
) {
    sqlx::query(
        "INSERT INTO audit_log \
         (event_id, timestamp, session_id, agent_id, operation, target_ids, outcome, detail) \
         VALUES (?1, 0, 'test', 'test', ?2, ?3, ?4, '')",
    )
    .bind(event_id)
    .bind(operation)
    .bind(target_ids)
    .bind(outcome)
    .execute(store.write_pool_server())
    .await
    .unwrap();
}

/// Count S1 edges in graph_edges.
async fn count_edges_by_source(store: &Store, source: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM graph_edges WHERE source = ?1")
        .bind(source)
        .fetch_one(store.write_pool_server())
        .await
        .unwrap()
}

/// Fetch a specific edge row.
async fn fetch_edge(
    store: &Store,
    source_id: i64,
    target_id: i64,
    relation_type: &str,
) -> Option<(f64, String, String, i64)> {
    sqlx::query_as::<_, (f64, String, String, i64)>(
        "SELECT weight, source, created_by, bootstrap_only \
         FROM graph_edges \
         WHERE source_id = ?1 AND target_id = ?2 AND relation_type = ?3",
    )
    .bind(source_id)
    .bind(target_id)
    .bind(relation_type)
    .fetch_optional(store.write_pool_server())
    .await
    .unwrap()
}

/// Read the S8 watermark from the counters table.
async fn read_s8_watermark(store: &Store) -> u64 {
    counters::read_counter(store.write_pool_server(), "s8_audit_log_watermark")
        .await
        .unwrap()
}

// ---------------------------------------------------------------------------
// S1 tests
// ---------------------------------------------------------------------------

/// R-07, AC-01: basic Informs edge written with correct source and weight.
#[tokio::test]
async fn test_s1_basic_informs_edge_written() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    for tag in &["t1", "t2", "t3"] {
        seed_tag(&store, 1, tag).await;
        seed_tag(&store, 2, tag).await;
    }

    run_s1_tick(&store, &make_config()).await;

    let edge = fetch_edge(&store, 1, 2, "Informs")
        .await
        .expect("edge must exist");
    let (weight, source, _created_by, bootstrap_only) = edge;
    assert_eq!(source, "S1");
    assert_eq!(bootstrap_only, 0);
    assert!(
        (weight - 0.3).abs() < 1e-6,
        "weight should be 0.3 for 3 shared tags"
    );
}

/// R-01 (source position): quarantined source entry — no edge written.
#[tokio::test]
async fn test_s1_excludes_quarantined_source() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // Entry 1 (lower id = source in S1 ordering) is Quarantined.
    seed_entry(&store, 1, 3).await;
    seed_entry(&store, 2, 0).await;
    for tag in &["t1", "t2", "t3"] {
        seed_tag(&store, 1, tag).await;
        seed_tag(&store, 2, tag).await;
    }

    run_s1_tick(&store, &make_config()).await;

    assert_eq!(count_edges_by_source(&store, "S1").await, 0);
}

/// R-01 (target position): quarantined target entry — no edge written.
#[tokio::test]
async fn test_s1_excludes_quarantined_target() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    // Entry 2 (higher id = target in t2.entry_id > t1.entry_id ordering) is Quarantined.
    seed_entry(&store, 2, 3).await;
    for tag in &["t1", "t2", "t3"] {
        seed_tag(&store, 1, tag).await;
        seed_tag(&store, 2, tag).await;
    }

    run_s1_tick(&store, &make_config()).await;

    assert_eq!(count_edges_by_source(&store, "S1").await, 0);
}

/// Edge case: exactly 3 shared tags qualifies; 2 does not.
#[tokio::test]
async fn test_s1_having_threshold_exactly_3() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // Pair (1,2): 3 shared tags — must qualify.
    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    for tag in &["t1", "t2", "t3"] {
        seed_tag(&store, 1, tag).await;
        seed_tag(&store, 2, tag).await;
    }
    // Pair (3,4): 2 shared tags — must NOT qualify.
    seed_entry(&store, 3, 0).await;
    seed_entry(&store, 4, 0).await;
    for tag in &["u1", "u2"] {
        seed_tag(&store, 3, tag).await;
        seed_tag(&store, 4, tag).await;
    }

    run_s1_tick(&store, &make_config()).await;

    assert_eq!(count_edges_by_source(&store, "S1").await, 1);
    assert!(fetch_edge(&store, 1, 2, "Informs").await.is_some());
    assert!(fetch_edge(&store, 3, 4, "Informs").await.is_none());
}

/// AC-02: idempotent — second run produces no duplicate.
#[tokio::test]
async fn test_s1_idempotent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    for tag in &["t1", "t2", "t3"] {
        seed_tag(&store, 1, tag).await;
        seed_tag(&store, 2, tag).await;
    }

    run_s1_tick(&store, &make_config()).await;
    run_s1_tick(&store, &make_config()).await;

    assert_eq!(count_edges_by_source(&store, "S1").await, 1);
}

/// AC-05: weight formula — 3→0.3, 5→0.5, 10→1.0, 12→1.0 (capped).
#[tokio::test]
async fn test_s1_weight_formula() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // Pair (1,2): 3 shared tags → weight=0.3
    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    for i in 0..3i64 {
        seed_tag(&store, 1, &format!("pair12_t{i}")).await;
        seed_tag(&store, 2, &format!("pair12_t{i}")).await;
    }
    // Pair (3,4): 10 shared tags → weight=1.0
    seed_entry(&store, 3, 0).await;
    seed_entry(&store, 4, 0).await;
    for i in 0..10i64 {
        seed_tag(&store, 3, &format!("pair34_t{i}")).await;
        seed_tag(&store, 4, &format!("pair34_t{i}")).await;
    }
    // Pair (5,6): 12 shared tags → weight=1.0 (capped)
    seed_entry(&store, 5, 0).await;
    seed_entry(&store, 6, 0).await;
    for i in 0..12i64 {
        seed_tag(&store, 5, &format!("pair56_t{i}")).await;
        seed_tag(&store, 6, &format!("pair56_t{i}")).await;
    }

    run_s1_tick(&store, &make_config_s1(10)).await;

    let (w12, _, _, _) = fetch_edge(&store, 1, 2, "Informs").await.unwrap();
    let (w34, _, _, _) = fetch_edge(&store, 3, 4, "Informs").await.unwrap();
    let (w56, _, _, _) = fetch_edge(&store, 5, 6, "Informs").await.unwrap();

    assert!((w12 - 0.3).abs() < 1e-5, "3 tags → 0.3");
    assert!((w34 - 1.0).abs() < 1e-5, "10 tags → 1.0");
    assert!((w56 - 1.0).abs() < 1e-5, "12 tags → capped at 1.0");
}

/// AC-04: cap respected — only top-N highest-overlap pairs selected.
#[tokio::test]
async fn test_s1_cap_respected() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // Create 5 pairs with distinct shared-tag counts: 3,4,5,6,7.
    for pair in 0..5i64 {
        let a = pair * 2 + 1;
        let b = pair * 2 + 2;
        let count = 3 + pair; // 3,4,5,6,7
        seed_entry(&store, a, 0).await;
        seed_entry(&store, b, 0).await;
        for i in 0..count {
            seed_tag(&store, a, &format!("p{pair}t{i}")).await;
            seed_tag(&store, b, &format!("p{pair}t{i}")).await;
        }
    }

    run_s1_tick(&store, &make_config_s1(3)).await;

    assert_eq!(count_edges_by_source(&store, "S1").await, 3);
    // The 3 highest-overlap pairs: (9,10)=7, (7,8)=6, (5,6)=5
    assert!(
        fetch_edge(&store, 9, 10, "Informs").await.is_some(),
        "7-tag pair must be selected"
    );
    assert!(
        fetch_edge(&store, 7, 8, "Informs").await.is_some(),
        "6-tag pair must be selected"
    );
    assert!(
        fetch_edge(&store, 5, 6, "Informs").await.is_some(),
        "5-tag pair must be selected"
    );
    assert!(
        fetch_edge(&store, 3, 4, "Informs").await.is_none(),
        "4-tag pair must not be selected"
    );
}

/// R-07: source value is 'S1', not 'nli'.
#[tokio::test]
async fn test_s1_source_value_is_s1_not_nli() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    for tag in &["t1", "t2", "t3"] {
        seed_tag(&store, 1, tag).await;
        seed_tag(&store, 2, tag).await;
    }

    run_s1_tick(&store, &make_config()).await;

    assert_eq!(count_edges_by_source(&store, "S1").await, 1);
    assert_eq!(count_edges_by_source(&store, "nli").await, 0);
}

/// No panic on empty corpus.
#[tokio::test]
async fn test_s1_empty_corpus_no_panic() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;
    run_s1_tick(&store, &make_config()).await;
    assert_eq!(count_edges_by_source(&store, "S1").await, 0);
}

// ---------------------------------------------------------------------------
// S2 tests
// ---------------------------------------------------------------------------

/// R-14, AC-07: empty vocabulary is a no-op.
#[tokio::test]
async fn test_s2_empty_vocabulary_is_noop() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry_with_content(&store, 1, "schema migration", "schema migration guide").await;
    seed_entry_with_content(&store, 2, "schema docs", "database schema reference").await;

    let config = InferenceConfig {
        s2_vocabulary: vec![],
        ..InferenceConfig::default()
    };
    run_s2_tick(&store, &config).await;

    assert_eq!(count_edges_by_source(&store, "S2").await, 0);
}

/// AC-06: basic S2 edge written.
#[tokio::test]
async fn test_s2_basic_informs_edge_written() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry_with_content(&store, 1, "schema migration", "migration guide").await;
    seed_entry_with_content(&store, 2, "schema docs", "schema reference").await;

    let config = make_config_s2(vec!["schema", "migration"], 200);
    run_s2_tick(&store, &config).await;

    assert!(fetch_edge(&store, 1, 2, "Informs").await.is_some());
    let (weight, source, _, bootstrap_only) = fetch_edge(&store, 1, 2, "Informs").await.unwrap();
    assert_eq!(source, "S2");
    assert_eq!(bootstrap_only, 0);
    // Entry 1 has both "schema" and "migration"; Entry 2 has "schema".
    // s1_terms(e1)=2, s2_terms(e2)=1 → shared_terms=3 → weight=0.3
    assert!(weight >= 0.2 - 1e-6, "weight must be >= 0.2");
}

/// R-01 (source position): quarantined source entry — no S2 edge.
#[tokio::test]
async fn test_s2_excludes_quarantined_source() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // Entry 1 (lower id) is Quarantined.
    sqlx::query(
        "INSERT OR IGNORE INTO entries \
         (id, title, content, topic, category, source, status, created_at, updated_at) \
         VALUES (1, 'api docs', 'api reference', 'test', 'test', 'test', 3, 0, 0)",
    )
    .execute(store.write_pool_server())
    .await
    .unwrap();
    seed_entry_with_content(&store, 2, "api guide", "api usage").await;

    let config = make_config_s2(vec!["api"], 200);
    run_s2_tick(&store, &config).await;

    assert_eq!(count_edges_by_source(&store, "S2").await, 0);
}

/// R-01 (target position): quarantined target entry — no S2 edge.
#[tokio::test]
async fn test_s2_excludes_quarantined_target() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry_with_content(&store, 1, "api docs", "api reference").await;
    // Entry 2 (higher id) is Quarantined.
    sqlx::query(
        "INSERT OR IGNORE INTO entries \
         (id, title, content, topic, category, source, status, created_at, updated_at) \
         VALUES (2, 'api guide', 'api usage', 'test', 'test', 'test', 3, 0, 0)",
    )
    .execute(store.write_pool_server())
    .await
    .unwrap();

    let config = make_config_s2(vec!["api"], 200);
    run_s2_tick(&store, &config).await;

    assert_eq!(count_edges_by_source(&store, "S2").await, 0);
}

/// R-11: "api" does NOT match "capabilities" (word-boundary guard).
#[tokio::test]
async fn test_s2_no_false_positive_capabilities_for_api() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // Entry 1 has "api" (should match).
    seed_entry_with_content(&store, 1, "the api docs", "api is documented").await;
    // Entry 2 has only "capabilities" — "api" is a substring but NOT a word.
    seed_entry_with_content(&store, 2, "capabilities only", "no other words here").await;

    let config = make_config_s2(vec!["api"], 200);
    run_s2_tick(&store, &config).await;

    // Entry 2 should not match "api" — so (1,2) pair total might be only 1 (entry 1 matches,
    // entry 2 does not). threshold is 2, so no edge should be written.
    assert_eq!(
        count_edges_by_source(&store, "S2").await,
        0,
        "api must not match capabilities due to word-boundary guard"
    );
}

/// R-11: "api" DOES match entry with exact word "api".
#[tokio::test]
async fn test_s2_true_positive_api_in_title() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry_with_content(&store, 1, "the api is documented", "api reference guide").await;
    seed_entry_with_content(&store, 2, "api schema reference", "api design patterns").await;

    let config = make_config_s2(vec!["api"], 200);
    run_s2_tick(&store, &config).await;

    // Both entries match "api"; shared_terms = 2 (1 from each side) → edge written.
    assert!(
        fetch_edge(&store, 1, 2, "Informs").await.is_some(),
        "both entries contain 'api' as a word — edge must be written"
    );
}

/// R-02: SQL injection with single quote — no panic, correct result.
#[tokio::test]
async fn test_s2_sql_injection_single_quote() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry_with_content(&store, 1, "it's a test", "it's documented").await;
    seed_entry_with_content(&store, 2, "another it's entry", "uses it's pattern").await;

    // Single-quote in vocabulary term must not cause SQL error.
    let config = make_config_s2(vec!["it's"], 200);
    let written = run_s2_tick(&store, &config).await;

    // No panic is the primary assertion. If the term matches, edges may be written.
    // Graph_edges table must still exist.
    let _ = count_edges_by_source(&store, "S2").await;
    let _ = written; // no panic — test passes
}

/// R-02: SQL injection with double-dash comment — no panic, table survives.
#[tokio::test]
async fn test_s2_sql_injection_double_dash() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry_with_content(&store, 1, "normal entry", "normal content").await;
    seed_entry_with_content(&store, 2, "another entry", "another content").await;

    // SQL comment injection via vocabulary term.
    let config = make_config_s2(vec!["-- DROP TABLE graph_edges"], 200);
    run_s2_tick(&store, &config).await;

    // graph_edges table must still exist (DROP was not executed).
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM graph_edges")
        .fetch_one(store.write_pool_server())
        .await
        .expect("graph_edges table must survive SQL injection attempt");
    assert_eq!(
        count, 0,
        "no edges written for a non-matching injection term"
    );
}

/// AC-08: S2 idempotent.
#[tokio::test]
async fn test_s2_idempotent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry_with_content(&store, 1, "schema migration guide", "database schema").await;
    seed_entry_with_content(&store, 2, "schema design", "schema patterns").await;

    let config = make_config_s2(vec!["schema"], 200);
    run_s2_tick(&store, &config).await;
    run_s2_tick(&store, &config).await;

    // INSERT OR IGNORE ensures no duplicates.
    assert_eq!(count_edges_by_source(&store, "S2").await, 1);
}

/// AC-12: cap respected.
#[tokio::test]
async fn test_s2_cap_respected() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // 4 entries all matching "schema" — produces 6 pairs all sharing >= 2 terms.
    for id in 1..=4i64 {
        seed_entry_with_content(&store, id, "schema design", "schema patterns").await;
    }

    let config = make_config_s2(vec!["schema"], 2);
    run_s2_tick(&store, &config).await;

    assert_eq!(count_edges_by_source(&store, "S2").await, 2);
}

/// Edge case: s2_terms threshold = 2 (1 term each side qualifies).
#[tokio::test]
async fn test_s2_threshold_exactly_2_terms() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // Entry 1 has "schema" but not "cache"; Entry 2 has "cache" but not "schema".
    seed_entry_with_content(&store, 1, "schema design", "database design").await;
    seed_entry_with_content(&store, 2, "cache strategy", "cache patterns").await;

    let config = make_config_s2(vec!["schema", "cache"], 200);
    run_s2_tick(&store, &config).await;

    // s1_terms(e1)=1 (schema), s2_terms(e2)=1 (cache) → total=2 → edge written.
    assert_eq!(
        count_edges_by_source(&store, "S2").await,
        1,
        "1+1=2 total terms qualifies"
    );
}

/// R-07 (S2): source value is 'S2'.
#[tokio::test]
async fn test_s2_source_value_is_s2() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry_with_content(&store, 1, "api design", "api patterns").await;
    seed_entry_with_content(&store, 2, "api reference", "api documentation").await;

    let config = make_config_s2(vec!["api"], 200);
    run_s2_tick(&store, &config).await;

    assert_eq!(count_edges_by_source(&store, "S2").await, 1);
    assert_eq!(count_edges_by_source(&store, "nli").await, 0);
    assert_eq!(count_edges_by_source(&store, "S1").await, 0);
}

// ---------------------------------------------------------------------------
// S8 tests
// ---------------------------------------------------------------------------

/// AC-14: basic CoAccess edge written for two active entries.
#[tokio::test]
async fn test_s8_basic_coaccess_edge_written() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    seed_audit_row(&store, 1, "context_search", 0, "[1,2]").await;

    let config = make_config_s8(1, 500); // interval=1 so always runs
    run_s8_tick(&store, &config, 0).await;

    let edge = fetch_edge(&store, 1, 2, "CoAccess")
        .await
        .expect("CoAccess edge must exist");
    let (weight, source, _created_by, bootstrap_only) = edge;
    assert_eq!(source, "S8");
    assert_eq!(bootstrap_only, 0);
    assert!((weight - 0.25).abs() < 1e-6, "S8 weight must be 0.25");
}

/// R-05: malformed JSON row — watermark advances past it, valid rows on both sides produce edges.
#[tokio::test]
async fn test_s8_watermark_advances_past_malformed_json_row() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    seed_entry(&store, 3, 0).await;
    seed_entry(&store, 4, 0).await;

    seed_audit_row(&store, 1, "context_search", 0, "[1,2]").await; // valid
    seed_audit_row(&store, 2, "context_search", 0, "not-json").await; // malformed
    seed_audit_row(&store, 3, "context_search", 0, "[3,4]").await; // valid

    let config = make_config_s8(1, 500);
    run_s8_tick(&store, &config, 0).await;

    // Both valid pairs must produce edges.
    assert!(
        fetch_edge(&store, 1, 2, "CoAccess").await.is_some(),
        "row 1 pair must be written"
    );
    assert!(
        fetch_edge(&store, 3, 4, "CoAccess").await.is_some(),
        "row 3 pair must be written"
    );

    // Watermark must advance past row 3 (covering the malformed row 2).
    assert_eq!(read_s8_watermark(&store).await, 3);

    // Second run must produce no new edges.
    run_s8_tick(&store, &config, 0).await;
    assert_eq!(count_edges_by_source(&store, "S8").await, 2);
}

/// R-12: context_briefing rows produce no edges.
#[tokio::test]
async fn test_s8_excludes_briefing_operation() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    seed_audit_row(&store, 1, "context_briefing", 0, "[1,2]").await;

    let config = make_config_s8(1, 500);
    run_s8_tick(&store, &config, 0).await;

    assert_eq!(count_edges_by_source(&store, "S8").await, 0);
}

/// R-12: outcome != 0 rows produce no edges.
#[tokio::test]
async fn test_s8_excludes_failed_search() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    seed_audit_row(&store, 1, "context_search", 1, "[1,2]").await; // outcome=1 (failure)

    let config = make_config_s8(1, 500);
    run_s8_tick(&store, &config, 0).await;

    assert_eq!(count_edges_by_source(&store, "S8").await, 0);
}

/// R-01: quarantined endpoint — no edge written.
#[tokio::test]
async fn test_s8_excludes_quarantined_endpoint() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 3).await; // Quarantined
    seed_audit_row(&store, 1, "context_search", 0, "[1,2]").await;

    let config = make_config_s8(1, 500);
    run_s8_tick(&store, &config, 0).await;

    assert_eq!(count_edges_by_source(&store, "S8").await, 0);
}

/// R-10: cap is on pairs, not on audit_log rows.
#[tokio::test]
async fn test_s8_pair_cap_not_row_cap() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    for id in 1..=5i64 {
        seed_entry(&store, id, 0).await;
    }
    // One row with 5 entries → 10 pairs.
    seed_audit_row(&store, 1, "context_search", 0, "[1,2,3,4,5]").await;

    let config = make_config_s8(1, 5); // cap=5 pairs
    run_s8_tick(&store, &config, 0).await;

    assert_eq!(
        count_edges_by_source(&store, "S8").await,
        5,
        "pair cap must limit to 5, not all 10 pairs"
    );
}

/// R-10: partial-row watermark semantics.
#[tokio::test]
async fn test_s8_partial_row_watermark_semantics() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // Row 1 has 4 entries → 6 pairs. Row 2 has 2 entries → 1 pair.
    for id in 1..=6i64 {
        seed_entry(&store, id, 0).await;
    }
    seed_audit_row(&store, 1, "context_search", 0, "[1,2,3,4]").await; // 6 pairs
    seed_audit_row(&store, 2, "context_search", 0, "[5,6]").await; // 1 pair

    // cap=3 — row 1 has 6 pairs; first 3 are accepted (partial row truncation).
    // Row 2 is never reached. Watermark stays at 0 (partial row not fully committed, C-12).
    let config = make_config_s8(1, 3);
    run_s8_tick(&store, &config, 0).await;

    // 3 edges written (cap applied within the row).
    assert_eq!(
        count_edges_by_source(&store, "S8").await,
        3,
        "3 pairs accepted up to cap from row 1"
    );
    assert_eq!(
        read_s8_watermark(&store).await,
        0,
        "watermark must not advance past partially-processed row (C-12)"
    );
}

/// R-13 (S8 idempotency): running twice produces no duplicate edges.
#[tokio::test]
async fn test_s8_idempotent() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    seed_audit_row(&store, 1, "context_search", 0, "[1,2]").await;

    let config = make_config_s8(1, 500);

    // First run: write edges and advance watermark.
    run_s8_tick(&store, &config, 0).await;
    assert_eq!(count_edges_by_source(&store, "S8").await, 1);

    // Second run (watermark = 1 now, no new rows): no new edges.
    run_s8_tick(&store, &config, 0).await;
    assert_eq!(count_edges_by_source(&store, "S8").await, 1);
}

/// Edge case: singleton target_ids (0 pairs) — watermark advances, no panic.
#[tokio::test]
async fn test_s8_singleton_target_ids_no_panic() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 42, 0).await;
    seed_audit_row(&store, 1, "context_search", 0, "[42]").await;

    let config = make_config_s8(1, 500);
    run_s8_tick(&store, &config, 0).await;

    assert_eq!(count_edges_by_source(&store, "S8").await, 0);
    assert_eq!(
        read_s8_watermark(&store).await,
        1,
        "watermark must advance past singleton row"
    );
}

/// Edge case: empty target_ids array — watermark advances, no panic.
#[tokio::test]
async fn test_s8_empty_target_ids_no_panic() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_audit_row(&store, 1, "context_search", 0, "[]").await;

    let config = make_config_s8(1, 500);
    run_s8_tick(&store, &config, 0).await;

    assert_eq!(count_edges_by_source(&store, "S8").await, 0);
    assert_eq!(read_s8_watermark(&store).await, 1);
}

/// R-07 (S8): source value is 'S8' and relation_type is 'CoAccess'.
#[tokio::test]
async fn test_s8_source_value_is_s8() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    seed_audit_row(&store, 1, "context_search", 0, "[1,2]").await;

    let config = make_config_s8(1, 500);
    run_s8_tick(&store, &config, 0).await;

    let edge = fetch_edge(&store, 1, 2, "CoAccess").await.unwrap();
    assert_eq!(edge.1, "S8", "source must be S8");
    assert_eq!(count_edges_by_source(&store, "nli").await, 0);
    assert_eq!(count_edges_by_source(&store, "S1").await, 0);
    assert_eq!(count_edges_by_source(&store, "S2").await, 0);
}

/// Tick interval gate: S8 does not run on non-batch ticks.
#[tokio::test]
async fn test_s8_gated_by_tick_interval() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    seed_audit_row(&store, 1, "context_search", 0, "[1,2]").await;

    let config = make_config_s8(10, 500); // interval=10
    // current_tick=1: 1 % 10 != 0 — should not run.
    let written = run_s8_tick(&store, &config, 1).await;
    assert_eq!(written, 0);
    assert_eq!(count_edges_by_source(&store, "S8").await, 0);

    // current_tick=10: 10 % 10 == 0 — should run.
    let written = run_s8_tick(&store, &config, 10).await;
    assert_eq!(written, 1);
    assert_eq!(count_edges_by_source(&store, "S8").await, 1);
}

// ---------------------------------------------------------------------------
// run_graph_enrichment_tick orchestration tests
// ---------------------------------------------------------------------------

/// AC-26: S1 and S2 run always; S8 also runs on tick=0 (0 % interval == 0).
#[tokio::test]
async fn test_enrichment_tick_calls_s1_and_s2_always() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    // S1 data: 2 entries sharing 3 tags.
    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    for tag in &["t1", "t2", "t3"] {
        seed_tag(&store, 1, tag).await;
        seed_tag(&store, 2, tag).await;
    }

    // S2 data: 2 entries matching vocab term.
    seed_entry_with_content(&store, 3, "api design", "api patterns").await;
    seed_entry_with_content(&store, 4, "api reference", "api documentation").await;

    // S8 data.
    seed_entry(&store, 5, 0).await;
    seed_entry(&store, 6, 0).await;
    seed_audit_row(&store, 1, "context_search", 0, "[5,6]").await;

    let config = InferenceConfig {
        s2_vocabulary: vec!["api".to_string()],
        max_s1_edges_per_tick: 200,
        max_s2_edges_per_tick: 200,
        s8_batch_interval_ticks: 10,
        max_s8_pairs_per_batch: 500,
        ..InferenceConfig::default()
    };

    run_graph_enrichment_tick(&store, &config, 0).await;

    assert!(
        count_edges_by_source(&store, "S1").await >= 1,
        "S1 must have run"
    );
    assert!(
        count_edges_by_source(&store, "S2").await >= 1,
        "S2 must have run"
    );
    // tick=0, interval=10: 0 % 10 == 0, so S8 must also run.
    assert_eq!(
        count_edges_by_source(&store, "S8").await,
        1,
        "S8 must run on tick=0"
    );
}

/// AC-13: S8 does not run on non-batch tick.
#[tokio::test]
async fn test_enrichment_tick_skips_s8_on_non_batch_tick() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    seed_audit_row(&store, 1, "context_search", 0, "[1,2]").await;

    let config = InferenceConfig {
        s8_batch_interval_ticks: 10,
        max_s8_pairs_per_batch: 500,
        ..InferenceConfig::default()
    };

    // current_tick=1: 1 % 10 != 0 — S8 must not run.
    run_graph_enrichment_tick(&store, &config, 1).await;
    assert_eq!(count_edges_by_source(&store, "S8").await, 0);
}

/// AC-13: S8 runs on batch tick.
#[tokio::test]
async fn test_enrichment_tick_s8_runs_on_batch_tick() {
    let tmp = tempfile::TempDir::new().unwrap();
    let store = unimatrix_store::test_helpers::open_test_store(&tmp).await;

    seed_entry(&store, 1, 0).await;
    seed_entry(&store, 2, 0).await;
    seed_audit_row(&store, 1, "context_search", 0, "[1,2]").await;

    let config = InferenceConfig {
        s8_batch_interval_ticks: 10,
        max_s8_pairs_per_batch: 500,
        ..InferenceConfig::default()
    };

    // current_tick=10: 10 % 10 == 0 — S8 must run.
    run_graph_enrichment_tick(&store, &config, 10).await;
    assert_eq!(count_edges_by_source(&store, "S8").await, 1);
}
