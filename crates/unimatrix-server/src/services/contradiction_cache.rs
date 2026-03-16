//! ContradictionScanCache: in-memory cache of the last contradiction scan result.
//!
//! The contradiction scan runs O(N) ONNX inference (one embed per active entry) and
//! is too expensive to run on every `context_status` call or every 15-minute tick.
//!
//! This module provides a `ContradictionScanCacheHandle` that the background tick
//! writes (on every `CONTRADICTION_SCAN_INTERVAL_TICKS`-th tick) and `StatusService`
//! reads (without running ONNX at all).
//!
//! Design follows the `ConfidenceStateHandle` / `SupersessionStateHandle` pattern:
//! - `ContradictionScanResult` holds the cached data.
//! - `ContradictionScanCacheHandle` = `Arc<RwLock<Option<ContradictionScanResult>>>`.
//! - Cold-start: `None` — `compute_report()` sets `contradiction_scan_performed: false`.
//! - Background tick is the sole writer; `compute_report()` is read-only.
//!
//! All `RwLock` acquisitions use `.unwrap_or_else(|e| e.into_inner())` for poison
//! recovery consistent with `EffectivenessState` and `CategoryAllowlist` conventions.

use std::sync::{Arc, RwLock};

use crate::infra::contradiction::ContradictionPair;

/// How many background ticks elapse between full contradiction scans.
///
/// At the default 15-minute tick interval this equals ~60 minutes.
/// Tick 0 (first tick) is included: `tick_counter % CONTRADICTION_SCAN_INTERVAL_TICKS == 0`.
pub const CONTRADICTION_SCAN_INTERVAL_TICKS: u32 = 4;

/// The result of a `scan_contradictions()` call, stored in the cache.
#[derive(Debug, Clone)]
pub struct ContradictionScanResult {
    /// Detected contradiction pairs sorted by conflict score descending.
    pub pairs: Vec<ContradictionPair>,
}

/// Thread-safe handle for the last contradiction scan result.
///
/// - `None`: cold-start or no scan has completed yet.
/// - `Some(result)`: the result of the most recent successful scan.
///
/// The background tick is the sole writer.
/// `StatusService::compute_report()` is the sole reader.
pub type ContradictionScanCacheHandle = Arc<RwLock<Option<ContradictionScanResult>>>;

/// Create a new cold-start cache handle (value is `None`).
pub fn new_contradiction_cache_handle() -> ContradictionScanCacheHandle {
    Arc::new(RwLock::new(None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contradiction_cache_cold_start_is_none() {
        let handle = new_contradiction_cache_handle();
        let guard = handle.read().unwrap();
        assert!(guard.is_none(), "cold-start cache must be None");
    }

    #[test]
    fn test_contradiction_cache_write_then_read() {
        let handle = new_contradiction_cache_handle();

        let result = ContradictionScanResult {
            pairs: vec![ContradictionPair {
                entry_id_a: 1,
                entry_id_b: 2,
                title_a: "A".to_string(),
                title_b: "B".to_string(),
                similarity: 0.9,
                conflict_score: 0.7,
                explanation: "negation opposition (1.00)".to_string(),
            }],
        };

        {
            let mut guard = handle.write().unwrap();
            *guard = Some(result.clone());
        }

        let guard = handle.read().unwrap();
        let cached = guard.as_ref().expect("should have a result");
        assert_eq!(cached.pairs.len(), 1);
        assert_eq!(cached.pairs[0].entry_id_a, 1);
        assert_eq!(cached.pairs[0].entry_id_b, 2);
        assert!((cached.pairs[0].conflict_score - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_contradiction_scan_interval_constant() {
        // Tick gate: run on tick 0, 4, 8 — NOT on 1, 2, 3, 5, 6, 7.
        assert!(
            0 % CONTRADICTION_SCAN_INTERVAL_TICKS == 0,
            "tick 0 must trigger scan"
        );
        assert!(
            4 % CONTRADICTION_SCAN_INTERVAL_TICKS == 0,
            "tick 4 must trigger scan"
        );
        assert!(
            8 % CONTRADICTION_SCAN_INTERVAL_TICKS == 0,
            "tick 8 must trigger scan"
        );
        assert!(
            1 % CONTRADICTION_SCAN_INTERVAL_TICKS != 0,
            "tick 1 must NOT trigger scan"
        );
        assert!(
            2 % CONTRADICTION_SCAN_INTERVAL_TICKS != 0,
            "tick 2 must NOT trigger scan"
        );
        assert!(
            3 % CONTRADICTION_SCAN_INTERVAL_TICKS != 0,
            "tick 3 must NOT trigger scan"
        );
        assert!(
            5 % CONTRADICTION_SCAN_INTERVAL_TICKS != 0,
            "tick 5 must NOT trigger scan"
        );
        assert!(
            6 % CONTRADICTION_SCAN_INTERVAL_TICKS != 0,
            "tick 6 must NOT trigger scan"
        );
        assert!(
            7 % CONTRADICTION_SCAN_INTERVAL_TICKS != 0,
            "tick 7 must NOT trigger scan"
        );
    }

    #[test]
    fn test_tick_counter_u32_max_wraps_without_panic() {
        // u32::MAX wrapping_add(1) must not panic.
        let counter: u32 = u32::MAX;
        let next = counter.wrapping_add(1);
        assert_eq!(next, 0, "u32::MAX.wrapping_add(1) must wrap to 0");
        // Tick gate still works at 0 after wrap.
        assert!(
            next % CONTRADICTION_SCAN_INTERVAL_TICKS == 0,
            "wrapped-to-0 counter must trigger scan"
        );
    }

    #[test]
    fn test_contradiction_scan_result_clone() {
        let result = ContradictionScanResult { pairs: vec![] };
        let cloned = result.clone();
        assert!(cloned.pairs.is_empty());
    }
}
