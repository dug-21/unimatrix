# Pseudocode: server-retrieval-integration (C3)

## File: `crates/unimatrix-server/src/server.rs`

### Modified: record_usage_for_entries()

The existing method is modified to pass `confidence::compute_confidence` as the confidence function to the store's `record_usage_with_confidence()`.

```
async function record_usage_for_entries(
    self,
    agent_id: &str,
    trust_level: TrustLevel,
    entry_ids: &[u64],
    helpful: Option<bool>,
    feature: Option<&str>,
):
    if entry_ids is empty:
        return

    // Step 1: Dedup access (unchanged)
    access_ids = self.usage_dedup.filter_access(agent_id, entry_ids)

    // Step 2: Vote actions (unchanged)
    helpful_ids, unhelpful_ids, dec_helpful_ids, dec_unhelpful_ids = ...existing logic...

    // Step 3: Record usage WITH confidence (CHANGED from record_usage)
    store = Arc::clone(self.store)
    all_ids = entry_ids.to_vec()
    // ...clone all owned data...

    usage_result = spawn_blocking(move || {
        store.record_usage_with_confidence(
            &all_ids,
            &access_ids_owned,
            &helpful_owned,
            &unhelpful_owned,
            &dec_helpful_owned,
            &dec_unhelpful_owned,
            Some(&confidence::compute_confidence),  // NEW: pass confidence function
        )
    }).await

    match usage_result:
        Ok(Ok(())) => {}
        Ok(Err(e)) => warn("usage recording failed: {e}")
        Err(e) => warn("usage recording task failed: {e}")

    // Step 4: Feature entries (unchanged)
    ...existing trust-gated feature_entries logic...
```

The ONLY change to this method is replacing `store.record_usage(...)` with `store.record_usage_with_confidence(..., Some(&confidence::compute_confidence))`.

## Import Addition

Add to server.rs imports:
```
use crate::confidence;
```

## Dependencies

- `crate::confidence::compute_confidence` (from C1)
- All existing server.rs dependencies unchanged
