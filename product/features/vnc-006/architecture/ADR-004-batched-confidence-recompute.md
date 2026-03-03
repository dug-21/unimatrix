## ADR-004: Batched Confidence Recomputation

### Context

Confidence recomputation currently happens in 8 separate fire-and-forget `spawn_blocking` blocks scattered across tools.rs (5 blocks) and uds_listener.rs (3 blocks). Each block:

1. Clones `Arc<Store>`
2. Spawns a blocking task
3. Gets the entry by ID
4. Calls `compute_confidence()`
5. Calls `store.update_confidence()`

The blocks are nearly identical, differing only in which entry IDs they operate on. Some mutations (e.g., context_correct) need to recompute confidence for multiple entries (the new correction and the deprecated original).

### Decision

Consolidate into `ConfidenceService::recompute(entry_ids: &[u64])` with batched execution:

- Single `spawn_blocking` call per batch
- Single iteration over entry IDs within the blocking task
- Per-entry failure is logged and skipped (consistent with existing fire-and-forget contract per Unimatrix ADR #53)
- No change to the confidence computation algorithm itself

```rust
impl ConfidenceService {
    pub(crate) fn recompute(&self, entry_ids: &[u64]) {
        if entry_ids.is_empty() { return; }
        let store = Arc::clone(&self.store);
        let ids = entry_ids.to_vec();
        let _ = tokio::task::spawn_blocking(move || {
            let now = current_timestamp_secs();
            for id in ids {
                match store.get(id) {
                    Ok(entry) => {
                        let conf = unimatrix_core::compute_confidence(&entry, now);
                        if let Err(e) = store.update_confidence(id, conf) {
                            tracing::warn!("confidence recompute failed for {id}: {e}");
                        }
                    }
                    Err(e) => tracing::warn!("confidence recompute: entry {id} not found: {e}"),
                }
            }
        });
    }
}
```

This aligns with crt-005's batch refresh pattern in the context_status maintain=true path, which already iterates over multiple entries in a single spawn_blocking call.

### Consequences

- **Easier**: Adding confidence recomputation to new mutation paths (one line: `self.services.confidence.recompute(&[id])`). Batch mutations (e.g., bulk deprecate) naturally batch recomputation.
- **Harder**: Timing semantics change slightly — previously 8 independent tasks could run concurrently, now a batch is sequential within one task. In practice, confidence recomputation is <1ms per entry, so the difference is negligible.
