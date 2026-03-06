# C1: RetrievalMode + SearchService Status Logic — Pseudocode

## Location
`crates/unimatrix-server/src/services/search.rs`

## New Types

```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RetrievalMode {
    Strict,
    #[default]
    Flexible,
}
```

## Modified: `ServiceSearchParams`

```
pub(crate) struct ServiceSearchParams {
    // ... existing fields unchanged ...
    pub(crate) retrieval_mode: RetrievalMode,  // NEW — defaults to Flexible
}
```

## Modified: `SearchService::search`

After Step 6 (fetch entries, exclude quarantined) and before Step 7 (re-rank):

```
// Step 6a: Status filter / penalty marking
//
// Determine if caller explicitly requested a specific status
let explicit_status_filter: Option<Status> = params.filters
    .as_ref()
    .and_then(|f| f.status)
    .filter(|s| *s != Status::Active)  // Active is default, not explicit

// Track penalties for Step 7
struct PenaltyEntry {
    penalty: f64,  // 1.0 = no penalty
}
let mut penalty_map: HashMap<u64, f64> = HashMap::new()

match params.retrieval_mode:
    RetrievalMode::Strict:
        // Hard filter: drop all non-Active and all superseded
        results_with_scores.retain(|(entry, _sim)|
            entry.status == Status::Active && entry.superseded_by.is_none()
        )

    RetrievalMode::Flexible:
        if explicit_status_filter.is_some():
            // Agent explicitly requested a status (e.g., Deprecated)
            // No penalties applied — agent knows what they want (FR-6.2)
            pass
        else:
            // Apply penalty markers (actual penalty applied in Step 7)
            for (entry, _sim) in &results_with_scores:
                if entry.superseded_by.is_some():
                    penalty_map.insert(entry.id, SUPERSEDED_PENALTY)  // 0.5
                else if entry.status == Status::Deprecated:
                    penalty_map.insert(entry.id, DEPRECATED_PENALTY)  // 0.7

// Step 6b: Supersession injection (C2 — see c2-supersession-injection.md)
// Integrated here but detailed in its own pseudocode file
```

## Step 7 Modification: Apply penalties during re-rank

```
// Step 7: Re-rank with penalties
results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)|
    prov_a = PROVENANCE_BOOST if entry_a.category == "lesson-learned" else 0.0
    prov_b = PROVENANCE_BOOST if entry_b.category == "lesson-learned" else 0.0
    base_a = rerank_score(*sim_a, entry_a.confidence) + prov_a
    base_b = rerank_score(*sim_b, entry_b.confidence) + prov_b
    // NEW: apply penalty multipliers
    penalty_a = penalty_map.get(&entry_a.id).copied().unwrap_or(1.0)
    penalty_b = penalty_map.get(&entry_b.id).copied().unwrap_or(1.0)
    final_a = base_a * penalty_a
    final_b = base_b * penalty_b
    final_b.partial_cmp(&final_a)
)
```

## Step 8 Modification: Co-access with deprecated exclusion

```
// Step 8: Co-access boost with deprecated exclusion (C3)
if results_with_scores.len() > 1:
    // Collect deprecated IDs from current result set
    deprecated_ids: HashSet<u64> = results_with_scores
        .iter()
        .filter(|(e, _)| e.status == Status::Deprecated)
        .map(|(e, _)| e.id)
        .collect()

    // ... existing anchor/result ID collection ...

    boost_map = compute_search_boost(
        &anchor_ids, &result_ids, &store, staleness_cutoff,
        &deprecated_ids  // NEW parameter
    )

    // Apply boost with penalties
    if !boost_map.is_empty():
        results_with_scores.sort_by(|(entry_a, sim_a), (entry_b, sim_b)|
            base_a = rerank_score(*sim_a, entry_a.confidence)
            base_b = rerank_score(*sim_b, entry_b.confidence)
            boost_a = boost_map.get(&entry_a.id).copied().unwrap_or(0.0)
            boost_b = boost_map.get(&entry_b.id).copied().unwrap_or(0.0)
            prov_a = PROVENANCE_BOOST if lesson-learned else 0.0
            prov_b = PROVENANCE_BOOST if lesson-learned else 0.0
            // NEW: apply penalty to (base + boost + prov)
            penalty_a = penalty_map.get(&entry_a.id).copied().unwrap_or(1.0)
            penalty_b = penalty_map.get(&entry_b.id).copied().unwrap_or(1.0)
            final_a = (base_a + boost_a + prov_a) * penalty_a
            final_b = (base_b + boost_b + prov_b) * penalty_b
            final_b.partial_cmp(&final_a)
        )
```

## Step 11 Modification: Include penalty in ScoredEntry

```
// Step 11: Build ScoredEntry with penalty-adjusted final_score
entries = results_with_scores.iter().map(|(entry, sim)|
    penalty = penalty_map.get(&entry.id).copied().unwrap_or(1.0)
    ScoredEntry {
        entry: entry.clone(),
        final_score: rerank_score(*sim, entry.confidence) * penalty,
        similarity: *sim,
        confidence: entry.confidence,
    }
)
```

## Imports Added

```rust
use std::collections::HashSet;
use unimatrix_engine::confidence::{DEPRECATED_PENALTY, SUPERSEDED_PENALTY, cosine_similarity};
use unimatrix_core::Status;
```

## Key Design Points

- `RetrievalMode` defaults to `Flexible` — backward compatible (ADR-001, SR-09)
- Penalties are multiplicative on the final score, not additive
- `explicit_status_filter` detection: if `QueryFilter.status` is set to something other than `Active`, it is considered explicit
- Empty results in Strict mode return empty vec — no fallback (FR-1.5)
- Penalty map is computed once, used in both Step 7 and Step 8 sorts
