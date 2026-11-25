//! #1009 — read-time quarantine exclusion for context_graph traversal modes.
//!
//! Every test quarantines with NO tick in between (no edge compaction, no snapshot rebuild)
//! and runs against BOTH a warm snapshot built BEFORE the quarantine (stale: still contains
//! Q) and a cold-start `use_fallback = true` handle (live-SQL DB paths).

use std::sync::{Arc, RwLock};

use unimatrix_store::SqlxStore;

use super::cross_surface::{
    cold_start_handle, insert_active, insert_supports_edge, open_test_store,
};
use crate::mcp::graph_read::GraphParams;
use crate::mcp::graph_read::graph_read_neighbors::handle_neighbors;
use crate::mcp::graph_read::graph_read_path::handle_path;
use crate::mcp::graph_read::graph_read_subgraph::handle_subgraph;
use crate::mcp::graph_read::graph_read_supersession::handle_chain;
use crate::services::typed_graph::TypedGraphState;

async fn set_status(store: &SqlxStore, id: u64, status: i64) {
    sqlx::query("UPDATE entries SET status = ?1 WHERE id = ?2")
        .bind(status)
        .bind(id as i64)
        .execute(store.write_pool_server())
        .await
        .expect("set status");
}

/// Warm snapshot rebuilt from the CURRENT store state (call before quarantining → stale).
async fn warm_handle(store: &SqlxStore) -> Arc<RwLock<TypedGraphState>> {
    let state = TypedGraphState::rebuild(store).await.expect("rebuild");
    assert!(!state.use_fallback, "rebuild must produce a warm snapshot");
    Arc::new(RwLock::new(state))
}

/// Seed A→Q→C (Supports). Returns (a, q, c).
async fn seed_aqc(store: &SqlxStore) -> (u64, u64, u64) {
    let a = insert_active(store, "A").await;
    let q = insert_active(store, "Q").await;
    let c = insert_active(store, "C").await;
    insert_supports_edge(store, a, q).await;
    insert_supports_edge(store, q, c).await;
    (a, q, c)
}

/// Both handles: warm snapshot (built pre-quarantine) and cold-start fallback.
async fn handles(store: &SqlxStore) -> Vec<(&'static str, Arc<RwLock<TypedGraphState>>)> {
    vec![
        ("warm", warm_handle(store).await),
        ("fallback", cold_start_handle()),
    ]
}

fn edge_ids(edges: &[crate::mcp::graph_read::EdgeRecord]) -> Vec<u64> {
    edges
        .iter()
        .flat_map(|e| [e.source_id, e.target_id])
        .collect()
}

async fn neighbors(
    store: &SqlxStore,
    h: &Arc<RwLock<TypedGraphState>>,
    id: u64,
    depth: u8,
    resolve: Option<bool>,
) -> Vec<crate::mcp::graph_read::EdgeRecord> {
    let p = GraphParams {
        mode: "neighbors".to_string(),
        id: Some(id),
        depth: Some(depth),
        resolve_supersessions: resolve,
        ..Default::default()
    };
    handle_neighbors(store, h, &p, id)
        .await
        .expect("neighbors")
        .edges
}

async fn path_found(
    store: &SqlxStore,
    h: &Arc<RwLock<TypedGraphState>>,
    f: u64,
    t: u64,
) -> Vec<u64> {
    let p = GraphParams {
        mode: "path".to_string(),
        from_id: Some(f),
        to_id: Some(t),
        ..Default::default()
    };
    let r = handle_path(store, h, &p).await.expect("path");
    if r.found {
        r.hops.iter().map(|h| h.entry_id).collect()
    } else {
        vec![]
    }
}

async fn subgraph(
    store: &SqlxStore,
    h: &Arc<RwLock<TypedGraphState>>,
    seeds: Vec<u64>,
    max_depth: u8,
) -> crate::mcp::graph_read::SubgraphResponse {
    let p = GraphParams {
        mode: "subgraph".to_string(),
        seed_ids: Some(seeds),
        max_depth: Some(max_depth),
        ..Default::default()
    };
    handle_subgraph(store, h, &p).await.expect("subgraph")
}

#[tokio::test]
async fn test_neighbors_quarantined_excluded_depth_1_and_2() {
    let (store, _dir) = open_test_store().await;
    let (a, q, c) = seed_aqc(&store).await;
    for (label, h) in handles(&store).await {
        set_status(&store, q, 3).await;
        for depth in [1u8, 2] {
            for resolve in [None, Some(false)] {
                let ids = edge_ids(&neighbors(&store, &h, a, depth, resolve).await);
                assert!(
                    !ids.contains(&q),
                    "{label} d={depth} r={resolve:?}: Q leaked {ids:?}"
                );
                assert!(
                    !ids.contains(&c),
                    "{label} d={depth}: C reached through Q {ids:?}"
                );
            }
        }
        // Quarantined anchor → empty.
        assert!(
            neighbors(&store, &h, q, 1, None).await.is_empty(),
            "{label}: Q anchor"
        );
        assert!(
            neighbors(&store, &h, q, 2, None).await.is_empty(),
            "{label}: Q anchor d2"
        );

        // Restore → reappears before any tick.
        set_status(&store, q, 0).await;
        let ids = edge_ids(&neighbors(&store, &h, a, 2, None).await);
        assert!(
            ids.contains(&q) && ids.contains(&c),
            "{label}: restore {ids:?}"
        );
    }
}

#[tokio::test]
async fn test_path_through_quarantined_broken_and_endpoints() {
    let (store, _dir) = open_test_store().await;
    let (a, q, c) = seed_aqc(&store).await;
    for (label, h) in handles(&store).await {
        set_status(&store, q, 3).await;
        assert!(
            path_found(&store, &h, a, c).await.is_empty(),
            "{label}: path via Q"
        );
        assert!(
            path_found(&store, &h, q, c).await.is_empty(),
            "{label}: from=Q"
        );
        assert!(
            path_found(&store, &h, a, q).await.is_empty(),
            "{label}: to=Q"
        );
        set_status(&store, q, 0).await;
        assert_eq!(
            path_found(&store, &h, a, c).await,
            vec![q, c],
            "{label}: restore"
        );
    }
}

#[tokio::test]
async fn test_path_quarantined_routes_via_alternate() {
    let (store, _dir) = open_test_store().await;
    let (a, q, c) = seed_aqc(&store).await;
    let b = insert_active(&store, "B").await;
    insert_supports_edge(&store, a, b).await;
    insert_supports_edge(&store, b, c).await;
    for (label, h) in handles(&store).await {
        set_status(&store, q, 3).await;
        assert_eq!(
            path_found(&store, &h, a, c).await,
            vec![b, c],
            "{label}: B route"
        );
        set_status(&store, q, 0).await;
    }
}

#[tokio::test]
async fn test_subgraph_quarantined_seed_and_neighbor_excluded() {
    let (store, _dir) = open_test_store().await;
    let (a, q, c) = seed_aqc(&store).await;
    for (label, h) in handles(&store).await {
        set_status(&store, q, 3).await;
        for depth in [1u8, 3] {
            let r = subgraph(&store, &h, vec![a], depth).await;
            let nodes: Vec<u64> = r.nodes.iter().map(|n| n.id).collect();
            assert_eq!(nodes, vec![a], "{label} d={depth}: nodes {nodes:?}");
            assert!(
                !edge_ids(&r.edges).contains(&q),
                "{label} d={depth}: edge touches Q"
            );
            assert!(!r.truncated, "{label} d={depth}: no false truncation");
        }
        let r = subgraph(&store, &h, vec![q], 3).await;
        assert!(r.nodes.is_empty() && r.edges.is_empty(), "{label}: Q seed");
        set_status(&store, q, 0).await;
        let r = subgraph(&store, &h, vec![a], 3).await;
        let nodes: Vec<u64> = r.nodes.iter().map(|n| n.id).collect();
        assert!(
            nodes.contains(&q) && nodes.contains(&c),
            "{label}: restore {nodes:?}"
        );
    }
}

#[tokio::test]
async fn test_chain_omits_quarantined_member_walks_through() {
    let (store, _dir) = open_test_store().await;
    let a = insert_active(&store, "A").await;
    let q = insert_active(&store, "Q").await;
    let c = insert_active(&store, "C").await;
    for (newer, older) in [(q, a), (c, q)] {
        sqlx::query("UPDATE entries SET supersedes = ?1 WHERE id = ?2")
            .bind(older as i64)
            .bind(newer as i64)
            .execute(store.write_pool_server())
            .await
            .expect("set supersedes");
        sqlx::query("UPDATE entries SET superseded_by = ?1 WHERE id = ?2")
            .bind(newer as i64)
            .bind(older as i64)
            .execute(store.write_pool_server())
            .await
            .expect("set superseded_by");
    }
    set_status(&store, q, 3).await;
    for anchor in [a, c] {
        let p = GraphParams {
            mode: "chain".to_string(),
            id: Some(anchor),
            ..Default::default()
        };
        let ids: Vec<u64> = handle_chain(&store, &p, anchor)
            .await
            .expect("chain")
            .entries
            .iter()
            .map(|e| e.id)
            .collect();
        assert!(!ids.contains(&q), "anchor {anchor}: Q leaked {ids:?}");
        assert!(
            ids.contains(&a) && ids.contains(&c),
            "anchor {anchor}: walk-through {ids:?}"
        );
    }
}
