//! SupersessionState: in-memory cache of all entries for supersession graph construction.
//!
//! Eliminates the 4x `Store::query_by_status()` calls that crt-014 added to the hot
//! search path (GH #264). The background maintenance tick (15-min) rebuilds this state
//! once; the search path reads `all_entries` under a short `RwLock` read lock with zero
//! store I/O.
//!
//! Design follows the `EffectivenessStateHandle` pattern (crt-018b):
//! - `SupersessionState` holds the cached data.
//! - `SupersessionStateHandle` = `Arc<RwLock<SupersessionState>>`.
//! - Cold-start: empty `all_entries`, `use_fallback: true` — search falls back to the
//!   flat `FALLBACK_PENALTY` until the first background tick populates state.
//! - Background tick is the sole writer; search path is read-only.
//!
//! Option A (ADR-002 alignment): only `all_entries` and `use_fallback` are stored in
//! the handle. `build_supersession_graph` is called by the search path outside the lock
//! using the cloned entry snapshot. This keeps `SupersessionGraph` (not `Clone`) out of
//! the handle entirely and eliminates store I/O from the search hot path.
//!
//! All `RwLock` acquisitions use `.unwrap_or_else(|e| e.into_inner())` for poison
//! recovery (consistent with `EffectivenessState` and `CategoryAllowlist` conventions).

use std::sync::{Arc, RwLock};

use unimatrix_core::{EntryRecord, Store};
use unimatrix_store::StoreError;

// ---------------------------------------------------------------------------
// SupersessionState
// ---------------------------------------------------------------------------

/// In-memory cache of all entries for supersession graph construction.
///
/// Populated by the background maintenance tick (sole writer). Read by
/// `SearchService` under a short read lock that is released before any other
/// lock is acquired (R-01 lock ordering invariant).
///
/// `use_fallback` is set to `true` on cold-start or when the most recent
/// background rebuild detected a cycle. When `true`, the search path skips
/// `build_supersession_graph` and applies `FALLBACK_PENALTY` directly.
#[derive(Debug)]
pub struct SupersessionState {
    /// Snapshot of all entries across all statuses, used by the search path to
    /// construct the supersession graph without store I/O.
    ///
    /// Empty on cold-start; populated after the first background tick.
    pub all_entries: Vec<EntryRecord>,

    /// When `true`, the search path skips graph construction and applies the
    /// flat `FALLBACK_PENALTY`. Set on cold-start and when a cycle was detected
    /// during the most recent rebuild.
    pub use_fallback: bool,
}

impl SupersessionState {
    /// Create a cold-start `SupersessionState` with empty entries and `use_fallback: true`.
    ///
    /// The search path applies `FALLBACK_PENALTY` until the first background tick
    /// populates state. Behavior is conservative and correct.
    pub fn new() -> Self {
        SupersessionState {
            all_entries: Vec::new(),
            use_fallback: true,
        }
    }

    /// Create a new `SupersessionStateHandle` wrapping a cold-start empty state.
    ///
    /// Called once by `ServiceLayer::with_rate_config()` to create the shared handle,
    /// which is then `Arc::clone`-d into `SearchService` and `spawn_background_tick`.
    pub fn new_handle() -> SupersessionStateHandle {
        Arc::new(RwLock::new(SupersessionState::new()))
    }

    /// Rebuild `SupersessionState` from the store by querying all four entry statuses.
    ///
    /// Called by the background maintenance tick after the write lock is released
    /// from `EffectivenessState`. Returns a new `SupersessionState` ready to replace
    /// the current one under a write lock.
    ///
    /// On success: `use_fallback = false`, `all_entries` contains the full snapshot.
    /// On store error: returns `Err` so the caller can log and retain old state.
    ///
    /// Cycle detection is performed by the search path at graph-build time, not here.
    /// `use_fallback` is set to `false` by this function; the search path sets it back
    /// to `true` transiently if `build_supersession_graph` returns `CycleDetected`.
    /// This is safe because the search path rebuilds the graph on each call (Option A).
    pub async fn rebuild(store: &Store) -> Result<Self, StoreError> {
        // Single async SQL SELECT (GH #266, nxs-011).
        // Replaces 4x query_by_status() calls that held the mutex 4 times
        // and caused contention against concurrent MCP spawn_blocking calls.
        let all_entries = store.query_all_entries().await?;
        Ok(SupersessionState {
            all_entries,
            use_fallback: false,
        })
    }
}

impl Default for SupersessionState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// SupersessionStateHandle
// ---------------------------------------------------------------------------

/// Thread-safe handle for `SupersessionState`.
///
/// Held by `ServiceLayer` and `spawn_background_tick`. Cloned (cheap `Arc::clone`)
/// into `SearchService` and the background tick so all components share the same
/// backing state.
///
/// All lock acquisitions must use `.unwrap_or_else(|e| e.into_inner())` —
/// never `.unwrap()` or `.expect()`.
pub type SupersessionStateHandle = Arc<RwLock<SupersessionState>>;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Cold-start state is empty and use_fallback is true --

    #[test]
    fn test_supersession_state_new_cold_start() {
        let state = SupersessionState::new();
        assert!(
            state.all_entries.is_empty(),
            "all_entries must be empty on cold-start"
        );
        assert!(
            state.use_fallback,
            "use_fallback must be true on cold-start"
        );
    }

    // -- Default delegates to new() --

    #[test]
    fn test_supersession_state_default_matches_new() {
        let via_new = SupersessionState::new();
        let via_default = SupersessionState::default();
        assert_eq!(via_new.all_entries.len(), via_default.all_entries.len());
        assert_eq!(via_new.use_fallback, via_default.use_fallback);
    }

    // -- new_handle() returns a usable Arc<RwLock<_>> --

    #[test]
    fn test_new_handle_readable_after_creation() {
        let handle = SupersessionState::new_handle();
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(guard.all_entries.is_empty());
        assert!(guard.use_fallback);
    }

    // -- Write then read: state is updated --

    #[test]
    fn test_new_handle_write_then_read() {
        let handle = SupersessionState::new_handle();

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
        let handle1 = SupersessionState::new_handle();
        let handle2 = SupersessionState::new_handle();

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

    // -- Poison recovery: poisoned read lock does not panic --

    #[test]
    fn test_poison_recovery_read_after_write_panic() {
        let handle = SupersessionState::new_handle();

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
        let handle = SupersessionState::new_handle();
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
}
