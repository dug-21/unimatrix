# Pseudocode: episodic (Episodic Augmentation)

## Structs

```
struct EpisodicAugmenter {
    max_boost: f64,          // Maximum score adjustment, default 0.02
    min_affinity: u32,       // Minimum co-access count to consider, default 3
}
```

## Score Adjustment

```
fn EpisodicAugmenter::compute_adjustments(
    &self,
    result_ids: &[u64],
    result_scores: &[f64],
    store: &dyn EntryStore,  // For co-access lookups
    staleness_cutoff: u64,
) -> Vec<f64>:
    // Returns score adjustments for each result
    // This is additive to the existing rerank+co-access score

    if result_ids.len() <= 1:
        return vec![0.0; result_ids.len()]

    let mut adjustments = vec![0.0f64; result_ids.len()]

    // Use top-3 as anchors (same pattern as existing co-access boost)
    let anchor_count = result_ids.len().min(3)

    for i in anchor_count..result_ids.len():
        let result_id = result_ids[i]
        let mut max_affinity = 0u32

        // Check co-access affinity with each anchor
        for j in 0..anchor_count:
            let anchor_id = result_ids[j]
            let pair_key = (min(anchor_id, result_id), max(anchor_id, result_id))
            // Look up co-access count (this uses existing store API)
            if let Ok(Some(record)) = store.get_co_access(pair_key.0, pair_key.1):
                if record.last_updated >= staleness_cutoff:
                    max_affinity = max_affinity.max(record.count)

        if max_affinity >= self.min_affinity:
            // Log-scaled boost, capped at max_boost
            let raw_boost = (max_affinity as f64).ln() / 10.0
            adjustments[i] = raw_boost.min(self.max_boost)

    return adjustments
```

NOTE: Episodic augmentation is the lowest-priority component (SR-04). If scope must be cut, this is the first candidate. The existing co-access boost in coaccess.rs already provides similar functionality. This component adds a refinement layer on top.

The implementation may choose to defer episodic augmentation to a follow-up if the scope proves too large, replacing it with a no-op stub that returns zero adjustments.
