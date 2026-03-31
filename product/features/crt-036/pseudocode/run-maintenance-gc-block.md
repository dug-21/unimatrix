# crt-036: run_maintenance GC Block — Pseudocode

**File:** `crates/unimatrix-server/src/services/status.rs`
**Action:** Replace step 4 (60-day DELETE) with cycle-based GC block + add `retention_config` param

---

## Purpose

Step 4 in `run_maintenance()` currently holds a 60-day wall-clock `DELETE FROM observations`.
This replaces that block entirely with the cycle-based GC orchestration loop plus the new
step 4f for `audit_log`. The PhaseFreqTable alignment guard (ADR-003) runs before the
cycle loop as part of step 4. Step 6 (`gc_sessions`) is unchanged and continues to run.

---

## Signature Change for run_maintenance()

```
// Current signature (line ~992):
pub(crate) async fn run_maintenance(
    &self,
    active_entries: &[EntryRecord],
    report: &mut StatusReport,
    session_registry: &SessionRegistry,
    entry_store: &Arc<Store>,
    pending_entries_analysis: &Arc<std::sync::Mutex<PendingEntriesAnalysis>>,
    inference_config: &InferenceConfig,
) -> Result<MaintenanceResult, ServiceError>

// New signature (add retention_config as final parameter):
pub(crate) async fn run_maintenance(
    &self,
    active_entries: &[EntryRecord],
    report: &mut StatusReport,
    session_registry: &SessionRegistry,
    entry_store: &Arc<Store>,
    pending_entries_analysis: &Arc<std::sync::Mutex<PendingEntriesAnalysis>>,
    inference_config: &InferenceConfig,
    retention_config: &RetentionConfig,   // NEW — crt-036
) -> Result<MaintenanceResult, ServiceError>
```

The `RetentionConfig` type must be imported at the top of `status.rs`:

```
use crate::infra::config::RetentionConfig;
```

---

## Step 4 Block Replacement

Replace the existing block at lines 1372–1384:

```
// OLD (remove entirely):
// 4. Observation retention cleanup (col-012: SQL DELETE)
{
    let now_millis = SystemTime::now()...;
    let sixty_days_millis = 60_i64 * 24 * 60 * 60 * 1000;
    let cutoff = now_millis - sixty_days_millis;
    let _ = sqlx::query("DELETE FROM observations WHERE ts_millis < ?1")
        .bind(cutoff)
        .execute(self.store.write_pool_server())
        .await;
}
```

Replace with:

```
// 4. Cycle-based activity GC (crt-036: replaces 60-day DELETE)
{
    let k = retention_config.activity_detail_retention_cycles;
    let max_per_tick = retention_config.max_cycles_per_tick;

    // Resolve purgeable cycles and oldest retained computed_at for alignment check.
    // list_purgeable_cycles returns (purgeable: Vec<String>, oldest_retained: Option<i64>).
    // Errors here are non-fatal: log warn and skip the entire GC block this tick.
    let (purgeable_cycles, oldest_retained_computed_at) = match self
        .store
        .list_purgeable_cycles(k, max_per_tick)
        .await
    {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!(error = %e, "cycle GC: list_purgeable_cycles failed; skipping GC this tick");
            // Skip to step 4f (audit_log GC still runs)
            goto_step_4f;  // [implementation note: use early return or restructure with a local async fn]
        }
    };

    // [PhaseFreqTable alignment guard — see phase-freq-table-guard.md for full logic]
    // Emits tracing::warn! if query_log_lookback_days implies a window older than the
    // oldest retained cycle's computed_at. No action taken — advisory only (ADR-003).
    // Skipped when oldest_retained_computed_at is None (fewer than K cycles reviewed).
    run_phase_freq_table_alignment_check(
        &oldest_retained_computed_at,
        inference_config.query_log_lookback_days,
        k,
    );

    let purgeable_count = purgeable_cycles.len();
    tracing::info!(
        k = k,
        purgeable_count = purgeable_count,
        capped_to = max_per_tick,
        "cycle GC: pass starting"
    );

    let mut cycles_pruned: u32 = 0;
    let mut cycles_skipped: u32 = 0;
    let mut total_rows_deleted: u64 = 0;

    // 4a-4e: Per-cycle loop
    for cycle_id in &purgeable_cycles {

        // 4a. crt-033 gate check: verify cycle_review_index row exists.
        // Record is retained in scope for use in step 4c (raw_signals_available update).
        let record = match self.store.get_cycle_review(cycle_id).await {
            Ok(Some(r)) => r,   // gate passed: record retained in scope
            Ok(None) => {
                // Defense-in-depth: cycle was in purgeable set but has no review row.
                // Should not normally happen; skip and log warn (FR-04).
                tracing::warn!(
                    cycle_id = %cycle_id,
                    reason = "no cycle_review_index row",
                    "cycle GC: gate skip"
                );
                cycles_skipped += 1;
                continue;
            }
            Err(e) => {
                // Transient read failure: skip cycle, continue to next (FR-04).
                tracing::warn!(
                    cycle_id = %cycle_id,
                    error = %e,
                    "cycle GC: gate check error, skipping cycle"
                );
                cycles_skipped += 1;
                continue;
            }
        };

        // 4b. Execute per-cycle transaction: DELETE observations, query_log,
        //     injection_log, sessions for this cycle. Connection released on return.
        let stats = match self.store.gc_cycle_activity(cycle_id).await {
            Ok(s) => s,
            Err(e) => {
                // Transaction rolled back. Cycle will be retried on next tick.
                // Do NOT call store_cycle_review() — data is still present.
                tracing::warn!(
                    cycle_id = %cycle_id,
                    error = %e,
                    "cycle GC: gc_cycle_activity failed; cycle deferred to next tick"
                );
                cycles_skipped += 1;
                continue;
            }
        };

        // 4c. Update raw_signals_available = 0 using the record retained from 4a.
        //     This runs OUTSIDE the per-cycle transaction (store_cycle_review takes &self,
        //     not a transaction handle). Uses struct update syntax to preserve summary_json
        //     and all other fields (SR-05 mitigation, ADR-001 consequences).
        if let Err(e) = self
            .store
            .store_cycle_review(&crate::store::CycleReviewRecord {
                raw_signals_available: 0,
                ..record
            })
            .await
        {
            // Non-fatal: GC data was deleted successfully; flag update failed.
            // raw_signals_available stays 1 (stale). A future scan can repair.
            tracing::warn!(
                cycle_id = %cycle_id,
                error = %e,
                "cycle GC: store_cycle_review raw_signals_available=0 failed (data deleted)"
            );
        }

        // 4d. Log per-cycle info.
        tracing::info!(
            cycle_id = %cycle_id,
            observations_deleted = stats.observations_deleted,
            query_log_deleted = stats.query_log_deleted,
            injection_log_deleted = stats.injection_log_deleted,
            sessions_deleted = stats.sessions_deleted,
            "cycle GC: cycle pruned"
        );

        let cycle_total = stats.observations_deleted
            + stats.query_log_deleted
            + stats.injection_log_deleted
            + stats.sessions_deleted;
        total_rows_deleted += cycle_total;
        cycles_pruned += 1;
    }

    // 4e. Unattributed cleanup (runs after cycle loop regardless of cap).
    match self.store.gc_unattributed_activity().await {
        Ok(ua) => {
            tracing::info!(
                observations_deleted = ua.observations_deleted,
                query_log_deleted = ua.query_log_deleted,
                sessions_deleted = ua.sessions_deleted,
                injection_log_deleted = ua.injection_log_deleted,
                "cycle GC: unattributed cleanup"
            );
            total_rows_deleted += ua.observations_deleted
                + ua.query_log_deleted
                + ua.sessions_deleted
                + ua.injection_log_deleted;
        }
        Err(e) => {
            tracing::warn!(error = %e, "cycle GC: gc_unattributed_activity failed");
        }
    }

    tracing::info!(
        cycles_pruned = cycles_pruned,
        cycles_skipped = cycles_skipped,
        total_rows_deleted = total_rows_deleted,
        "cycle GC: pass complete"
    );
}

// 4f. audit_log time-based GC (independent of cycle GC; "4f" avoids sub-step collision).
{
    match self
        .store
        .gc_audit_log(retention_config.audit_log_retention_days)
        .await
    {
        Ok(rows) => {
            tracing::info!(
                rows_deleted = rows,
                cutoff_days = retention_config.audit_log_retention_days,
                "cycle GC: audit_log cleanup"
            );
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                "cycle GC: gc_audit_log failed"
            );
        }
    }
}
```

---

## Implementation Notes

**Goto restructuring:** Rust has no `goto`. The "skip to step 4f" path for
`list_purgeable_cycles` failure should be handled by wrapping the cycle loop in an
`async` closure or an inner labeled block. Example pattern:

```
'gc_cycle_block: {
    let (purgeable_cycles, oldest_retained) = match self.store.list_purgeable_cycles(k, max_per_tick).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(...);
            break 'gc_cycle_block;
        }
    };
    // ... rest of cycle loop ...
}
// 4f continues here unconditionally
```

**Import for CycleReviewRecord:** The `store_cycle_review()` call in step 4c uses
`CycleReviewRecord` from `unimatrix_store`. Import path:
`use unimatrix_store::cycle_review_index::CycleReviewRecord;`
(Verify this matches the actual pub use path in the store crate.)

**run_maintenance_simple test helper:** The test helper at line ~2158 that calls
`run_maintenance()` must be updated to pass a `RetentionConfig::default()` as the new
final argument. All existing test invocations of `run_maintenance()` need this update.

---

## PhaseFreqTable Alignment Check Helper

The alignment check is documented in detail in `phase-freq-table-guard.md`. In
`status.rs` it is called as an inline block or private helper:

```
fn run_phase_freq_table_alignment_check(
    oldest_retained_computed_at: &Option<i64>,
    query_log_lookback_days: u32,
    activity_detail_retention_cycles: u32,
)
```

See `phase-freq-table-guard.md` for the full logic.

---

## background.rs Threading

`run_single_tick()` in `background.rs` must be updated to:
1. Accept `retention_config: &Arc<RetentionConfig>` as a parameter alongside
   `inference_config: &Arc<InferenceConfig>`.
2. Pass `retention_config` (dereffed: `&**retention_config` or `retention_config.as_ref()`)
   to `run_maintenance()` call site.

The tick loop function (the outer loop that calls `run_single_tick`) also receives
`Arc<RetentionConfig>` and passes it through by `Arc::clone`.

The call site in `run_maintenance_with_status()` (line ~956) currently calls:
```
status_svc.run_maintenance(
    &active_entries,
    &mut report,
    session_registry,
    entry_store,
    pending_entries,
    inference_config,
).await?;
```

Becomes:
```
status_svc.run_maintenance(
    &active_entries,
    &mut report,
    session_registry,
    entry_store,
    pending_entries,
    inference_config,
    retention_config,   // NEW — pass as &RetentionConfig (deref from Arc)
).await?;
```

Threading change in `run_tick_loop()` function signature (the outer tick function):
```
// Add alongside existing Arc<InferenceConfig> parameter:
retention_config: Arc<RetentionConfig>,
```

---

## Error Handling Summary

| Failure point | Behavior |
|---------------|----------|
| `list_purgeable_cycles` fails | Log warn; skip cycle loop; step 4f (audit_log) still runs |
| `get_cycle_review` returns `Ok(None)` | Log warn; skip this cycle; continue loop |
| `get_cycle_review` returns `Err` | Log warn; skip this cycle; continue loop |
| `gc_cycle_activity` returns `Err` | Log warn; do NOT call `store_cycle_review`; continue loop |
| `store_cycle_review` (flag update) fails | Log warn; non-fatal; continue loop |
| `gc_unattributed_activity` fails | Log warn; continue to step 4f |
| `gc_audit_log` fails | Log warn; non-fatal; maintenance continues |

---

## Key Test Scenarios

- GC pass with N = 5 purgeable cycles: assert `cycles_pruned = 5` in log (AC-15).
- Gate-skipped cycle (no review row): assert warn log with cycle ID (AC-15, R-05).
- `max_cycles_per_tick = 5` with 20 purgeable cycles: first tick prunes 5, 15 remain (AC-16, R-08).
- Oldest cycles processed first: cycle with lowest `computed_at` pruned before more recent ones (AC-16).
- `store_cycle_review` failure after `gc_cycle_activity` success: data deleted, flag stays 1, no panic (R-13).
- All protected tables (entries, GRAPH_EDGES, cycle_events, observation_phase_metrics) unchanged
  after GC pass (AC-03, AC-14, R-14).
- `summary_json` byte-identical before and after GC for pruned cycle (AC-05, R-03).
- `raw_signals_available = 0` for pruned cycle; `= 1` (unchanged) for retained cycle (AC-05).
- Concurrent write during multi-cycle GC: independent write completes without timeout (R-04).
