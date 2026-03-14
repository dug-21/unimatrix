# Component: confidence-refresh-batch

**Files**:
- `crates/unimatrix-server/src/infra/coherence.rs` (batch size constant)
- `crates/unimatrix-server/src/services/status.rs` (refresh loop Step 2)

## Purpose

Two modifications to the existing confidence refresh loop in `run_maintenance`:

1. Increase `MAX_CONFIDENCE_REFRESH_BATCH` from 100 to 500 in `coherence.rs`.
2. Add a `std::time::Instant` wall-clock duration guard that breaks the loop
   early if `elapsed > 200ms`, with a log of the partial count.
3. Pass the current `alpha0`/`beta0` snapshot (read from `ConfidenceState` ONCE
   before the loop) to `compute_confidence` inside the loop.

The snapshot-before-loop pattern is mandatory (IR-02): reading `ConfidenceState`
inside the loop would acquire the read lock up to 500 times per tick, serializing
concurrent search calls that need the read lock.

## Modified Constant in coherence.rs

```
// CHANGE: 100 -> 500
pub const MAX_CONFIDENCE_REFRESH_BATCH: usize = 500
```

No other changes to `coherence.rs`.

## Modified Step 2 in run_maintenance

The existing Step 2 block is shown below with the three additions annotated
as NEW-1, NEW-2, NEW-3.

```
// Step 2: Confidence refresh (batch 500, 200ms guard, alpha0/beta0 snapshot)
let mut confidence_refreshed = 0u64
{
    let staleness_threshold = coherence::DEFAULT_STALENESS_THRESHOLD_SECS
    let batch_cap           = coherence::MAX_CONFIDENCE_REFRESH_BATCH  // now 500

    let mut stale_entries: Vec<&EntryRecord> = active_entries
        .iter()
        .filter(|e| {
            let ref_ts = e.updated_at.max(e.last_accessed_at)
            if ref_ts == 0 { return true }
            if now_ts > ref_ts { (now_ts - ref_ts) > staleness_threshold }
            else { false }
        })
        .collect()

    stale_entries.sort_by_key(|e| e.updated_at.max(e.last_accessed_at))
    stale_entries.truncate(batch_cap)

    if !stale_entries.is_empty():
        // NEW-1: Snapshot alpha0/beta0 ONCE before the loop (IR-02)
        let (snapshot_alpha0, snapshot_beta0) = {
            let guard = self.confidence_state
                .read()
                .unwrap_or_else(|e| e.into_inner())
            (guard.alpha0, guard.beta0)
        }

        // NEW-2: Build ids_and_confs with alpha0/beta0 injected
        let ids_and_confs: Vec<(u64, f64)> = stale_entries
            .iter()
            .map(|e| {
                // CHANGED: pass snapshot_alpha0, snapshot_beta0
                let conf = compute_confidence(e, now_ts, snapshot_alpha0, snapshot_beta0)
                (e.id, conf)
            })
            .collect()

        let store_for_refresh = Arc::clone(&self.store)
        let refresh_result = tokio::task::spawn_blocking(move || {
            let mut refreshed = 0u64

            // NEW-3: Wall-clock duration guard (FR-05)
            // The guard is checked BEFORE each update_confidence call (R-13).
            let start = std::time::Instant::now()
            let budget = std::time::Duration::from_millis(200)

            for (id, new_conf) in ids_and_confs:
                // Pre-iteration guard (not post-iteration)
                if start.elapsed() > budget:
                    tracing::info!(
                        "confidence refresh: time budget exhausted after {refreshed} entries"
                    )
                    break

                match store_for_refresh.update_confidence(id, new_conf):
                    Ok(()) => refreshed += 1
                    Err(e) =>
                        tracing::warn!("confidence refresh failed for {id}: {e}")

            refreshed
        }).await

        match refresh_result:
            Ok(count) =>
                report.confidence_refreshed_count = count
                confidence_refreshed = count
            Err(e) =>
                tracing::warn!("confidence refresh task failed: {e}")
}
```

## Notes on Guard Placement (R-13)

The duration guard check:
```
if start.elapsed() > budget { break }
```
MUST appear as the first statement in the loop body, before
`store_for_refresh.update_confidence(id, new_conf)`. This ensures:

- If already over budget at the start of an iteration, break immediately.
- If over budget during a slow `update_confidence` call, the guard fires at the
  start of the NEXT iteration (acceptable — per-call latency for a SQLite
  single-row write is typically < 1ms, so overshoot is negligible).

Do NOT check elapsed after the update call (R-13 risk: the last iteration runs
even when already over budget).

## Notes on ids_and_confs Construction

`compute_confidence` is called in async context (in `run_maintenance`) for the
map step that builds `ids_and_confs`. This is acceptable because
`compute_confidence` is a pure CPU computation with no I/O. The resulting
`Vec<(u64, f64)>` is then moved into `spawn_blocking` for the DB write loop.

This matches the existing pattern in the current code (the `map` producing
`ids_and_confs` was already outside `spawn_blocking`).

## Error Handling

- `store.update_confidence(id, new_conf)` failure: logged at warn level, count
  not incremented, loop continues. Per-entry failure does not abort the batch.
- `spawn_blocking` join failure (`JoinError`): logged at warn level;
  `confidence_refreshed = 0` for this tick.
- Duration budget exhausted: logged at info level with partial count. Next tick
  picks up remaining stale entries (the sort-by-staleness ordering ensures
  oldest entries are processed first).
- `ConfidenceState` read lock poisoned: `unwrap_or_else(|e| e.into_inner())`
  recovers with last-written values (FM-03).

## Key Test Scenarios

```
// Batch size constant (AC-07):
max_confidence_refresh_batch_is_500:
    assert_eq!(MAX_CONFIDENCE_REFRESH_BATCH, 500)

// Duration guard fires pre-iteration (AC-07, R-13):
// This test requires a mock or a very small budget to trigger early exit.
// Approach: construct a large stale entry list and use a 0ms budget.
// The first iteration should observe elapsed > 0ms and break.
confidence_refresh_duration_guard_fires_pre_iteration:
    // Inject a Duration::ZERO budget into the refresh logic for testing.
    // Verify that if start.elapsed() > budget at iteration start, loop breaks
    // before calling update_confidence.
    // Result: refreshed == 0 when budget is immediately exhausted.

// alpha0/beta0 snapshot taken outside loop (IR-02 compliance):
// Verified by code review: look for the snapshot binding before the ids_and_confs map.

// compute_confidence called with non-default prior (R-01 integration):
// This is covered more thoroughly in test-infrastructure.md (IR-01 integration test).
// Unit test here: build a stale entry, call the updated run_maintenance logic
// with a ConfidenceState that has alpha0=8.0, beta0=2.0, and verify the stored
// confidence differs from what cold-start alpha0=3.0, beta0=3.0 would produce
// for an entry with helpful_count=5.
confidence_refresh_uses_empirical_prior:
    let entry = make_test_entry_with_votes(helpful=5, unhelpful=0)
    // Cold-start: h = (5+3)/(5+6) = 0.727
    // Empirical (a0=8, b0=2): h = (5+8)/(5+10) = 0.867 (higher)
    // The refresh should store the higher value when ConfidenceState has a0=8
```
