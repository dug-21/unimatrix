//! ConfidenceService: batched fire-and-forget confidence recomputation.
//!
//! Replaces scattered compute_confidence + update_confidence blocks
//! throughout tools.rs (ADR-004).
//!
//! Also exposes `ConfidenceState` and `ConfidenceStateHandle` (crt-019):
//! runtime-variable quad `{alpha0, beta0, observed_spread, confidence_weight}`
//! updated on each maintenance tick and read by search/refresh paths.

use std::sync::{Arc, RwLock};

use unimatrix_core::Store;

// ---------------------------------------------------------------------------
// ConfidenceState (crt-019)
// ---------------------------------------------------------------------------

/// Runtime-variable confidence formula parameters.
///
/// Held in a shared `Arc<RwLock<ConfidenceState>>` so the background
/// maintenance tick (writer) and the search/refresh paths (readers) can access
/// the latest prior estimates without contention.
///
/// Initial values per R-06:
/// - `alpha0 = 3.0`, `beta0 = 3.0` (cold-start)
/// - `observed_spread = 0.1471` (pre-crt-019 measured value — NOT 0.0)
/// - `confidence_weight = 0.184` (clamp(0.1471 * 1.25, 0.15, 0.25))
#[derive(Debug, Clone)]
pub(crate) struct ConfidenceState {
    /// Bayesian prior — positive pseudo-votes.
    pub alpha0: f64,
    /// Bayesian prior — negative pseudo-votes.
    pub beta0: f64,
    /// p95 − p5 confidence spread of the active-entry population.
    pub observed_spread: f64,
    /// `clamp(observed_spread * 1.25, 0.15, 0.25)` — search blend weight.
    pub confidence_weight: f64,
}

/// Shared handle to `ConfidenceState`.
///
/// All lock acquisitions use `unwrap_or_else(|e| e.into_inner())` for
/// poison recovery, consistent with the `CategoryAllowlist` pattern (FM-03).
pub(crate) type ConfidenceStateHandle = Arc<RwLock<ConfidenceState>>;

impl Default for ConfidenceState {
    fn default() -> Self {
        // R-06: initialize with measured pre-crt-019 spread so confidence_weight
        // is non-floor (0.184) before the first maintenance tick.
        ConfidenceState {
            alpha0: 3.0,
            beta0: 3.0,
            observed_spread: 0.1471,
            confidence_weight: 0.184,
        }
    }
}

impl ConfidenceState {
    /// Create a new handle with default initial values.
    pub(crate) fn new_handle() -> ConfidenceStateHandle {
        Arc::new(RwLock::new(ConfidenceState::default()))
    }
}

/// Batched confidence recomputation service.
#[derive(Clone)]
pub(crate) struct ConfidenceService {
    store: Arc<Store>,
}

impl ConfidenceService {
    pub(crate) fn new(store: Arc<Store>) -> Self {
        ConfidenceService { store }
    }

    /// Recompute confidence for a batch of entries.
    ///
    /// Fire-and-forget via `spawn_blocking`. Single iteration over entry IDs
    /// within one blocking task. Per-entry failure is logged and skipped
    /// (consistent with existing fire-and-forget contract, ADR #53).
    ///
    /// Empty `entry_ids` is a no-op (FR-03.4).
    pub(crate) fn recompute(&self, entry_ids: &[u64]) {
        if entry_ids.is_empty() {
            return;
        }

        let store = Arc::clone(&self.store);
        let ids = entry_ids.to_vec();

        let _ = tokio::task::spawn_blocking(move || {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();

            for id in ids {
                match store.get(id) {
                    Ok(entry) => {
                        let conf =
                            unimatrix_engine::confidence::compute_confidence(&entry, now, 3.0, 3.0);
                        if let Err(e) = store.update_confidence(id, conf) {
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
