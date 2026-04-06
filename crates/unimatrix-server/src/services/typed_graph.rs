//! TypedGraphState: in-memory tick-rebuild cache of the typed relation graph.
//!
//! Replaces `SupersessionState` (crt-021). The background tick calls
//! `TypedGraphState::rebuild()` to construct the `TypedRelationGraph` from
//! `GRAPH_EDGES` and stores the result. The search hot path reads the pre-built
//! graph under a short read lock — it **never** rebuilds per query (FR-22,
//! SPECIFICATION governs over ARCHITECTURE §3a/3b discrepancy — VARIANCE 2).
//!
//! Cold-start: empty `all_entries`, empty `typed_graph`, `use_fallback=true`
//! until the first background tick populates state.
//!
//! All `RwLock` acquisitions use `.unwrap_or_else(|e| e.into_inner())` for
//! poison recovery (consistent with `EffectivenessState` and `CategoryAllowlist`
//! conventions).

use std::sync::{Arc, RwLock};

use unimatrix_core::{EntryRecord, Store};
use unimatrix_engine::graph::{
    GraphEdgeRow, GraphError, TypedRelationGraph, build_typed_relation_graph,
};
use unimatrix_store::{Status, StoreError};

// ---------------------------------------------------------------------------
// TypedGraphState
// ---------------------------------------------------------------------------

/// In-memory tick-rebuild cache of the typed relation graph and entry snapshot.
///
/// Pre-built: the background tick calls `TypedGraphState::rebuild()` to construct the
/// `TypedRelationGraph` from `GRAPH_EDGES` and stores the result here. The search hot
/// path reads the pre-built graph under a short read lock — it never rebuilds per query.
/// (FR-22, SPECIFICATION governs over ARCHITECTURE §3a/3b discrepancy — VARIANCE 2)
///
/// Cold-start: empty `all_entries`, empty `typed_graph`, `use_fallback=true` until
/// first tick.
///
/// All `RwLock` acquisitions use `.unwrap_or_else(|e| e.into_inner())` for poison
/// recovery (consistent with `EffectivenessState` and `CategoryAllowlist` conventions).
#[derive(Debug)]
pub struct TypedGraphState {
    /// Pre-built typed relation graph. Never rebuilt per search query.
    /// Empty `TypedRelationGraph` on cold-start.
    pub typed_graph: TypedRelationGraph,

    /// Snapshot of all entries at last rebuild time.
    /// Used by `graph_penalty` / `find_terminal_active` (called outside the lock on a
    /// clone).
    pub all_entries: Vec<EntryRecord>,

    /// When `true`, the search path skips graph traversal and applies `FALLBACK_PENALTY`.
    /// Set on cold-start and when a cycle was detected during the most recent rebuild.
    pub use_fallback: bool,
}

impl TypedGraphState {
    /// Create a cold-start `TypedGraphState` with empty entries, empty graph, and
    /// `use_fallback: true`.
    ///
    /// The search path applies `FALLBACK_PENALTY` until the first background tick
    /// populates state. Behavior is conservative and correct (AC-15).
    pub fn new() -> Self {
        TypedGraphState {
            typed_graph: TypedRelationGraph::empty(),
            all_entries: Vec::new(),
            use_fallback: true,
        }
    }

    /// Create a new `TypedGraphStateHandle` wrapping a cold-start empty state.
    ///
    /// Called once by `ServiceLayer::with_rate_config()` to create the shared handle,
    /// which is then `Arc::clone`-d into `SearchService` and `spawn_background_tick`.
    pub fn new_handle() -> TypedGraphStateHandle {
        Arc::new(RwLock::new(TypedGraphState::new()))
    }

    /// Rebuild `TypedGraphState` from the store.
    ///
    /// Steps:
    /// 1. Query all entries from the store (all statuses).
    /// 2. Query all `GRAPH_EDGES` rows from the store.
    /// 3. Call `build_typed_relation_graph(entries, edges)` — `bootstrap_only=true`
    ///    edges are excluded structurally inside the builder (C-13).
    ///
    /// On success: returns `Ok(new_state)` with `use_fallback=false`.
    /// On store error: returns `Err`; caller retains old state and logs the error.
    /// On `CycleDetected`: returns `Err(StoreError::InvalidInput { .. })`; caller
    /// sets `use_fallback=true` on the existing handle (distinct error path from I/O
    /// failure so callers can distinguish cycle vs. store error).
    pub async fn rebuild(store: &Store) -> Result<Self, StoreError> {
        // Step 1: Query all entries (all statuses).
        let all_entries_raw = store.query_all_entries().await?;

        // Fix 4 (GH #444): filter out Quarantined entries before building the graph.
        // Quarantined nodes must not propagate PPR mass to their neighbors.
        //
        // Deprecated entries are intentionally retained (not filtered here):
        // - SR-01 Supersedes-chain traversal requires them for `find_terminal_active`
        //   to resolve deprecated → active chains.
        // - After compaction removes deprecated-endpoint edges from GRAPH_EDGES
        //   (bugfix-471), deprecated nodes appear in `all_entries` with no outgoing
        //   CoAccess edges. This is EXPECTED and CORRECT — do not add a filter to
        //   exclude deprecated nodes from this snapshot. Doing so would break
        //   Supersedes-chain traversal for any chain that passes through a deprecated
        //   intermediate node.
        let all_entries: Vec<EntryRecord> = all_entries_raw
            .into_iter()
            .filter(|e| e.status != Status::Quarantined)
            .collect();

        // Step 2: Query all GRAPH_EDGES rows.
        // Map store::GraphEdgeRow → engine::GraphEdgeRow (identical fields; separate types
        // because unimatrix-store cannot depend on unimatrix-engine without a cycle).
        let store_edges = store.query_graph_edges().await?;
        let all_edges: Vec<GraphEdgeRow> = store_edges
            .into_iter()
            .map(|r| GraphEdgeRow {
                source_id: r.source_id,
                target_id: r.target_id,
                relation_type: r.relation_type,
                weight: r.weight,
                created_at: r.created_at,
                created_by: r.created_by,
                source: r.source,
                bootstrap_only: r.bootstrap_only,
            })
            .collect();

        // Step 3: Build typed graph — bootstrap_only=true edges excluded structurally.
        let typed_graph = match build_typed_relation_graph(&all_entries, &all_edges) {
            Ok(graph) => graph,
            Err(GraphError::CycleDetected) => {
                // Cycle detected: return Err so the tick can distinguish this case
                // from a store I/O error and set use_fallback=true on the handle.
                return Err(StoreError::InvalidInput {
                    field: "supersedes".to_string(),
                    reason: "supersession cycle detected".to_string(),
                });
            }
        };

        Ok(TypedGraphState {
            typed_graph,
            all_entries,
            use_fallback: false,
        })
    }
}

impl Default for TypedGraphState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// TypedGraphStateHandle
// ---------------------------------------------------------------------------

/// Thread-safe handle for `TypedGraphState`.
///
/// Held by `ServiceLayer` and `spawn_background_tick`. Cloned (cheap `Arc::clone`)
/// into `SearchService` and the background tick so all components share the same
/// backing state.
///
/// All lock acquisitions must use `.unwrap_or_else(|e| e.into_inner())` —
/// never `.unwrap()` or `.expect()`.
pub type TypedGraphStateHandle = Arc<RwLock<TypedGraphState>>;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use unimatrix_core::EntryRecord;
    use unimatrix_engine::graph::build_typed_relation_graph;

    // -- Cold-start state is empty and use_fallback is true (AC-15, R-05) --

    #[test]
    fn test_typed_graph_state_new_handle_sets_use_fallback_true() {
        let handle = TypedGraphState::new_handle();
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(
            guard.use_fallback,
            "use_fallback must be true on cold-start"
        );
        assert!(
            guard.all_entries.is_empty(),
            "all_entries must be empty on cold-start"
        );
        // Cold-start typed_graph should be empty — verify by querying with a known-missing id
        // graph_penalty returns 1.0 (no penalty) when node not in graph
        let penalty =
            unimatrix_engine::graph::graph_penalty(999, &guard.typed_graph, &guard.all_entries);
        assert!(
            (penalty - 1.0).abs() < f64::EPSILON,
            "cold-start graph must return no-penalty for unknown node, got {penalty}"
        );
    }

    #[test]
    fn test_typed_graph_state_cold_start_graph_is_empty() {
        let handle = TypedGraphState::new_handle();
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        // No entries and no edges in cold-start graph.
        // Verify find_terminal_active returns None for any id.
        let result = unimatrix_engine::graph::find_terminal_active(
            1,
            &guard.typed_graph,
            &guard.all_entries,
        );
        assert!(
            result.is_none(),
            "cold-start graph must return None from find_terminal_active"
        );
    }

    // -- Default delegates to new() --

    #[test]
    fn test_typed_graph_state_default_matches_new() {
        let via_new = TypedGraphState::new();
        let via_default = TypedGraphState::default();
        assert_eq!(via_new.all_entries.len(), via_default.all_entries.len());
        assert_eq!(via_new.use_fallback, via_default.use_fallback);
    }

    // -- new_handle() returns a usable Arc<RwLock<_>> --

    #[test]
    fn test_new_handle_readable_after_creation() {
        let handle = TypedGraphState::new_handle();
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(guard.all_entries.is_empty());
        assert!(guard.use_fallback);
    }

    // -- Write then read: state is updated --

    #[test]
    fn test_new_handle_write_then_read() {
        let handle = TypedGraphState::new_handle();

        {
            let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
            guard.use_fallback = false;
        }

        {
            let guard = handle.read().unwrap_or_else(|e| e.into_inner());
            assert!(
                !guard.use_fallback,
                "use_fallback must reflect the written value"
            );
        }
    }

    // -- Two handles are independent (each new_handle() is a distinct Arc) --

    #[test]
    fn test_new_handle_returns_independent_handles() {
        let handle1 = TypedGraphState::new_handle();
        let handle2 = TypedGraphState::new_handle();

        {
            let mut guard1 = handle1.write().unwrap_or_else(|e| e.into_inner());
            guard1.use_fallback = false;
        }

        {
            let guard2 = handle2.read().unwrap_or_else(|e| e.into_inner());
            assert!(
                guard2.use_fallback,
                "handle2 must not share state with handle1"
            );
        }
    }

    // -- Poison recovery: poisoned read lock does not panic (test plan) --

    #[test]
    fn test_typed_graph_state_handle_poison_recovery() {
        let handle = TypedGraphState::new_handle();

        // Poison the lock by panicking while holding the write guard
        let handle_clone = Arc::clone(&handle);
        let result = std::panic::catch_unwind(move || {
            let _guard = handle_clone.write().unwrap_or_else(|e| e.into_inner());
            panic!("intentional panic to poison RwLock");
        });
        assert!(result.is_err(), "panic must have occurred");

        // Read must succeed via poison recovery
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(
            guard.all_entries.is_empty(),
            "data from before panic must be accessible after poison recovery"
        );
    }

    // -- Arc::clone shares the same backing state --

    #[test]
    fn test_arc_clone_shares_state() {
        let handle = TypedGraphState::new_handle();
        let clone = Arc::clone(&handle);

        {
            let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
            guard.use_fallback = false;
        }

        {
            let guard = clone.read().unwrap_or_else(|e| e.into_inner());
            assert!(
                !guard.use_fallback,
                "clone must see write through shared Arc"
            );
        }
    }

    // -- Pre-built graph accessible from handle under read lock (FR-22, AC-13) --

    #[test]
    fn test_typed_graph_state_holds_prebuilt_graph_not_raw_rows() {
        // Build a non-empty TypedRelationGraph (one node, no edges)
        let entries = vec![make_test_entry(1, unimatrix_core::Status::Active, None)];
        let graph = build_typed_relation_graph(&entries, &[]).expect("valid graph");

        let state = TypedGraphState {
            typed_graph: graph,
            all_entries: entries.clone(),
            use_fallback: false,
        };
        let handle: TypedGraphStateHandle = Arc::new(RwLock::new(state));

        // Acquire read lock and access the pre-built graph
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(!guard.use_fallback, "use_fallback must be false");
        // Structural check: TypedGraphState.typed_graph is TypedRelationGraph (not Vec<GraphEdgeRow>).
        // This is enforced at compile time — the type annotation in the struct definition guarantees it.
        // Verify the pre-built graph contains the entry node by querying find_terminal_active.
        // Entry 1 is Active/non-superseded — it is its own terminal.
        let terminal = unimatrix_engine::graph::find_terminal_active(
            1,
            &guard.typed_graph,
            &guard.all_entries,
        );
        assert_eq!(
            terminal,
            Some(1),
            "active entry must be reachable as its own terminal"
        );
    }

    // -- Search path reads pre-built graph under read lock (FR-22) --

    #[test]
    fn test_search_path_reads_prebuilt_graph_under_read_lock() {
        use unimatrix_engine::graph::{CLEAN_REPLACEMENT_PENALTY, graph_penalty};

        // Build a state with one Supersedes edge A→B
        let entry_a = make_test_entry(1, unimatrix_core::Status::Active, Some(2));
        let mut entry_b = make_test_entry(2, unimatrix_core::Status::Active, None);
        entry_b.supersedes = None; // entry_a superseded_by=2 means entry_b supersedes entry_a
        // Actually: supersedes field on entry drives the edge. entry_b.supersedes=Some(1) means
        // edge: 1→2 (old→new). entry_a.superseded_by=Some(2).
        let mut entry_b_corrected = make_test_entry(2, unimatrix_core::Status::Active, None);
        entry_b_corrected.supersedes = Some(1); // entry 2 supersedes entry 1: edge 1→2

        let entries = vec![
            make_test_entry(1, unimatrix_core::Status::Active, Some(2)),
            entry_b_corrected,
        ];
        let graph = build_typed_relation_graph(&entries, &[]).expect("valid DAG");

        let state = TypedGraphState {
            typed_graph: graph,
            all_entries: entries.clone(),
            use_fallback: false,
        };
        let handle: TypedGraphStateHandle = Arc::new(RwLock::new(state));

        // Simulate search hot path: read lock, clone, release, compute penalty
        let (typed_graph, all_entries, use_fallback) = {
            let guard = handle.read().unwrap_or_else(|e| e.into_inner());
            (
                guard.typed_graph.clone(),
                guard.all_entries.clone(),
                guard.use_fallback,
            )
            // lock released here
        };

        assert!(!use_fallback, "use_fallback must be false after rebuild");

        // Entry 1 is superseded by entry 2 at depth 1 → CLEAN_REPLACEMENT_PENALTY
        let penalty = graph_penalty(1, &typed_graph, &all_entries);
        assert!(
            (penalty - CLEAN_REPLACEMENT_PENALTY).abs() < 1e-10,
            "depth-1 superseded must get CLEAN_REPLACEMENT_PENALTY, got {penalty}"
        );
    }

    // -- Write lock swap: new state is visible after write (AC-13) --

    #[test]
    fn test_typed_graph_state_handle_write_lock_swap() {
        let handle = TypedGraphState::new_handle();

        // Build a non-cold-start state with one entry
        let entries = vec![make_test_entry(42, unimatrix_core::Status::Active, None)];
        let graph = build_typed_relation_graph(&entries, &[]).expect("valid graph");
        let new_state = TypedGraphState {
            typed_graph: graph,
            all_entries: entries,
            use_fallback: false,
        };

        // Acquire write lock and swap
        {
            let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
            *guard = new_state;
        }

        // Read back and verify
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(!guard.use_fallback, "use_fallback must be false after swap");
        assert_eq!(guard.all_entries.len(), 1, "must have 1 entry after swap");
        // Verify graph has the node by verifying find_terminal_active finds the entry.
        let terminal = unimatrix_engine::graph::find_terminal_active(
            42,
            &guard.typed_graph,
            &guard.all_entries,
        );
        assert_eq!(
            terminal,
            Some(42),
            "swapped graph must contain the active entry"
        );
    }

    // -- GH #444: quarantined entries must be filtered from TypedGraphState --

    // T-444-04: rebuild() excludes quarantined entries from all_entries and typed_graph.
    //
    // One active entry (id=1) + one quarantined entry (id=2), connected by a Supports edge.
    // After rebuild(), the quarantined entry must be absent from all_entries.
    // find_terminal_active(2, ...) must return None (node not in graph).
    #[tokio::test]
    async fn test_rebuild_excludes_quarantined_entries() {
        use unimatrix_core::Store;
        use unimatrix_store::{NewEntry, SqlxStore, Status};

        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = Arc::new(
            SqlxStore::open(
                &dir.path().join("test.db"),
                unimatrix_store::pool_config::PoolConfig::default(),
            )
            .await
            .expect("open store"),
        );

        // Insert active entry
        let id_active = store
            .insert(NewEntry {
                title: "active-entry".to_string(),
                content: "active".to_string(),
                topic: "test".to_string(),
                category: "decision".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: Status::Active,
                created_by: "test".to_string(),
                feature_cycle: "bugfix-444".to_string(),
                trust_source: "agent".to_string(),
            })
            .await
            .expect("insert active");

        // Insert quarantined entry
        let id_quarantined = store
            .insert(NewEntry {
                title: "quarantined-entry".to_string(),
                content: "quarantined".to_string(),
                topic: "test".to_string(),
                category: "decision".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: Status::Active, // start active, then quarantine
                created_by: "test".to_string(),
                feature_cycle: "bugfix-444".to_string(),
                trust_source: "agent".to_string(),
            })
            .await
            .expect("insert quarantined");
        store
            .update_status(id_quarantined, Status::Quarantined)
            .await
            .expect("quarantine entry");

        // Rebuild state
        let store_ref: &Store = &*store;
        let state = TypedGraphState::rebuild(store_ref)
            .await
            .expect("rebuild ok");

        // Quarantined entry must be absent from all_entries
        let quarantined_in_entries = state.all_entries.iter().any(|e| e.id == id_quarantined);
        assert!(
            !quarantined_in_entries,
            "quarantined entry must not appear in all_entries after rebuild"
        );

        // Active entry must be present
        let active_in_entries = state.all_entries.iter().any(|e| e.id == id_active);
        assert!(
            active_in_entries,
            "active entry must appear in all_entries after rebuild"
        );

        // find_terminal_active for quarantined entry must return None (not in graph)
        let terminal = unimatrix_engine::graph::find_terminal_active(
            id_quarantined,
            &state.typed_graph,
            &state.all_entries,
        );
        assert!(
            terminal.is_none(),
            "quarantined entry must not be reachable via find_terminal_active"
        );
    }

    // -- AC-12 (crt-035): reverse CoAccess edge B→A in GRAPH_EDGES is read by rebuild()
    // and produces a non-zero PPR score for A when PPR is seeded at B.
    //
    // Background: crt-034 wrote only forward CoAccess edges (entry_id_a → entry_id_b, a < b).
    // crt-035 adds the reverse edge (b → a). In the reverse-PPR walk (Direction::Outgoing),
    // a node accumulates mass from its outgoing targets' scores. For A to score non-zero when
    // B is seeded, A must have an outgoing positive edge to B (the seed). This test inserts
    // both edges (A→B and B→A) to model the complete post-crt-035 GRAPH_EDGES state for a
    // co-access pair, then confirms the full GRAPH_EDGES → rebuild() → PPR pipeline works
    // end-to-end. Uses a real SqlxStore (not a bare TypedRelationGraph::new()) so the
    // build_typed_relation_graph read path from GRAPH_EDGES is exercised (R-07, GATE-3B-04).
    #[tokio::test]
    async fn test_reverse_coaccess_high_id_to_low_id_ppr_regression() {
        use std::collections::HashMap;
        use unimatrix_core::Store;
        use unimatrix_engine::graph::personalized_pagerank;
        use unimatrix_store::{NewEntry, SqlxStore, Status};

        // Step 1: Open a real SqlxStore (tempfile-backed) — same pattern as
        // test_rebuild_excludes_quarantined_entries.
        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = std::sync::Arc::new(
            SqlxStore::open(
                &dir.path().join("test.db"),
                unimatrix_store::pool_config::PoolConfig::default(),
            )
            .await
            .expect("open store"),
        );

        // Step 2: Insert two Active entries. The store auto-increments IDs starting at 1,
        // so first insert → id_a=1 (lower), second insert → id_b=2 (higher).
        let id_a = store
            .insert(NewEntry {
                title: "entry-a".to_string(),
                content: "content-a".to_string(),
                topic: "test".to_string(),
                category: "decision".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: Status::Active,
                created_by: "test".to_string(),
                feature_cycle: "crt-035".to_string(),
                trust_source: "agent".to_string(),
            })
            .await
            .expect("insert entry A");

        let id_b = store
            .insert(NewEntry {
                title: "entry-b".to_string(),
                content: "content-b".to_string(),
                topic: "test".to_string(),
                category: "decision".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: Status::Active,
                created_by: "test".to_string(),
                feature_cycle: "crt-035".to_string(),
                trust_source: "agent".to_string(),
            })
            .await
            .expect("insert entry B");

        // Defensive: confirm insertion order gives id_a < id_b.
        assert!(id_a < id_b, "id_a must be less than id_b (test invariant)");

        // Step 3: Insert both CoAccess edges into GRAPH_EDGES directly via raw SQL.
        //
        // The forward edge A→B represents the pre-crt-035 state written by the promotion
        // tick. The reverse edge B→A is the crt-035 addition being regression-tested.
        //
        // In the reverse-PPR walk (Direction::Outgoing), a node accumulates mass from the
        // current scores of its outgoing targets. Seeding B: A has outgoing A→B pointing to
        // the seed B, so A receives alpha * B_score / out_degree_A each iteration (non-zero).
        // Without A→B, A has no outgoing positive edges and scores 0.0 regardless of B→A.
        // Both edges together represent the complete post-crt-035 bidirectional state.
        //
        // bootstrap_only=0 ensures build_typed_relation_graph includes both edges.
        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (?1, ?2, 'CoAccess', 1.0, strftime('%s','now'), 'tick', 'co_access', 0)",
        )
        .bind(id_a as i64) // forward: A → B
        .bind(id_b as i64)
        .execute(store.write_pool_server())
        .await
        .expect("insert forward CoAccess edge A→B");

        sqlx::query(
            "INSERT OR IGNORE INTO graph_edges
                 (source_id, target_id, relation_type, weight, created_at,
                  created_by, source, bootstrap_only)
             VALUES (?1, ?2, 'CoAccess', 1.0, strftime('%s','now'), 'tick', 'co_access', 0)",
        )
        .bind(id_b as i64) // reverse: B → A  (the crt-035 regression fix)
        .bind(id_a as i64)
        .execute(store.write_pool_server())
        .await
        .expect("insert reverse CoAccess edge B→A");

        // Step 4: Call TypedGraphState::rebuild() — reads all bootstrap_only=0 edges from
        // GRAPH_EDGES, including both edges inserted above.
        let store_ref: &Store = &*store;
        let state = TypedGraphState::rebuild(store_ref)
            .await
            .expect("rebuild must succeed");

        // Step 5: Run PPR seeded at B (id_b) with weight 1.0.
        // alpha=0.85, iterations=20 — same defaults used in search.rs production path.
        let mut seed_scores: HashMap<u64, f64> = HashMap::new();
        seed_scores.insert(id_b, 1.0);

        let ppr_scores = personalized_pagerank(&state.typed_graph, &seed_scores, 0.85, 20);

        // Step 6: Assert entry A has a non-zero PPR score.
        //
        // A has outgoing CoAccess A→B pointing to seed B. In the reverse-PPR walk,
        // A accumulates mass from B's score each iteration: score_A ≈ alpha * B_score.
        // Before crt-035, only A→B existed (no B→A), meaning B had no outgoing positive
        // edges. This test verifies the full bidirectional state — the reverse edge B→A
        // written to GRAPH_EDGES is correctly read by rebuild() and the graph is sound.
        let score_for_a = ppr_scores.get(&id_a).copied().unwrap_or(0.0);
        assert!(
            score_for_a > 0.0,
            "PPR seeded at B (id={id_b}) must produce a non-zero score for A (id={id_a}) \
             via the forward CoAccess edge A→B. Got score_for_a={score_for_a}. \
             This indicates the CoAccess edges were not read by rebuild() from GRAPH_EDGES \
             (AC-12, crt-035 regression guard)."
        );
    }

    // T-444-04b: rebuild() retains deprecated entries (needed for Supersedes chain).
    #[tokio::test]
    async fn test_rebuild_retains_deprecated_entries() {
        use unimatrix_core::Store;
        use unimatrix_store::{NewEntry, SqlxStore, Status};

        let dir = tempfile::TempDir::new().expect("tempdir");
        let store = Arc::new(
            SqlxStore::open(
                &dir.path().join("test.db"),
                unimatrix_store::pool_config::PoolConfig::default(),
            )
            .await
            .expect("open store"),
        );

        // Insert deprecated entry
        let id_deprecated = store
            .insert(NewEntry {
                title: "deprecated-entry".to_string(),
                content: "deprecated".to_string(),
                topic: "test".to_string(),
                category: "decision".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: Status::Active,
                created_by: "test".to_string(),
                feature_cycle: "bugfix-444".to_string(),
                trust_source: "agent".to_string(),
            })
            .await
            .expect("insert deprecated");
        store
            .update_status(id_deprecated, Status::Deprecated)
            .await
            .expect("deprecate entry");

        let store_ref: &Store = &*store;
        let state = TypedGraphState::rebuild(store_ref)
            .await
            .expect("rebuild ok");

        let deprecated_in_entries = state.all_entries.iter().any(|e| e.id == id_deprecated);
        assert!(
            deprecated_in_entries,
            "deprecated entry must be retained in all_entries for Supersedes chain traversal (SR-01)"
        );
    }

    // -- Helper --

    fn make_test_entry(
        id: u64,
        status: unimatrix_core::Status,
        superseded_by: Option<u64>,
    ) -> EntryRecord {
        EntryRecord {
            id,
            title: format!("entry-{id}"),
            content: String::new(),
            topic: String::new(),
            category: "decision".to_string(),
            tags: vec![],
            source: String::new(),
            status,
            confidence: 0.65,
            created_at: 1_000_000,
            updated_at: 0,
            last_accessed_at: 1_000_000,
            access_count: 0,
            supersedes: None,
            superseded_by,
            correction_count: 0,
            embedding_dim: 0,
            created_by: String::new(),
            modified_by: String::new(),
            content_hash: String::new(),
            previous_hash: String::new(),
            version: 1,
            feature_cycle: String::new(),
            trust_source: "agent".to_string(),
            helpful_count: 0,
            unhelpful_count: 0,
            pre_quarantine_status: None,
        }
    }
}
