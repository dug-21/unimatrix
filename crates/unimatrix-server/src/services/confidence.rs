//! ConfidenceService: batched fire-and-forget confidence recomputation.
//!
//! Replaces scattered compute_confidence + update_confidence blocks
//! throughout tools.rs (ADR-004).
//!
//! crt-019: Adds `ConfidenceState` struct and `ConfidenceStateHandle` type alias
//! for adaptive blend weight and Bayesian prior state management (ADR-001, ADR-002).
//! Also exposes `ConfidenceState::new_handle()` for standalone handle creation in
//! background.rs and other callers.

use std::sync::{Arc, RwLock};

use unimatrix_core::Store;

// ---------------------------------------------------------------------------
// ConfidenceState
// ---------------------------------------------------------------------------

/// Runtime-variable confidence parameters updated on each maintenance tick.
///
/// Holds the four-field quad `{alpha0, beta0, observed_spread, confidence_weight}`
/// that the maintenance tick (StatusService) updates and search paths read.
///
/// Initial values (R-06): `observed_spread = 0.1471` (pre-crt-019 measured value),
/// giving `confidence_weight = 0.18375` immediately on server start — not `0.0`,
/// which would regress to the floor (0.15) until the first maintenance tick.
///
/// All lock acquisitions use `unwrap_or_else(|e| e.into_inner())` for poison
/// recovery (FM-03), consistent with `CategoryAllowlist` convention.
#[derive(Debug, Clone)]
pub struct ConfidenceState {
    /// Bayesian prior positive pseudo-votes (cold-start: 3.0).
    pub alpha0: f64,
    /// Bayesian prior negative pseudo-votes (cold-start: 3.0).
    pub beta0: f64,
    /// p95-p5 confidence spread of the active population (initial: 0.1471).
    pub observed_spread: f64,
    /// `clamp(observed_spread * 1.25, 0.15, 0.25)` (initial: 0.18375).
    ///
    /// Stored to avoid re-computing the clamp on every search call.
    pub confidence_weight: f64,
}

impl Default for ConfidenceState {
    /// Initialize with pre-crt-019 measured values (R-06).
    ///
    /// Uses `observed_spread = 0.1471` so that `confidence_weight = 0.18375`
    /// on server start without waiting for the first maintenance tick.
    fn default() -> Self {
        ConfidenceState {
            alpha0: 3.0,
            beta0: 3.0,
            observed_spread: 0.1471,
            // clamp(0.1471 * 1.25, 0.15, 0.25) = clamp(0.18375, 0.15, 0.25) = 0.18375
            confidence_weight: 0.18375,
        }
    }
}

impl ConfidenceState {
    /// Create a new handle with default initial values.
    ///
    /// Convenience constructor used by background.rs and other callers that
    /// need a standalone handle not wired through `ConfidenceService`.
    pub fn new_handle() -> ConfidenceStateHandle {
        Arc::new(RwLock::new(ConfidenceState::default()))
    }
}

/// Thread-safe handle for `ConfidenceState`.
///
/// All readers (SearchService) clone the needed f64 value under a short read
/// lock. The writer (StatusService) holds the write lock only for the brief
/// critical section that updates all four fields atomically (ADR-002).
pub type ConfidenceStateHandle = Arc<RwLock<ConfidenceState>>;

// ---------------------------------------------------------------------------
// ConfidenceService
// ---------------------------------------------------------------------------

/// Batched confidence recomputation service.
///
/// Holds a `ConfidenceStateHandle` (crt-019) so callers can obtain a cloned
/// handle via `state_handle()` for wiring into `SearchService` and
/// `StatusService` without going through `ConfidenceService` at call time.
#[derive(Clone)]
pub(crate) struct ConfidenceService {
    store: Arc<Store>,
    state: ConfidenceStateHandle,
}

impl ConfidenceService {
    pub(crate) fn new(store: Arc<Store>) -> Self {
        ConfidenceService {
            store,
            state: Arc::new(RwLock::new(ConfidenceState::default())),
        }
    }

    /// Return a cloned `Arc` to the shared `ConfidenceState` handle.
    ///
    /// Used by `ServiceLayer::with_rate_config` to wire the same handle into
    /// both `SearchService` (reader) and `StatusService` (writer).
    pub(crate) fn state_handle(&self) -> ConfidenceStateHandle {
        Arc::clone(&self.state)
    }

    /// Recompute confidence for a batch of entries.
    ///
    /// Fire-and-forget via `spawn_blocking`. Snapshots `alpha0`/`beta0` from
    /// `ConfidenceState` before spawning — the closure captures current prior
    /// values so the Bayesian formula is used correctly (R-01).
    ///
    /// Empty `entry_ids` is a no-op (FR-03.4).
    pub(crate) fn recompute(&self, entry_ids: &[u64]) {
        if entry_ids.is_empty() {
            return;
        }

        let store = Arc::clone(&self.store);
        let ids = entry_ids.to_vec();

        // Snapshot the prior BEFORE spawn_blocking (on async thread — no lock issues).
        let (alpha0, beta0) = {
            let guard = self.state.read().unwrap_or_else(|e| e.into_inner());
            (guard.alpha0, guard.beta0)
        };

        let _ = tokio::spawn(async move {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            for id in ids {
                match store.get(id).await {
                    Ok(entry) => {
                        let conf = unimatrix_engine::confidence::compute_confidence(
                            &entry, now, alpha0, beta0,
                        );
                        if let Err(e) = store.update_confidence(id, conf).await {
                            tracing::warn!("confidence recompute failed for {id}: {e}");
                        }
                    }
                    Err(e) => {
                        tracing::warn!("confidence recompute: entry {id} not found: {e}");
                    }
                }
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- R-06: Initial observed_spread must be 0.1471 (not 0.0) --
    #[test]
    fn test_confidence_state_initial_observed_spread() {
        let state = ConfidenceState::default();
        assert!(
            (state.observed_spread - 0.1471).abs() < 1e-6,
            "initial observed_spread must be 0.1471 (pre-crt-019 measured), got {}",
            state.observed_spread
        );
    }

    // -- R-06: Initial confidence_weight must be ~0.18375 (above floor 0.15) --
    #[test]
    fn test_confidence_state_initial_weight() {
        let state = ConfidenceState::default();
        // clamp(0.1471 * 1.25, 0.15, 0.25) = clamp(0.18375, 0.15, 0.25) = 0.18375
        assert!(
            (state.confidence_weight - 0.18375).abs() < 1e-6,
            "initial confidence_weight must be ~0.18375, got {}",
            state.confidence_weight
        );
        // Must be strictly > 0.15 (floor) on server start without any tick
        assert!(
            state.confidence_weight > 0.15,
            "initial confidence_weight must exceed floor (0.15), got {}",
            state.confidence_weight
        );
    }

    // -- R-06: Cold-start priors must be 3.0 --
    #[test]
    fn test_confidence_state_initial_priors() {
        let state = ConfidenceState::default();
        assert_eq!(state.alpha0, 3.0, "initial alpha0 must be 3.0 (cold-start)");
        assert_eq!(state.beta0, 3.0, "initial beta0 must be 3.0 (cold-start)");
    }

    // -- Atomicity: write of all four fields visible as a consistent quad --
    #[test]
    fn test_confidence_state_update_all_four_fields() {
        let handle = Arc::new(RwLock::new(ConfidenceState::default()));

        // Simulate a maintenance tick writing all four fields
        {
            let mut state = handle.write().unwrap_or_else(|e| e.into_inner());
            state.alpha0 = 2.5;
            state.beta0 = 4.0;
            state.observed_spread = 0.22;
            // clamp(0.22 * 1.25, 0.15, 0.25) = clamp(0.275, 0.15, 0.25) = 0.25
            state.confidence_weight = 0.25;
        }

        // Verify all four updated atomically
        let state = handle.read().unwrap_or_else(|e| e.into_inner());
        assert_eq!(state.alpha0, 2.5);
        assert_eq!(state.beta0, 4.0);
        assert_eq!(state.observed_spread, 0.22);
        assert_eq!(state.confidence_weight, 0.25);
    }

    // -- Write-then-read roundtrip --
    #[test]
    fn test_confidence_state_handle_write_read() {
        let handle: ConfidenceStateHandle = Arc::new(RwLock::new(ConfidenceState::default()));

        {
            let mut guard = handle.write().unwrap_or_else(|e| e.into_inner());
            guard.alpha0 = 5.0;
            guard.beta0 = 2.0;
            guard.observed_spread = 0.20;
            // clamp(0.20 * 1.25, 0.15, 0.25) = clamp(0.25, 0.15, 0.25) = 0.25
            guard.confidence_weight = 0.25;
        }

        let read_guard = handle.read().unwrap_or_else(|e| e.into_inner());
        assert_eq!(read_guard.alpha0, 5.0);
        assert_eq!(read_guard.beta0, 2.0);
        assert_eq!(read_guard.observed_spread, 0.20);
        assert_eq!(read_guard.confidence_weight, 0.25);
    }

    // -- Concurrent read does not deadlock while holding a read lock --
    //
    // Two read guards can be held simultaneously; the state remains consistent.
    #[test]
    fn test_confidence_state_concurrent_reads_consistent() {
        let handle: ConfidenceStateHandle = Arc::new(RwLock::new(ConfidenceState::default()));

        let guard1 = handle.read().unwrap_or_else(|e| e.into_inner());
        let guard2 = handle.read().unwrap_or_else(|e| e.into_inner());

        // Both guards see the same initial values
        assert_eq!(guard1.alpha0, guard2.alpha0);
        assert_eq!(guard1.confidence_weight, guard2.confidence_weight);

        drop(guard1);
        drop(guard2);
    }

    // -- confidence_weight must never be initialised to 0.0 (EC-04 analogue) --
    #[test]
    fn test_confidence_state_weight_not_zero() {
        let state = ConfidenceState::default();
        assert_ne!(
            state.confidence_weight, 0.0,
            "confidence_weight must not be 0.0 on init — would regress to floor"
        );
    }

    // -- Clone produces independent copy (write to original does not affect clone) --
    #[test]
    fn test_confidence_state_clone_independent() {
        let original = ConfidenceState::default();
        let mut cloned = original.clone();
        cloned.alpha0 = 99.0;

        let fresh = ConfidenceState::default();
        assert_eq!(
            fresh.alpha0, 3.0,
            "clone mutation must not affect the default value"
        );
        assert_eq!(cloned.alpha0, 99.0);
    }
}
