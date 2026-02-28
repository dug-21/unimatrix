//! Episodic augmentation: post-search result refinement.
//!
//! This is the lowest-priority component (SR-04). Currently implemented as
//! a no-op stub that returns zero adjustments. The full implementation would
//! boost search results based on co-access affinity with anchor results.
//!
//! The existing co-access boost in coaccess.rs already provides similar
//! functionality. This module would add a refinement layer on top.

/// Post-search episodic augmenter.
///
/// Computes small score adjustments for search results based on co-access
/// affinity with top anchor results.
pub struct EpisodicAugmenter {
    /// Maximum score adjustment. Default: 0.02.
    pub max_boost: f64,
    /// Minimum co-access count to consider. Default: 3.
    pub min_affinity: u32,
}

impl EpisodicAugmenter {
    /// Create a new augmenter with the given parameters.
    pub fn new(max_boost: f64, min_affinity: u32) -> Self {
        Self {
            max_boost,
            min_affinity,
        }
    }

    /// Compute score adjustments for search results.
    ///
    /// Currently returns zero adjustments (no-op stub per SR-04).
    /// The full implementation would use co-access affinity with anchor results.
    pub fn compute_adjustments(
        &self,
        result_ids: &[u64],
        _result_scores: &[f64],
    ) -> Vec<f64> {
        vec![0.0; result_ids.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // T-EPI-01: Construction with defaults
    #[test]
    fn construction() {
        let aug = EpisodicAugmenter::new(0.02, 3);
        assert_eq!(aug.max_boost, 0.02);
        assert_eq!(aug.min_affinity, 3);
    }

    // T-EPI-02: No adjustment for single result
    #[test]
    fn single_result_no_adjustment() {
        let aug = EpisodicAugmenter::new(0.02, 3);
        let adjustments = aug.compute_adjustments(&[1], &[0.9]);
        assert_eq!(adjustments.len(), 1);
        assert_eq!(adjustments[0], 0.0);
    }

    // T-EPI-03: No adjustment when no co-access affinity (stub)
    #[test]
    fn no_affinity_no_adjustment() {
        let aug = EpisodicAugmenter::new(0.02, 3);
        let adjustments = aug.compute_adjustments(
            &[1, 2, 3, 4, 5],
            &[0.9, 0.85, 0.8, 0.75, 0.7],
        );
        assert_eq!(adjustments.len(), 5);
        assert!(adjustments.iter().all(|a| *a == 0.0));
    }

    // T-EPI-06: Stale co-access records (stub returns zero)
    #[test]
    fn stub_returns_zero() {
        let aug = EpisodicAugmenter::new(0.02, 3);
        let adjustments = aug.compute_adjustments(&[1, 2, 3], &[0.9, 0.8, 0.7]);
        assert!(adjustments.iter().all(|a| *a == 0.0));
    }

    // T-EPI-07: Below min_affinity threshold (stub)
    #[test]
    fn empty_results() {
        let aug = EpisodicAugmenter::new(0.02, 3);
        let adjustments = aug.compute_adjustments(&[], &[]);
        assert!(adjustments.is_empty());
    }
}
