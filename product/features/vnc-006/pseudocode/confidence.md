# Pseudocode: ConfidenceService (services/confidence.rs)

## Struct

```
struct ConfidenceService {
    store: Arc<Store>,
}
```

## Constructor

```
fn new(store: Arc<Store>) -> Self:
    ConfidenceService { store }
```

## recompute()

```
fn recompute(&self, entry_ids: &[u64]):
    // No-op for empty slice (FR-03.4)
    if entry_ids.is_empty():
        return

    let store = Arc::clone(&self.store)
    let ids = entry_ids.to_vec()

    // Single spawn_blocking for the batch (ADR-004)
    let _ = tokio::task::spawn_blocking(move || {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()

        for id in ids:
            match store.get(id):
                Ok(entry) =>
                    let conf = unimatrix_engine::confidence::compute_confidence(&entry, now)
                    if let Err(e) = store.update_confidence(id, conf):
                        tracing::warn!("confidence recompute failed for {id}: {e}")
                Err(e) =>
                    tracing::warn!("confidence recompute: entry {id} not found: {e}")
    })
```

## Notes

- This replaces 5 blocks in tools.rs and 3 blocks in uds_listener.rs (via run_confidence_consumer).
- The confidence blocks in uds_listener.rs are part of `run_confidence_consumer` which handles signal-driven confidence updates. Those remain separate -- they use `record_usage_with_confidence` which has different semantics (combines usage recording with confidence).
- The 5 blocks in tools.rs that match this pattern: context_store (line 682-701), context_correct (lines 929-940), context_deprecate (line 1028), context_status maintain (lines 1478, 1990, 2040), context_briefing (line 2560).
- Actually, looking more carefully: some of these are NOT simple fire-and-forget confidence recomputes. The context_status maintain=true path does batch confidence refresh via a different mechanism. Only the direct `compute_confidence` + `update_confidence` blocks in mutation handlers should be replaced.
- Precise replacement targets in tools.rs: context_store (1 block), context_correct (2 blocks -- new + deprecated), context_deprecate (1 block), context_quarantine (could benefit but currently doesn't recompute).
- UDS: the confidence consumer is NOT replaced -- it serves a different purpose (signal-driven usage+confidence).
- Total replacement count may be fewer than 8 originally estimated; the exact count will be determined during implementation.
