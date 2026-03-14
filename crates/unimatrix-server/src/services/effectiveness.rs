//! EffectivenessState: in-memory cache of per-entry effectiveness classifications.
//!
//! Provides the shared state container (`EffectivenessState`), its thread-safe
//! handle type (`EffectivenessStateHandle`), and the per-service snapshot cache
//! (`EffectivenessSnapshot`) used by `SearchService` and `BriefingService` to
//! avoid HashMap clones on every search call (ADR-001).
//!
//! This file is purely type and constructor definitions — no business logic resides here.
//! The background tick (`background.rs`) is the sole writer. Readers hold short-lived
//! read locks and release them before acquiring any other lock (R-01 lock ordering).
//!
//! All `RwLock` and `Mutex` acquisitions use `.unwrap_or_else(|e| e.into_inner())`
//! for poison recovery (consistent with `CategoryAllowlist` convention). Never use
//! `.unwrap()` or `.expect()` on these locks — a poisoned lock is recovered rather
//! than causing a panic that cascades to all subsequent search calls (Security Risk 3).
//!
//! Cold-start: empty maps produce `utility_delta = 0.0` for all entries. Behavior is
//! identical to pre-crt-018b; no fallback or guard logic required (AC-06, NFR-06).
//!
//! Note on `generation` overflow: `u64` allows 2^64 writes before wrapping to 0.
//! At one write per 15-minute tick, this is ~8.8 billion years. No guard needed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use unimatrix_engine::effectiveness::EffectivenessCategory;

// ---------------------------------------------------------------------------
// EffectivenessState
// ---------------------------------------------------------------------------

/// In-memory cache of per-entry effectiveness classifications.
///
/// Holds the per-entry `EffectivenessCategory` map and consecutive-bad-cycle
/// counters populated by the background maintenance tick (sole writer).
///
/// Read by `SearchService` and `BriefingService` under short read locks that
/// are released before any other lock is acquired (R-01 lock ordering invariant).
///
/// The `generation` field is incremented on each write; readers compare against
/// their cached generation to decide whether to re-clone the `categories` HashMap,
/// avoiding redundant clones between ticks (ADR-001).
#[derive(Debug)]
pub struct EffectivenessState {
    /// entry_id -> last-known EffectivenessCategory, populated by background tick.
    /// Absent key means: not yet classified. `utility_delta = 0.0` for absent keys.
    pub categories: HashMap<u64, EffectivenessCategory>,

    /// entry_id -> count of consecutive background ticks where the entry was
    /// classified Ineffective or Noisy. Absent key means counter = 0.
    ///
    /// In-memory only; resets to empty on server restart (intentional — Constraint 6).
    pub consecutive_bad_cycles: HashMap<u64, u32>,

    /// Incremented on every write to `EffectivenessState`.
    ///
    /// Readers compare against their cached generation to decide whether to
    /// re-clone the `categories` HashMap. Only the background tick writer
    /// increments this field. Wraps on overflow (u64, ~8.8 billion years at
    /// one tick per 15 minutes — not a practical concern).
    pub generation: u64,
}

impl EffectivenessState {
    /// Create a new empty `EffectivenessState` for cold-start.
    ///
    /// All maps are empty; `generation` starts at 0. This produces `utility_delta = 0.0`
    /// for all entries until the first background tick populates the categories map.
    /// System behavior is identical to pre-crt-018b until data arrives (AC-06, NFR-06).
    pub fn new() -> Self {
        EffectivenessState {
            categories: HashMap::new(),
            consecutive_bad_cycles: HashMap::new(),
            generation: 0,
        }
    }

    /// Create a new `EffectivenessStateHandle` wrapping a cold-start empty state.
    ///
    /// Called once by `ServiceLayer::with_rate_config()` to create the shared handle,
    /// which is then `Arc::clone`-d into `SearchService`, `BriefingService`, and
    /// `spawn_background_tick`. Mirrors `ConfidenceState::new_handle()`.
    pub fn new_handle() -> EffectivenessStateHandle {
        Arc::new(RwLock::new(EffectivenessState::new()))
    }
}

impl Default for EffectivenessState {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// EffectivenessStateHandle
// ---------------------------------------------------------------------------

/// Thread-safe handle for `EffectivenessState`.
///
/// Held by `ServiceLayer` alongside `ConfidenceStateHandle`. Cloned (cheap
/// `Arc::clone`) into `SearchService`, `BriefingService`, and the background
/// tick path so all components share the same backing state.
///
/// All lock acquisitions must use `.unwrap_or_else(|e| e.into_inner())` —
/// never `.unwrap()` or `.expect()`.
pub type EffectivenessStateHandle = Arc<RwLock<EffectivenessState>>;

// ---------------------------------------------------------------------------
// EffectivenessSnapshot
// ---------------------------------------------------------------------------

/// Cached snapshot of `EffectivenessState.categories` for a single service instance.
///
/// Held as `Arc<Mutex<EffectivenessSnapshot>>` in `SearchService` and
/// `BriefingService`. The `Arc` wrapper ensures rmcp-cloned instances of a
/// service share the same cached copy (R-06 mitigation — clone avoidance).
///
/// The snapshot is only refreshed when `generation` changes, which happens once
/// per background tick (~15 minutes). All other calls use the cached copy
/// without re-cloning the `categories` HashMap (ADR-001).
#[derive(Debug)]
pub struct EffectivenessSnapshot {
    /// The `EffectivenessState.generation` value at the time this snapshot was taken.
    pub generation: u64,
    /// Clone of `EffectivenessState.categories` at snapshot time.
    pub categories: HashMap<u64, EffectivenessCategory>,
}

impl EffectivenessSnapshot {
    /// Create a new shared snapshot cache initialised to the cold-start sentinel (generation=0).
    ///
    /// Used by `SearchService::new()` and `BriefingService::new()` to initialise the
    /// per-service snapshot cache. The `Arc` wrapper ensures all rmcp-cloned instances
    /// of the same service share one cache object.
    pub fn new_shared() -> Arc<Mutex<EffectivenessSnapshot>> {
        Arc::new(Mutex::new(EffectivenessSnapshot {
            generation: 0,
            categories: HashMap::new(),
        }))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- AC-06 / R-07 — Cold-start state is empty and generation starts at zero --

    #[test]
    fn test_effectiveness_state_new_returns_empty() {
        let state = EffectivenessState::new();
        assert!(
            state.categories.is_empty(),
            "categories must be empty on cold-start"
        );
        assert!(
            state.consecutive_bad_cycles.is_empty(),
            "consecutive_bad_cycles must be empty on cold-start"
        );
        assert_eq!(state.generation, 0, "generation must start at 0");
    }

    // -- ADR-001 — Generation counter starts at zero --

    #[test]
    fn test_generation_starts_at_zero() {
        let state = EffectivenessState::new();
        assert_eq!(state.generation, 0);
    }

    // -- Type alias compiles and is usable via read lock --

    #[test]
    fn test_effectiveness_state_handle_type_alias() {
        let handle: EffectivenessStateHandle = Arc::new(RwLock::new(EffectivenessState::new()));
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert_eq!(guard.categories.len(), 0);
    }

    // -- ADR-001 — Generation increments on write; visible to subsequent readers --

    #[test]
    fn test_generation_increments_on_write() {
        let handle = EffectivenessState::new_handle();

        {
            let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
            guard.categories.insert(1, EffectivenessCategory::Effective);
            guard.generation = 1;
        }
        {
            let guard = handle.read().unwrap_or_else(|e| e.into_inner());
            assert_eq!(guard.generation, 1);
        }

        {
            let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
            guard.categories.insert(2, EffectivenessCategory::Noisy);
            guard.generation = 2;
        }
        {
            let guard = handle.read().unwrap_or_else(|e| e.into_inner());
            assert_eq!(guard.generation, 2);
            assert_eq!(guard.categories.len(), 2);
        }
    }

    // -- R-01 — Read guard must be dropped before acquiring a write lock (no deadlock) --

    #[test]
    fn test_generation_read_write_no_simultaneous_locks() {
        let handle = EffectivenessState::new_handle();

        // Acquire read lock, copy generation, then DROP the read guard
        let generation_snapshot = {
            let guard = handle.read().unwrap_or_else(|e| e.into_inner());
            guard.generation
        }; // read guard dropped here

        // Acquire write lock — no deadlock because read guard is out of scope
        {
            let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
            guard.generation = generation_snapshot + 1;
        }

        // Acquire read again — consistent state
        {
            let guard = handle.read().unwrap_or_else(|e| e.into_inner());
            assert_eq!(guard.generation, 1);
        }
    }

    // -- new_handle() produces an independent Arc (each call gives distinct state) --

    #[test]
    fn test_new_handle_returns_independent_handles() {
        let handle1 = EffectivenessState::new_handle();
        let handle2 = EffectivenessState::new_handle();

        {
            let mut guard1 = handle1.write().unwrap_or_else(|e| e.into_inner());
            guard1
                .categories
                .insert(99, EffectivenessCategory::Ineffective);
            guard1.generation = 1;
        }

        // handle2 must remain empty (distinct Arc<RwLock<_>>)
        {
            let guard2 = handle2.read().unwrap_or_else(|e| e.into_inner());
            assert!(
                guard2.categories.is_empty(),
                "handle2 must not share state with handle1"
            );
            assert_eq!(guard2.generation, 0);
        }
    }

    // -- R-06 — EffectivenessSnapshot shared via Arc<Mutex<_>> across clones --

    #[test]
    fn test_effectiveness_snapshot_generation_match() {
        let shared = EffectivenessSnapshot::new_shared();
        let clone = Arc::clone(&shared);

        // Update via original Arc
        {
            let mut cache = shared.lock().unwrap_or_else(|e| e.into_inner());
            cache.generation = 5;
            cache.categories.insert(1, EffectivenessCategory::Settled);
        }

        // Clone must see the same state (shared backing object)
        {
            let cache = clone.lock().unwrap_or_else(|e| e.into_inner());
            assert_eq!(
                cache.generation, 5,
                "clone must see updated generation via shared Arc"
            );
            assert_eq!(
                cache.categories.get(&1),
                Some(&EffectivenessCategory::Settled)
            );
        }
    }

    // -- Poison recovery: poisoned read lock does not panic --

    #[test]
    fn test_effectiveness_state_handle_poison_recovery() {
        let handle = EffectivenessState::new_handle();

        // Pre-populate with a known entry before poisoning
        {
            let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
            guard
                .categories
                .insert(42, EffectivenessCategory::Effective);
            guard.generation = 7;
        }

        // Poison the lock by panicking while holding the write guard
        let handle_clone = Arc::clone(&handle);
        let result = std::panic::catch_unwind(move || {
            let _guard = handle_clone.write().unwrap_or_else(|e| e.into_inner());
            panic!("intentional panic to poison RwLock");
        });
        assert!(result.is_err(), "panic must have occurred");

        // Read lock must succeed via poison recovery — stale state is accessible
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            guard.categories.get(&42),
            Some(&EffectivenessCategory::Effective),
            "data written before panic must be accessible after poison recovery"
        );
        assert_eq!(guard.generation, 7);
    }

    // -- Default trait delegates to new() --

    #[test]
    fn test_effectiveness_state_default_matches_new() {
        let via_new = EffectivenessState::new();
        let via_default = EffectivenessState::default();
        assert_eq!(via_new.generation, via_default.generation);
        assert_eq!(via_new.categories.len(), via_default.categories.len());
        assert_eq!(
            via_new.consecutive_bad_cycles.len(),
            via_default.consecutive_bad_cycles.len()
        );
    }

    // -- new_handle() can be read and written after creation --

    #[test]
    fn test_new_handle_can_be_read_and_written() {
        let handle = EffectivenessState::new_handle();

        // Write
        {
            let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
            guard.categories.insert(1, EffectivenessCategory::Noisy);
            guard.consecutive_bad_cycles.insert(1, 3);
            guard.generation = 1;
        }

        // Read back
        {
            let guard = handle.read().unwrap_or_else(|e| e.into_inner());
            assert_eq!(
                guard.categories.get(&1),
                Some(&EffectivenessCategory::Noisy)
            );
            assert_eq!(guard.consecutive_bad_cycles.get(&1), Some(&3u32));
            assert_eq!(guard.generation, 1);
        }
    }
}
