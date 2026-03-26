//! Co-access pair generation and boost computation.
//!
//! Tracks which entries are retrieved together and computes additive boosts
//! for search and briefing results based on co-access frequency.

use std::collections::{HashMap, HashSet};

use unimatrix_store::SqlxStore;

/// Maximum entries to consider for co-access pair generation.
/// 10 entries = 45 pairs. Beyond this, pairs are not generated.
pub const MAX_CO_ACCESS_ENTRIES: usize = 10;

/// Staleness threshold for co-access pairs.
///
/// Set to 365 days (one year) to tolerate feature cycles that go dormant for
/// weeks or months and then resume. Co-access signal accumulated before a pause
/// should remain available when work resumes. Pairs older than this threshold
/// are excluded from boost calculations and deleted during maintenance ticks.
pub const CO_ACCESS_STALENESS_SECONDS: u64 = 365 * 24 * 3600; // 31_536_000

/// Maximum additive boost for search results from co-access signal.
pub const MAX_CO_ACCESS_BOOST: f64 = 0.03;

/// Maximum additive boost for briefing results from co-access signal.
pub const MAX_BRIEFING_CO_ACCESS_BOOST: f64 = 0.01;

/// Co-access count beyond which boost is fully saturated (log-transform denominator).
pub const MAX_MEANINGFUL_CO_ACCESS: f64 = 20.0;

/// Partner count beyond which co-access affinity is fully saturated.
pub const MAX_MEANINGFUL_PARTNERS: f64 = 10.0;

/// Generate ordered pairs from entry IDs, considering at most `max_entries`.
///
/// Returns `Vec<(min_id, max_id)>`. Pair count: `k*(k-1)/2` where `k = min(len, max_entries)`.
/// At `max_entries=10`, this is at most 45 pairs.
pub fn generate_pairs(entry_ids: &[u64], max_entries: usize) -> Vec<(u64, u64)> {
    let effective = &entry_ids[..entry_ids.len().min(max_entries)];
    let k = effective.len();
    let mut pairs = Vec::with_capacity(k * k.saturating_sub(1) / 2);

    for i in 0..k {
        for j in (i + 1)..k {
            let (a, b) = if effective[i] <= effective[j] {
                (effective[i], effective[j])
            } else {
                (effective[j], effective[i])
            };
            pairs.push((a, b));
        }
    }

    pairs
}

/// Compute the log-transformed co-access boost for a given count.
///
/// Formula (ADR-002):
///   `raw = ln(1 + count) / ln(1 + MAX_MEANINGFUL_CO_ACCESS)`
///   `boost = min(raw, 1.0) * max_boost`
///
/// Returns a value in `[0.0, max_boost]`.
fn co_access_boost(count: u32, max_boost: f64) -> f64 {
    if count == 0 {
        return 0.0;
    }
    let raw = (1.0 + count as f64).ln() / (1.0 + MAX_MEANINGFUL_CO_ACCESS).ln();
    let capped = raw.min(1.0);
    capped * max_boost
}

/// Compute co-access boost scores for search results.
///
/// Takes anchor IDs (top results) and all result IDs. For each result
/// that is a co-access partner of any anchor, computes a boost based on
/// co-access count (log-transformed, capped).
///
/// If a result is a partner of multiple anchors, takes the maximum boost.
///
/// `deprecated_ids` excludes entries from both anchor and partner roles (crt-010).
/// Pass an empty set for backward-compatible behavior.
///
/// Returns `HashMap<entry_id, boost>` where values are in `[0.0, MAX_CO_ACCESS_BOOST]`.
pub fn compute_search_boost(
    anchor_ids: &[u64],
    result_ids: &[u64],
    store: &SqlxStore,
    staleness_cutoff: u64,
    deprecated_ids: &HashSet<u64>,
) -> HashMap<u64, f64> {
    compute_boost_internal(
        anchor_ids,
        result_ids,
        store,
        staleness_cutoff,
        MAX_CO_ACCESS_BOOST,
        deprecated_ids,
    )
}

/// Compute co-access boost scores for briefing results.
/// Same algorithm as search but with a smaller max boost.
///
/// `deprecated_ids` excludes entries from both anchor and partner roles (crt-010).
/// Pass an empty set for backward-compatible behavior.
pub fn compute_briefing_boost(
    anchor_ids: &[u64],
    result_ids: &[u64],
    store: &SqlxStore,
    staleness_cutoff: u64,
    deprecated_ids: &HashSet<u64>,
) -> HashMap<u64, f64> {
    compute_boost_internal(
        anchor_ids,
        result_ids,
        store,
        staleness_cutoff,
        MAX_BRIEFING_CO_ACCESS_BOOST,
        deprecated_ids,
    )
}

/// Internal boost computation shared by search and briefing.
fn compute_boost_internal(
    anchor_ids: &[u64],
    result_ids: &[u64],
    store: &SqlxStore,
    staleness_cutoff: u64,
    max_boost: f64,
    deprecated_ids: &HashSet<u64>,
) -> HashMap<u64, f64> {
    let mut boost_map: HashMap<u64, f64> = HashMap::new();
    let result_set: HashSet<u64> = result_ids.iter().copied().collect();

    for &anchor_id in anchor_ids {
        // crt-010: skip deprecated anchors
        if deprecated_ids.contains(&anchor_id) {
            continue;
        }

        let partners = match tokio::runtime::Handle::current()
            .block_on(store.get_co_access_partners(anchor_id, staleness_cutoff))
        {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("co-access partner lookup failed for {anchor_id}: {e}");
                continue;
            }
        };

        for (partner_id, record) in partners {
            if !result_set.contains(&partner_id) {
                continue;
            }
            if partner_id == anchor_id {
                continue;
            }
            // crt-010: skip deprecated partners
            if deprecated_ids.contains(&partner_id) {
                continue;
            }

            let boost = co_access_boost(record.count, max_boost);
            let existing = boost_map.entry(partner_id).or_insert(0.0);
            if boost > *existing {
                *existing = boost;
            }
        }
    }

    boost_map
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- generate_pairs tests (R-04) --

    #[test]
    fn generate_pairs_cap_enforcement() {
        let ids: Vec<u64> = (1..=15).collect();
        let pairs = generate_pairs(&ids, MAX_CO_ACCESS_ENTRIES);
        // 10 choose 2 = 45
        assert_eq!(pairs.len(), 45);
    }

    #[test]
    fn generate_pairs_single_entry() {
        let pairs = generate_pairs(&[1], 10);
        assert!(pairs.is_empty());
    }

    #[test]
    fn generate_pairs_two_entries() {
        let pairs = generate_pairs(&[5, 3], 10);
        assert_eq!(pairs, vec![(3, 5)]);
    }

    #[test]
    fn generate_pairs_exactly_ten() {
        let ids: Vec<u64> = (1..=10).collect();
        let pairs = generate_pairs(&ids, 10);
        assert_eq!(pairs.len(), 45);
    }

    #[test]
    fn generate_pairs_empty() {
        let pairs = generate_pairs(&[], 10);
        assert!(pairs.is_empty());
    }

    #[test]
    fn generate_pairs_ordered() {
        let pairs = generate_pairs(&[10, 5, 8], 10);
        for (a, b) in &pairs {
            assert!(a < b, "pair ({a}, {b}) is not ordered");
        }
        assert_eq!(pairs.len(), 3);
    }

    // -- co_access_boost formula tests (R-02) --

    #[test]
    fn boost_at_zero() {
        assert_eq!(co_access_boost(0, MAX_CO_ACCESS_BOOST), 0.0);
    }

    #[test]
    fn boost_at_one() {
        let b = co_access_boost(1, 0.03);
        // ln(2)/ln(21) * 0.03 ~= 0.00682
        assert!(b > 0.006 && b < 0.008, "boost at 1 = {b}");
    }

    #[test]
    fn boost_at_twenty_cap() {
        let b = co_access_boost(20, 0.03);
        // ln(21)/ln(21) = 1.0, * 0.03 = 0.03
        assert!((b - 0.03).abs() < 0.001, "boost at 20 = {b}");
    }

    #[test]
    fn boost_at_hundred_capped() {
        let b = co_access_boost(100, 0.03);
        assert!((b - 0.03).abs() < 0.001, "boost at 100 = {b}");
    }

    #[test]
    fn boost_at_u32_max_no_overflow() {
        let b = co_access_boost(u32::MAX, 0.03);
        assert!((b - 0.03).abs() < 0.001, "boost at u32::MAX = {b}");
    }

    #[test]
    fn boost_diminishing_returns() {
        let b10 = co_access_boost(10, 0.03);
        let b20 = co_access_boost(20, 0.03);
        let b0 = co_access_boost(0, 0.03);
        // b20 - b10 < b10 - b0 (diminishing returns)
        assert!(
            (b20 - b10) < (b10 - b0),
            "not diminishing: b0={b0}, b10={b10}, b20={b20}"
        );
    }

    #[test]
    fn briefing_boost_smaller_max() {
        let b = co_access_boost(20, MAX_BRIEFING_CO_ACCESS_BOOST);
        assert!((b - 0.01).abs() < 0.001, "briefing boost at 20 = {b}");
    }

    // -- compute_search_boost tests (R-06) --

    #[test]
    fn similarity_dominance() {
        // Entry A: similarity=0.95, no boost
        // Entry B: similarity=0.85, max boost=0.03
        let score_a = crate::confidence::rerank_score(0.95, 0.5, 0.15); // no boost
        let score_b = crate::confidence::rerank_score(0.85, 0.5, 0.15) + MAX_CO_ACCESS_BOOST;
        assert!(
            score_a > score_b,
            "similarity should dominate: score_a={score_a}, score_b={score_b}"
        );
    }

    #[test]
    fn tiebreaker_behavior() {
        let base = crate::confidence::rerank_score(0.90, 0.5, 0.15);
        let with_boost = base + 0.02;
        assert!(with_boost > base);
    }

    // -- GH #408: staleness threshold regression --

    #[test]
    fn co_access_staleness_at_least_one_year() {
        assert!(
            CO_ACCESS_STALENESS_SECONDS >= 365 * 24 * 3600,
            "staleness window must be at least one year to tolerate dormant feature cycles (GH #408)"
        );
    }

    // -- crt-005: f64 type verification --

    // UT-C2-07: MAX_CO_ACCESS_BOOST constants are f64
    #[test]
    fn co_access_boost_constants_f64() {
        assert_eq!(MAX_CO_ACCESS_BOOST, 0.03_f64);
        assert_eq!(MAX_BRIEFING_CO_ACCESS_BOOST, 0.01_f64);
        let _: f64 = MAX_CO_ACCESS_BOOST; // compile-time type check
        let _: f64 = MAX_BRIEFING_CO_ACCESS_BOOST;
    }
}
