# Pseudocode: C4 -- Co-Access Boost Module

## Crate: unimatrix-server

### New module: coaccess.rs

```rust
use std::collections::HashMap;
use unimatrix_store::Store;

/// Maximum entries to consider for co-access pair generation.
/// 10 entries = 45 pairs. Beyond this, pairs are not generated.
pub const MAX_CO_ACCESS_ENTRIES: usize = 10;

/// Default staleness threshold for co-access pairs: 30 days in seconds.
pub const CO_ACCESS_STALENESS_SECONDS: u64 = 30 * 24 * 3600; // 2_592_000

/// Maximum additive boost for search results from co-access signal.
pub const MAX_CO_ACCESS_BOOST: f32 = 0.03;

/// Maximum additive boost for briefing results from co-access signal.
pub const MAX_BRIEFING_CO_ACCESS_BOOST: f32 = 0.01;

/// Co-access count beyond which boost is fully saturated (log-transform denominator).
pub const MAX_MEANINGFUL_CO_ACCESS: f64 = 20.0;

/// Partner count beyond which co-access affinity is fully saturated.
pub const MAX_MEANINGFUL_PARTNERS: f64 = 10.0;

/// Generate ordered pairs from entry IDs, considering at most max_entries.
/// Returns Vec<(min_id, max_id)>.
///
/// Pair count: min(len, max_entries) choose 2 = k*(k-1)/2.
/// At max_entries=10, this is at most 45 pairs.
pub fn generate_pairs(entry_ids: &[u64], max_entries: usize) -> Vec<(u64, u64)> {
    let effective = &entry_ids[..entry_ids.len().min(max_entries)];
    let mut pairs = Vec::with_capacity(effective.len() * (effective.len().saturating_sub(1)) / 2);

    for i in 0..effective.len() {
        for j in (i + 1)..effective.len() {
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
///   raw = ln(1 + count) / ln(1 + MAX_MEANINGFUL_CO_ACCESS)
///   boost = min(raw, 1.0) * max_boost
///
/// Returns a value in [0.0, max_boost].
fn co_access_boost(count: u32, max_boost: f32) -> f32 {
    if count == 0 {
        return 0.0;
    }
    let raw = (1.0 + count as f64).ln() / (1.0 + MAX_MEANINGFUL_CO_ACCESS).ln();
    let capped = raw.min(1.0);
    (capped * max_boost as f64) as f32
}

/// Compute co-access boost scores for search results.
///
/// Takes anchor IDs (top results) and all result IDs.
/// For each result that is a co-access partner of any anchor, computes a boost.
/// If a result is a partner of multiple anchors, takes the maximum boost.
///
/// Returns HashMap<entry_id, boost> where values are in [0.0, MAX_CO_ACCESS_BOOST].
pub fn compute_search_boost(
    anchor_ids: &[u64],
    result_ids: &[u64],
    store: &Store,
    staleness_cutoff: u64,
) -> HashMap<u64, f32> {
    compute_boost_internal(anchor_ids, result_ids, store, staleness_cutoff, MAX_CO_ACCESS_BOOST)
}

/// Compute co-access boost scores for briefing results.
/// Same algorithm as search but with smaller max boost.
pub fn compute_briefing_boost(
    anchor_ids: &[u64],
    result_ids: &[u64],
    store: &Store,
    staleness_cutoff: u64,
) -> HashMap<u64, f32> {
    compute_boost_internal(anchor_ids, result_ids, store, staleness_cutoff, MAX_BRIEFING_CO_ACCESS_BOOST)
}

/// Internal boost computation shared by search and briefing.
fn compute_boost_internal(
    anchor_ids: &[u64],
    result_ids: &[u64],
    store: &Store,
    staleness_cutoff: u64,
    max_boost: f32,
) -> HashMap<u64, f32> {
    let mut boost_map: HashMap<u64, f32> = HashMap::new();

    // Build a set of result IDs for quick membership check
    let result_set: std::collections::HashSet<u64> = result_ids.iter().copied().collect();

    for &anchor_id in anchor_ids {
        // Get co-access partners for this anchor
        let partners = match store.get_co_access_partners(anchor_id, staleness_cutoff) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!("co-access partner lookup failed for {anchor_id}: {e}");
                continue;
            }
        };

        for (partner_id, record) in partners {
            // Only boost results that are in the current result set
            if !result_set.contains(&partner_id) {
                continue;
            }
            // Skip if partner is the anchor itself (should not happen with ordered keys, but defensive)
            if partner_id == anchor_id {
                continue;
            }

            let boost = co_access_boost(record.count, max_boost);

            // Take maximum boost across all anchors
            let existing = boost_map.entry(partner_id).or_insert(0.0);
            if boost > *existing {
                *existing = boost;
            }
        }
    }

    boost_map
}
```

Key design notes:
- `generate_pairs` is used by both C3 (recording) and tests
- `co_access_boost` is the ADR-002 log-transform formula, private but tested via public functions
- `compute_search_boost` and `compute_briefing_boost` differ only in max_boost constant
- When a result is a partner of multiple anchors, the maximum boost wins (not additive -- prevents multi-anchor inflation)
- Partner lookup failures are logged and skipped (graceful degradation per R-03)
- The result_set membership check avoids computing boost for entries not in the current result set
