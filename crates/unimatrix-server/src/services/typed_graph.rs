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
use unimatrix_store::StoreError;

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
        let all_entries = store.query_all_entries().await?;

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
