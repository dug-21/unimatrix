# Pseudocode: C4 Coherence Module

## Purpose

New module with pure dimension score functions, composite lambda computation, and maintenance recommendation generation. All functions are pure (no I/O, deterministic).

## Files Created

- `crates/unimatrix-server/src/coherence.rs`

## Files Modified

- `crates/unimatrix-server/src/lib.rs` -- add `pub mod coherence;`

## Named Constants

```
pub const DEFAULT_STALENESS_THRESHOLD_SECS: u64 = 24 * 3600;
pub const DEFAULT_STALE_RATIO_TRIGGER: f64 = 0.10;
pub const DEFAULT_LAMBDA_THRESHOLD: f64 = 0.8;
pub const MAX_CONFIDENCE_REFRESH_BATCH: usize = 100;

pub const DEFAULT_WEIGHTS: CoherenceWeights = CoherenceWeights {
    confidence_freshness: 0.35,
    graph_quality: 0.30,
    embedding_consistency: 0.15,
    contradiction_density: 0.20,
};
```

## CoherenceWeights Struct

```
pub struct CoherenceWeights {
    pub confidence_freshness: f64,
    pub graph_quality: f64,
    pub embedding_consistency: f64,
    pub contradiction_density: f64,
}
```

## Dimension Score Functions

### confidence_freshness_score

```
pub fn confidence_freshness_score(
    entries: &[EntryRecord], now: u64, staleness_threshold_secs: u64,
) -> (f64, u64):
    if entries.is_empty(): return (1.0, 0)
    stale_count = 0u64
    for entry in entries:
        reference = max(entry.updated_at, entry.last_accessed_at)
        if reference == 0:
            stale_count += 1; continue
        if now > reference && (now - reference) > staleness_threshold_secs:
            stale_count += 1
    total = entries.len() as u64
    score = (total - stale_count) as f64 / total as f64
    return (score, stale_count)
```

### graph_quality_score

```
pub fn graph_quality_score(stale_count: usize, point_count: usize) -> f64:
    if point_count == 0: return 1.0
    score = 1.0 - (stale_count as f64 / point_count as f64)
    return score.clamp(0.0, 1.0)
```

### embedding_consistency_score

```
pub fn embedding_consistency_score(inconsistent_count: usize, total_checked: usize) -> f64:
    if total_checked == 0: return 1.0
    score = 1.0 - (inconsistent_count as f64 / total_checked as f64)
    return score.clamp(0.0, 1.0)
```

### contradiction_density_score

```
pub fn contradiction_density_score(total_quarantined: u64, total_active: u64) -> f64:
    if total_active == 0: return 1.0
    score = 1.0 - (total_quarantined as f64 / total_active as f64)
    return score.clamp(0.0, 1.0)
```

## compute_lambda

```
pub fn compute_lambda(
    freshness: f64, graph_quality: f64,
    embedding_consistency: Option<f64>, contradiction_density: f64,
    weights: &CoherenceWeights,
) -> f64:
    match embedding_consistency:
        Some(embed_score):
            lambda = weights.confidence_freshness * freshness
                   + weights.graph_quality * graph_quality
                   + weights.embedding_consistency * embed_score
                   + weights.contradiction_density * contradiction_density
            return lambda.clamp(0.0, 1.0)
        None:
            remaining = weights.confidence_freshness
                      + weights.graph_quality
                      + weights.contradiction_density
            if remaining <= 0.0: return 1.0
            lambda = (weights.confidence_freshness * freshness
                    + weights.graph_quality * graph_quality
                    + weights.contradiction_density * contradiction_density)
                   / remaining
            return lambda.clamp(0.0, 1.0)
```

## oldest_stale_age (helper)

```
pub fn oldest_stale_age(
    entries: &[EntryRecord], now: u64, staleness_threshold_secs: u64,
) -> u64:
    oldest = 0u64
    for entry in entries:
        reference = max(entry.updated_at, entry.last_accessed_at)
        age = if reference == 0 && now > 0 { now } else if now > reference { now - reference } else { 0 }
        if age > staleness_threshold_secs:
            oldest = max(oldest, age)
    return oldest
```

## generate_recommendations

```
pub fn generate_recommendations(
    lambda: f64, threshold: f64,
    stale_confidence_count: u64, oldest_stale_age_secs: u64,
    graph_stale_ratio: f64, embedding_inconsistent_count: usize,
    total_quarantined: u64,
) -> Vec<String>:
    if lambda >= threshold: return vec![]

    recs = vec![]
    if stale_confidence_count > 0:
        days = oldest_stale_age_secs / 86400
        recs.push("{stale_confidence_count} entries have stale confidence (oldest: {days} days) -- run with maintain: true to refresh")
    if graph_stale_ratio > DEFAULT_STALE_RATIO_TRIGGER:
        pct = (graph_stale_ratio * 100.0) as u64
        recs.push("HNSW graph has {pct}% stale nodes -- run with maintain: true to compact")
    if embedding_inconsistent_count > 0:
        recs.push("{embedding_inconsistent_count} embedding inconsistencies detected")
    if total_quarantined > 0:
        recs.push("{total_quarantined} entries quarantined -- review for resolution")
    return recs
```

## Key Test Scenarios

### Dimension Scores (R-10)
1-3: confidence_freshness: empty->1.0, all stale->0.0, none stale->1.0
4: Uses max(updated_at, last_accessed_at) for staleness
5-7: graph_quality: 0 points->1.0, stale>total->clamped 0.0, 0 stale->1.0
8-9: embedding_consistency: 0 checked->1.0, all inconsistent->0.0
10-11: contradiction_density: 0 active->1.0, quarantined>active->clamped 0.0

### Lambda (R-05)
12-17: Weighted sum, re-normalization, all-1.0, all-0.0, weight sum invariant

### Recommendations (R-20)
18-22: >= threshold->empty, < threshold->specific recs, exactly 0.8->empty
