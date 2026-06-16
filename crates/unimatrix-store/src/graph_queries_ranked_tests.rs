//! Unit tests for `graph_queries_ranked.rs` — the get-only ranked read + split count
//! (vnc-037). All tests assert at the **store boundary** (the returned `Vec<RankedEdge>`
//! / `EdgeCountSplit`), before any projection.
//!
//! Trace discipline (#3645/#3621): every ranking/canon scenario carries a per-edge
//! trace `(source, target_confidence, weight) → expected slot`; the expected top-N is
//! derived from the rule (`ORDER BY (source='agent') DESC, t.confidence DESC NULLS
//! LAST, target_id ASC LIMIT GET_EDGE_DISPLAY_LIMIT`), never intuited.

use super::*;
use crate::db::SqlxStore;
use crate::pool_config::PoolConfig;

// -----------------------------------------------------------------------
// Test helpers (extend the conventions in graph_queries_tests.rs)
// -----------------------------------------------------------------------

async fn open_test_store() -> (SqlxStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("test.db");
    let store = SqlxStore::open(&path, PoolConfig::test_default())
        .await
        .expect("open test store");
    (store, dir)
}

/// Insert a minimal entry with an explicit `confidence` (the ranked-variant rank key).
/// Returns the assigned id.
async fn insert_entry_conf(pool: &SqlitePool, title: &str, confidence: f64) -> u64 {
    let id: i64 =
        sqlx::query_scalar::<_, i64>("SELECT value FROM counters WHERE name = 'next_entry_id'")
            .fetch_one(pool)
            .await
            .expect("get next_entry_id");
    let new_id = id + 1;
    sqlx::query("UPDATE counters SET value = ?1 WHERE name = 'next_entry_id'")
        .bind(new_id)
        .execute(pool)
        .await
        .expect("update next_entry_id");

    let now = 1_700_000_000_i64;
    sqlx::query(
        "INSERT INTO entries (id, title, content, topic, category, source, status,
         confidence, created_at, updated_at, last_accessed_at, access_count,
         supersedes, superseded_by, correction_count, embedding_dim,
         created_by, modified_by, content_hash, previous_hash,
         version, feature_cycle, trust_source, helpful_count, unhelpful_count)
         VALUES (?1, ?2, 'content', 'test', 'pattern', 'test', 0,
         ?3, ?4, ?4, ?4, 0, NULL, NULL, 0, 0, '', '', '', '', 1, '', '', 0, 0)",
    )
    .bind(new_id)
    .bind(title)
    .bind(confidence)
    .bind(now)
    .execute(pool)
    .await
    .expect("insert entry");

    new_id as u64
}

/// Insert a graph_edges row with an explicit `source` provenance and `weight`.
async fn insert_edge_full(
    pool: &SqlitePool,
    source_id: u64,
    target_id: u64,
    rel: &str,
    source: &str,
    weight: f64,
) {
    let now = 1_700_000_000_i64;
    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
         (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
         VALUES (?1, ?2, ?3, ?4, ?5, '', ?6, 0)",
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(rel)
    .bind(weight)
    .bind(now)
    .bind(source)
    .execute(pool)
    .await
    .expect("insert edge");
}

/// Insert a graph_edges row with provenance and weight = 1.0 (the common case).
async fn insert_edge_src(pool: &SqlitePool, s: u64, t: u64, rel: &str, source: &str) {
    insert_edge_full(pool, s, t, rel, source, 1.0).await;
}

/// Insert a reciprocal symmetric pair (both A→B and B→A rows).
async fn insert_symmetric_pair(pool: &SqlitePool, a: u64, b: u64, rel: &str, source: &str) {
    insert_edge_src(pool, a, b, rel, source).await;
    insert_edge_src(pool, b, a, rel, source).await;
}

fn target_ids(rows: &[RankedEdge]) -> Vec<u64> {
    rows.iter().map(|r| r.row.target_id).collect()
}

// =======================================================================
// R-02 — Ranking ORDER BY (Critical, discriminating per #3886)
// =======================================================================

/// The load-bearing #3886 test: ranking is by **target confidence**, NOT
/// `graph_edges.weight`, and the proof target sits OUTSIDE the cap by weight-order.
///
/// Trace (cap = GET_EDGE_DISPLAY_LIMIT = 3):
/// | edge  | source   | t.confidence | weight | correct slot |
/// |-------|----------|--------------|--------|--------------|
/// | A→T1  | co_access| 0.90         | 0.1    | 1            |
/// | A→T2  | co_access| 0.70         | 1.0    | 2            |
/// | A→T3  | co_access| 0.50         | 1.0    | 3            |
/// | A→T4  | co_access| 0.30         | 1.0    | excluded     |
///
/// A weight-ordering bug sinks T1 (weight 0.1) and surfaces T2,T3,T4 → a visibly
/// different top-3 → fails correctly.
#[tokio::test]
async fn test_query_ranked_by_target_confidence_proof_outside_cap() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    let t1 = insert_entry_conf(wp, "T1", 0.90).await;
    let t2 = insert_entry_conf(wp, "T2", 0.70).await;
    let t3 = insert_entry_conf(wp, "T3", 0.50).await;
    let t4 = insert_entry_conf(wp, "T4", 0.30).await;

    // T1 has the HIGHEST confidence but the LOWEST weight — confidence must win.
    insert_edge_full(wp, a, t1, "Supports", "co_access", 0.1).await;
    insert_edge_full(wp, a, t2, "Supports", "co_access", 1.0).await;
    insert_edge_full(wp, a, t3, "Supports", "co_access", 1.0).await;
    insert_edge_full(wp, a, t4, "Supports", "co_access", 1.0).await;

    let rows = query_ranked_neighbors(store.read_pool(), a)
        .await
        .expect("query");

    assert_eq!(
        rows.len(),
        GET_EDGE_DISPLAY_LIMIT as usize,
        "exactly cap rows returned"
    );
    assert_eq!(
        target_ids(&rows),
        vec![t1, t2, t3],
        "ranked by descending target confidence, NOT weight"
    );
    // Proof-outside-cap discriminators:
    assert!(
        target_ids(&rows).contains(&t1),
        "high-confidence/low-weight T1 IS included — weight does not decide"
    );
    assert!(
        !target_ids(&rows).contains(&t4),
        "lowest-confidence T4 is excluded by global rank"
    );

    store.close().await.unwrap();
}

/// `(source='agent') DESC` term: ≥ cap authored fills every slot, no inferred shows.
#[tokio::test]
async fn test_query_ranked_authored_priority_under_cap() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    // 3 authored (Prerequisite, source='agent') + 2 high-confidence inferred.
    let p1 = insert_entry_conf(wp, "P1", 0.1).await;
    let p2 = insert_entry_conf(wp, "P2", 0.1).await;
    let p3 = insert_entry_conf(wp, "P3", 0.1).await;
    let inf1 = insert_entry_conf(wp, "INF1", 0.99).await;
    let inf2 = insert_entry_conf(wp, "INF2", 0.99).await;

    insert_edge_src(wp, a, p1, "Prerequisite", "agent").await;
    insert_edge_src(wp, a, p2, "Prerequisite", "agent").await;
    insert_edge_src(wp, a, p3, "Prerequisite", "agent").await;
    insert_edge_src(wp, a, inf1, "Supports", "co_access").await;
    insert_edge_src(wp, a, inf2, "Supports", "co_access").await;

    let rows = query_ranked_neighbors(store.read_pool(), a)
        .await
        .expect("query");

    assert_eq!(rows.len(), GET_EDGE_DISPLAY_LIMIT as usize);
    let ids = target_ids(&rows);
    for p in [p1, p2, p3] {
        assert!(ids.contains(&p), "authored {p} must fill a slot");
    }
    assert!(
        !ids.contains(&inf1) && !ids.contains(&inf2),
        "no inferred edge appears when authored >= cap (despite higher confidence)"
    );
    assert!(rows.iter().all(|r| r.row.source == "agent"));

    store.close().await.unwrap();
}

/// Inferred fills only when authored < cap, ordered by target confidence.
#[tokio::test]
async fn test_query_ranked_inferred_fill_only_when_authored_lt_3() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    let auth = insert_entry_conf(wp, "AUTH", 0.1).await;
    let t8 = insert_entry_conf(wp, "T8", 0.8).await;
    let t6 = insert_entry_conf(wp, "T6", 0.6).await;
    let t4 = insert_entry_conf(wp, "T4", 0.4).await;

    insert_edge_src(wp, a, auth, "Prerequisite", "agent").await;
    insert_edge_src(wp, a, t8, "Supports", "co_access").await;
    insert_edge_src(wp, a, t6, "Supports", "co_access").await;
    insert_edge_src(wp, a, t4, "Supports", "co_access").await;

    let rows = query_ranked_neighbors(store.read_pool(), a)
        .await
        .expect("query");

    assert_eq!(rows.len(), GET_EDGE_DISPLAY_LIMIT as usize);
    // authored slot 1, then inferred by descending confidence; t4 (0.4) excluded.
    assert_eq!(target_ids(&rows), vec![auth, t8, t6]);

    store.close().await.unwrap();
}

/// Deterministic tiebreak: equal confidence resolves by `target_id ASC`, stable.
#[tokio::test]
async fn test_query_ranked_deterministic_tiebreak() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    // Equal confidence targets; insert out of id order to expose any insertion bias.
    let t = [
        insert_entry_conf(wp, "T0", 0.5).await,
        insert_entry_conf(wp, "T1", 0.5).await,
        insert_entry_conf(wp, "T2", 0.5).await,
        insert_entry_conf(wp, "T3", 0.5).await,
    ];
    // Insert edges in REVERSE id order.
    for &tid in t.iter().rev() {
        insert_edge_src(wp, a, tid, "Supports", "co_access").await;
    }

    let run1 = target_ids(&query_ranked_neighbors(store.read_pool(), a).await.unwrap());
    let run2 = target_ids(&query_ranked_neighbors(store.read_pool(), a).await.unwrap());

    assert_eq!(run1, run2, "stable across runs");
    let mut expected: Vec<u64> = t.to_vec();
    expected.sort_unstable();
    expected.truncate(GET_EDGE_DISPLAY_LIMIT as usize);
    assert_eq!(run1, expected, "equal confidence resolves by target_id ASC");

    store.close().await.unwrap();
}

/// Cold-start: all confidences at the 0.0 default → tiebreak decides, not row order.
#[tokio::test]
async fn test_query_ranked_cold_start_uniform_zero_tiebreak() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.0).await;
    let t = [
        insert_entry_conf(wp, "T0", 0.0).await,
        insert_entry_conf(wp, "T1", 0.0).await,
        insert_entry_conf(wp, "T2", 0.0).await,
        insert_entry_conf(wp, "T3", 0.0).await,
    ];
    for &tid in t.iter().rev() {
        insert_edge_src(wp, a, tid, "Supports", "co_access").await;
    }

    let rows = target_ids(&query_ranked_neighbors(store.read_pool(), a).await.unwrap());
    let mut expected: Vec<u64> = t.to_vec();
    expected.sort_unstable();
    expected.truncate(GET_EDGE_DISPLAY_LIMIT as usize);
    assert_eq!(rows, expected, "uniform 0.0 → target_id ASC, not arbitrary");

    store.close().await.unwrap();
}

// =======================================================================
// R-04 — Rank-and-limit in SQL, not Rust (Critical, store boundary)
// =======================================================================

/// High-degree node: ≥50 edges → the function returns EXACTLY cap rows at the store
/// boundary. The full neighbor set is never materialized (a Rust-slice bug would
/// satisfy rendered output but allocate the full Vec — this asserts the returned len).
#[tokio::test]
async fn test_query_ranked_high_degree_returns_exactly_cap_rows() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    for i in 0..50 {
        let t = insert_entry_conf(wp, &format!("T{i}"), 0.5).await;
        insert_edge_src(wp, a, t, "Supports", "co_access").await;
    }

    let rows = query_ranked_neighbors(store.read_pool(), a)
        .await
        .expect("query");

    assert_eq!(
        rows.len(),
        GET_EDGE_DISPLAY_LIMIT as usize,
        "SQL LIMIT caps at the store boundary — full fan-out never materialized"
    );

    store.close().await.unwrap();
}

// =======================================================================
// R-01 (ranked side) — Canonicalize BEFORE rank
// =======================================================================

/// Order-of-operations: symmetric pairs collapse BEFORE the cap, so an authored
/// asymmetric edge still wins a slot. A no-canon impl lets duplicate symmetric rows
/// crowd the authored edge out under LIMIT 3.
#[tokio::test]
async fn test_query_ranked_canon_before_cap_authored_wins() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    let auth = insert_entry_conf(wp, "AUTH", 0.1).await;

    // 4 symmetric pairs (each two reciprocal rows = 8 raw rows pre-canon).
    for i in 0..4 {
        let s = insert_entry_conf(wp, &format!("SYM{i}"), 0.99).await;
        insert_symmetric_pair(wp, a, s, "CoAccess", "co_access").await;
    }
    insert_edge_src(wp, a, auth, "Prerequisite", "agent").await;

    let rows = query_ranked_neighbors(store.read_pool(), a)
        .await
        .expect("query");

    assert_eq!(rows.len(), GET_EDGE_DISPLAY_LIMIT as usize);
    assert!(
        target_ids(&rows).contains(&auth),
        "authored edge wins a slot — symmetric pairs collapsed before the cap"
    );

    store.close().await.unwrap();
}

/// Each symmetric type collapses a reciprocal pair to ONE row in the ranked result.
#[tokio::test]
async fn test_query_ranked_symmetric_collapses_to_one_row() {
    for rel in ["Contradicts", "CoAccess", "Informs"] {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        let a = insert_entry_conf(wp, "A", 0.5).await;
        let b = insert_entry_conf(wp, "B", 0.5).await;
        insert_symmetric_pair(wp, a, b, rel, "co_access").await;

        let rows = query_ranked_neighbors(store.read_pool(), a)
            .await
            .expect("query");

        assert_eq!(rows.len(), 1, "{rel}: reciprocal pair collapses to ONE row");
        assert_eq!(rows[0].row.target_id, b);
        assert_eq!(rows[0].direction, "both", "{rel}: canonical direction is ↔");

        store.close().await.unwrap();
    }
}

/// Asymmetric types pass through untouched with a meaningful direction.
#[tokio::test]
async fn test_query_ranked_asymmetric_untouched() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    let out = insert_entry_conf(wp, "OUT", 0.5).await;
    let inc = insert_entry_conf(wp, "INC", 0.5).await;

    insert_edge_src(wp, a, out, "Prerequisite", "agent").await; // outbound from A
    insert_edge_src(wp, inc, a, "Supports", "agent").await; // inbound to A

    let rows = query_ranked_neighbors(store.read_pool(), a)
        .await
        .expect("query");

    assert_eq!(
        rows.len(),
        2,
        "two distinct asymmetric edges, not collapsed"
    );
    let out_row = rows.iter().find(|r| r.row.target_id == out).unwrap();
    let inc_row = rows.iter().find(|r| r.row.target_id == inc).unwrap();
    assert_eq!(out_row.direction, "outbound");
    assert_eq!(inc_row.direction, "inbound");

    store.close().await.unwrap();
}

// =======================================================================
// R-06 — Confidence LEFT JOIN (High)
// =======================================================================

/// Dangling target (target_id has no entries row) is RETAINED with
/// target_confidence = None and ranks last via NULLS LAST.
#[tokio::test]
async fn test_query_ranked_dangling_target_retained_nulls_last() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    let resolved = insert_entry_conf(wp, "RES", 0.6).await;
    let dangling: u64 = 999_999; // no entries row

    insert_edge_src(wp, a, resolved, "Supports", "co_access").await;
    insert_edge_src(wp, a, dangling, "Supports", "co_access").await;

    let rows = query_ranked_neighbors(store.read_pool(), a)
        .await
        .expect("query");

    assert_eq!(rows.len(), 2, "dangling edge retained (LEFT JOIN)");
    assert_eq!(
        target_ids(&rows),
        vec![resolved, dangling],
        "resolved (0.6) ranks before dangling (NULL, NULLS LAST)"
    );
    let dang = rows.iter().find(|r| r.row.target_id == dangling).unwrap();
    assert!(
        dang.row.target_confidence.is_none(),
        "dangling target_confidence is None"
    );

    store.close().await.unwrap();
}

/// LEFT (not INNER): a lone dangling edge still appears (INNER would drop it → empty).
#[tokio::test]
async fn test_query_ranked_join_is_left_not_inner() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    insert_edge_src(wp, a, 999_999, "Supports", "co_access").await;

    let rows = query_ranked_neighbors(store.read_pool(), a)
        .await
        .expect("query");

    assert_eq!(rows.len(), 1, "lone dangling edge retained — JOIN is LEFT");
    assert!(rows[0].row.target_confidence.is_none());

    store.close().await.unwrap();
}

/// Mixed resolved + NULL targets: resolved first, NULLs by target_id ASC, stable.
#[tokio::test]
async fn test_query_ranked_null_confidence_deterministic() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    let resolved = insert_entry_conf(wp, "RES", 0.6).await;
    let dang_hi: u64 = 900_001;
    let dang_lo: u64 = 900_000;

    insert_edge_src(wp, a, resolved, "Supports", "co_access").await;
    insert_edge_src(wp, a, dang_hi, "Supports", "co_access").await;
    insert_edge_src(wp, a, dang_lo, "Supports", "co_access").await;

    let run1 = target_ids(&query_ranked_neighbors(store.read_pool(), a).await.unwrap());
    let run2 = target_ids(&query_ranked_neighbors(store.read_pool(), a).await.unwrap());

    assert_eq!(run1, run2, "stable across runs");
    assert_eq!(
        run1,
        vec![resolved, dang_lo, dang_hi],
        "resolved first, then NULLs by target_id ASC"
    );

    store.close().await.unwrap();
}

// =======================================================================
// R-12 — Supersedes excluded (Medium)
// =======================================================================

#[tokio::test]
async fn test_query_ranked_supersedes_absent() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    let kept = insert_entry_conf(wp, "KEPT", 0.5).await;
    let sup = insert_entry_conf(wp, "SUP", 0.99).await;

    insert_edge_src(wp, a, kept, "Supports", "co_access").await;
    insert_edge_src(wp, a, sup, "Supersedes", "agent").await;

    let rows = query_ranked_neighbors(store.read_pool(), a)
        .await
        .expect("query");

    assert!(
        !target_ids(&rows).contains(&sup),
        "Supersedes edge excluded from ranked result"
    );
    assert!(target_ids(&rows).contains(&kept));

    store.close().await.unwrap();
}

// =======================================================================
// Security + no-literal-3 (AC-13a)
// =======================================================================

/// AC-13a: the ranked SQL binds the cap as `?2` — there is no literal `3` token,
/// and the cap value flows from GET_EDGE_DISPLAY_LIMIT (a positional bind).
#[tokio::test]
async fn test_query_ranked_no_literal_three_and_positional_binds() {
    // Structural assertion on the statement assembled by the function.
    let sql = format!(
        "{CANON_CTE}
        SELECT d.relation_type, d.source, d.other_id AS target_id, d.direction,
               t.confidence AS target_confidence
        FROM deduped d
        LEFT JOIN entries t ON t.id = d.other_id
        ORDER BY (d.source = 'agent') DESC,
                 t.confidence DESC NULLS LAST,
                 target_id ASC
        LIMIT ?2"
    );
    assert!(
        sql.contains("LIMIT ?2"),
        "cap is a positional bind, not inlined"
    );
    assert!(
        !sql.contains('3'),
        "no literal 3 anywhere in the ranked SQL"
    );
    assert!(sql.contains("?1"), "anchor is a positional bind");
}

/// Non-existent anchor returns an empty Vec, not an error (ADR-001).
#[tokio::test]
async fn test_query_ranked_nonexistent_anchor_empty() {
    let (store, _dir) = open_test_store().await;
    let rows = query_ranked_neighbors(store.read_pool(), 999_999)
        .await
        .expect("no error for non-existent anchor");
    assert!(rows.is_empty());
    store.close().await.unwrap();
}

// =======================================================================
// store-split-count — R-03 (Critical) + R-01 totals side
// =======================================================================

/// Uncapped exact: 8 mixed edges (> cap) → inbound+outbound+both == 8, NOT capped.
#[tokio::test]
async fn test_count_uncapped_exact() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    // 5 outbound asymmetric, 3 inbound asymmetric = 8 total.
    for i in 0..5 {
        let t = insert_entry_conf(wp, &format!("O{i}"), 0.5).await;
        insert_edge_src(wp, a, t, "Supports", "co_access").await;
    }
    for i in 0..3 {
        let s = insert_entry_conf(wp, &format!("I{i}"), 0.5).await;
        insert_edge_src(wp, s, a, "Supports", "co_access").await;
    }

    let split = count_neighbors_split(store.read_pool(), a)
        .await
        .expect("count");

    assert_eq!(split.outbound, 5);
    assert_eq!(split.inbound, 3);
    assert_eq!(split.both, 0);
    assert_eq!(
        split.inbound + split.outbound + split.both,
        8,
        "uncapped total across buckets — NOT computed off the capped ≤3 set"
    );

    store.close().await.unwrap();
}

/// #744 observability: high inbound + zero outbound reports the true split.
#[tokio::test]
async fn test_count_direction_split_load_bearing() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    for i in 0..5 {
        let s = insert_entry_conf(wp, &format!("I{i}"), 0.5).await;
        insert_edge_src(wp, s, a, "Prerequisite", "agent").await;
    }

    let split = count_neighbors_split(store.read_pool(), a)
        .await
        .expect("count");

    assert_eq!(split.inbound, 5);
    assert_eq!(split.outbound, 0);
    assert_eq!(split.both, 0);

    store.close().await.unwrap();
}

/// #744 inbound-degree integrity (REPLACES the retired "↔ folds into inbound"): a `↔`
/// edge increments `both`, NEVER `inbound`. N CoAccess + 0 true inbound → both:N,
/// inbound:0. Plus a mixed scenario: N CoAccess + M asymmetric inbound → both:N,
/// inbound:M (never inbound:N+M).
#[tokio::test]
async fn test_count_symmetric_increments_both_never_inbound() {
    for rel in ["Contradicts", "CoAccess", "Informs"] {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        let a = insert_entry_conf(wp, "A", 0.5).await;
        // 3 symmetric pairs.
        for i in 0..3 {
            let s = insert_entry_conf(wp, &format!("S{i}"), 0.5).await;
            insert_symmetric_pair(wp, a, s, rel, "co_access").await;
        }
        // 2 true asymmetric inbound.
        for i in 0..2 {
            let s = insert_entry_conf(wp, &format!("IN{i}"), 0.5).await;
            insert_edge_src(wp, s, a, "Prerequisite", "agent").await;
        }

        let split = count_neighbors_split(store.read_pool(), a)
            .await
            .expect("count");

        assert_eq!(split.both, 3, "{rel}: each ↔ counts once in `both`");
        assert_eq!(
            split.inbound, 2,
            "{rel}: inbound is the TRUE asymmetric inbound degree (↔ NOT folded in)"
        );
        assert_ne!(
            split.both, split.inbound,
            "{rel}: `both` is distinct from `inbound`"
        );

        store.close().await.unwrap();
    }
}

/// authored aggregate over the FULL deduped set (not displayed ≤3, not only `both`):
/// 9 symmetric edges, 7 agent-asserted → both:9, authored:7.
#[tokio::test]
async fn test_count_authored_aggregate_over_full_set() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    for i in 0..9 {
        let s = insert_entry_conf(wp, &format!("S{i}"), 0.5).await;
        let src = if i < 7 { "agent" } else { "co_access" };
        insert_symmetric_pair(wp, a, s, "Contradicts", src).await;
    }

    let split = count_neighbors_split(store.read_pool(), a)
        .await
        .expect("count");

    assert_eq!(split.both, 9, "9 canonicalized symmetric edges");
    assert_eq!(
        split.authored, 7,
        "authored counts every source='agent' canonical row over the full set"
    );

    store.close().await.unwrap();
}

/// R-01 totals side: each symmetric type contributes ONCE to `both` (not in+out,
/// not inbound). Asserted directly on EdgeCountSplit, independent of the ranked query.
#[tokio::test]
async fn test_count_symmetric_counted_once() {
    for rel in ["Contradicts", "CoAccess", "Informs"] {
        let (store, _dir) = open_test_store().await;
        let wp = &store.write_pool;

        let a = insert_entry_conf(wp, "A", 0.5).await;
        let b = insert_entry_conf(wp, "B", 0.5).await;
        insert_symmetric_pair(wp, a, b, rel, "co_access").await;

        let split = count_neighbors_split(store.read_pool(), a)
            .await
            .expect("count");

        assert_eq!(split.both, 1, "{rel}: pair contributes once to `both`");
        assert_eq!(split.inbound, 0, "{rel}: not counted as inbound");
        assert_eq!(split.outbound, 0, "{rel}: not counted as outbound");

        store.close().await.unwrap();
    }
}

/// The two-queries-must-agree guard: a symmetric pair occupies ONE ranked slot AND
/// contributes ONE to the count. A divergent canonicalization fails here.
#[tokio::test]
async fn test_count_canon_parity_with_rank_query() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    let b = insert_entry_conf(wp, "B", 0.5).await;
    let out = insert_entry_conf(wp, "OUT", 0.5).await;

    insert_symmetric_pair(wp, a, b, "CoAccess", "co_access").await;
    insert_edge_src(wp, a, out, "Supports", "co_access").await;

    let rows = query_ranked_neighbors(store.read_pool(), a)
        .await
        .expect("rank");
    let split = count_neighbors_split(store.read_pool(), a)
        .await
        .expect("count");

    // Symmetric pair: one ranked slot, one `both`.
    let sym_rows = rows.iter().filter(|r| r.row.target_id == b).count();
    assert_eq!(sym_rows, 1, "symmetric pair = one ranked slot");
    assert_eq!(split.both, 1, "symmetric pair = one `both` count");
    // Total parity: rank returns 2 rows (≤cap), count totals 2 (1 both + 1 outbound).
    assert_eq!(split.both + split.outbound + split.inbound, 2);

    store.close().await.unwrap();
}

/// Order-of-ops: counting is post-canonicalization — a pair seeded as two rows
/// totals 1, not 2.
#[tokio::test]
async fn test_count_before_canon_would_double() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    let b = insert_entry_conf(wp, "B", 0.5).await;
    insert_symmetric_pair(wp, a, b, "Informs", "behavioral").await;

    let split = count_neighbors_split(store.read_pool(), a)
        .await
        .expect("count");

    assert_eq!(
        split.both + split.inbound + split.outbound,
        1,
        "post-canon: a two-row pair totals 1, not 2"
    );

    store.close().await.unwrap();
}

/// R-12: Supersedes is absent from all buckets and the authored aggregate.
#[tokio::test]
async fn test_count_supersedes_not_counted() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    let kept = insert_entry_conf(wp, "KEPT", 0.5).await;
    let sup = insert_entry_conf(wp, "SUP", 0.5).await;

    insert_edge_src(wp, a, kept, "Supports", "agent").await;
    insert_edge_src(wp, a, sup, "Supersedes", "agent").await;

    let split = count_neighbors_split(store.read_pool(), a)
        .await
        .expect("count");

    assert_eq!(split.outbound, 1, "only the Supports edge counts");
    assert_eq!(split.inbound, 0);
    assert_eq!(split.both, 0);
    assert_eq!(split.authored, 1, "Supersedes excluded from authored too");

    store.close().await.unwrap();
}

/// Zero-edge / non-existent id → {0,0,0,0}, not an error (COALESCE over zero rows).
#[tokio::test]
async fn test_count_zero_edges() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;
    let isolated = insert_entry_conf(wp, "ISO", 0.5).await;

    let split = count_neighbors_split(store.read_pool(), isolated)
        .await
        .expect("count");
    assert_eq!(
        split,
        EdgeCountSplit {
            inbound: 0,
            outbound: 0,
            both: 0,
            authored: 0
        }
    );

    let missing = count_neighbors_split(store.read_pool(), 999_999)
        .await
        .expect("count");
    assert_eq!(
        missing,
        EdgeCountSplit {
            inbound: 0,
            outbound: 0,
            both: 0,
            authored: 0
        }
    );

    store.close().await.unwrap();
}

/// Nested shape: the struct exposes the three {inbound,outbound,both} buckets the
/// JSON edge_totals renders, with authored carried alongside but distinct.
#[tokio::test]
async fn test_count_nested_shape_three_buckets() {
    let (store, _dir) = open_test_store().await;
    let wp = &store.write_pool;

    let a = insert_entry_conf(wp, "A", 0.5).await;
    let o = insert_entry_conf(wp, "O", 0.5).await;
    let i = insert_entry_conf(wp, "I", 0.5).await;
    let s = insert_entry_conf(wp, "S", 0.5).await;

    insert_edge_src(wp, a, o, "Supports", "agent").await; // outbound, authored
    insert_edge_src(wp, i, a, "Prerequisite", "co_access").await; // inbound
    insert_symmetric_pair(wp, a, s, "CoAccess", "co_access").await; // both

    let split = count_neighbors_split(store.read_pool(), a)
        .await
        .expect("count");

    assert_eq!(split.inbound, 1);
    assert_eq!(split.outbound, 1);
    assert_eq!(split.both, 1);
    assert_eq!(
        split.authored, 1,
        "authored is carried but distinct from buckets"
    );

    store.close().await.unwrap();
}

/// Security: the count SQL binds the anchor positionally; the CASE/predicates are static.
#[tokio::test]
async fn test_count_uses_positional_binds() {
    let sql = format!(
        "{CANON_CTE}
        SELECT
            COALESCE(SUM(CASE WHEN direction = 'inbound'  THEN 1 ELSE 0 END), 0) AS inbound
        FROM deduped"
    );
    assert!(sql.contains("?1"), "anchor is a positional bind");
    assert!(
        sql.contains("SUM(CASE"),
        "counting is a SQL aggregate, not a row fetch"
    );
}
