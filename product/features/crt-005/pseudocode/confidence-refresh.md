# Pseudocode: C5 Confidence Refresh

## Purpose

Recompute stale confidence values during `context_status` when `maintain: true`. Capped at MAX_CONFIDENCE_REFRESH_BATCH (100) per call.

## Files Modified

- `crates/unimatrix-server/src/tools.rs` -- context_status handler

## Integration Point

Added after dimension score computation, before report build. Runs only when maintain=true.

## Pseudocode

```
// Inside context_status handler:

let confidence_refreshed_count: u64;

if maintain_enabled:
    // Identify stale entries using same logic as confidence_freshness_score
    stale_entries: Vec<&EntryRecord> = all_active_entries.iter()
        .filter(|e| {
            let ref_ts = max(e.updated_at, e.last_accessed_at);
            if ref_ts == 0: return true
            if now > ref_ts: (now - ref_ts) > DEFAULT_STALENESS_THRESHOLD_SECS
            else: false
        })
        .collect()

    // Sort oldest first (lowest reference timestamp)
    stale_entries.sort_by_key(|e| max(e.updated_at, e.last_accessed_at))

    // Cap at batch size
    batch = &stale_entries[..min(stale_entries.len(), MAX_CONFIDENCE_REFRESH_BATCH)]

    refreshed = 0u64
    for entry in batch:
        new_conf = compute_confidence(entry, now)
        match store.update_confidence(entry.id, new_conf):
            Ok(()) => refreshed += 1
            Err(e) => warn!("confidence refresh failed for {}: {}", entry.id, e)

    confidence_refreshed_count = refreshed
else:
    confidence_refreshed_count = 0
```

## Error Handling

- Individual update_confidence failures: logged, skipped, batch continues
- confidence_refreshed_count reflects only successes
- context_status always completes successfully

## Key Test Scenarios

1. maintain=true + stale entries: refreshed_count > 0 (AC-09)
2. maintain=false: refreshed_count == 0 (R-07)
3. 200 stale: max 100 refreshed (R-08, AC-19)
4. 50 stale: all 50 refreshed
5. Second call refreshes remaining
6. Oldest-first ordering
7. Individual failure does not abort batch
