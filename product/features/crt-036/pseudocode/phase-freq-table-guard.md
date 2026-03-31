# crt-036: PhaseFreqTable Alignment Guard — Pseudocode

**File:** `crates/unimatrix-server/src/services/status.rs`
**Location:** Inside step 4 of `run_maintenance()`, before the per-cycle loop.

---

## Purpose

Emit `tracing::warn!` when the `query_log_lookback_days` from `InferenceConfig`
implies a data window that extends before the oldest retained cycle's `computed_at`.
This is a tick-time advisory only — no startup rejection, no blocking of GC execution.
(ADR-003, FR-10)

The guard resolves the SR-07 risk: after crt-036 ships, `query_log` rows for cycles
outside the K window are deleted. If `query_log_lookback_days = 365` but the oldest
retained cycle was reviewed 30 days ago, `PhaseFreqTable::rebuild` will silently see
a truncated window. The warning makes this mismatch visible in the tick log.

---

## Data Inputs

This guard receives its inputs as a by-product of `list_purgeable_cycles()`.
That method returns `(Vec<String>, Option<i64>)` where the second element is the
`computed_at` of the K-th most recently reviewed cycle (the oldest cycle in the
K-window). When fewer than K cycles have been reviewed, it returns `None` —
no pruning has occurred yet, so no gap can exist.

```
oldest_retained_computed_at: Option<i64>  -- from list_purgeable_cycles result
query_log_lookback_days: u32              -- from inference_config.query_log_lookback_days
activity_detail_retention_cycles: u32    -- from retention_config (for log field only)
```

---

## Algorithm

```
fn run_phase_freq_table_alignment_check(
    oldest_retained_computed_at: &Option<i64>,
    query_log_lookback_days: u32,
    activity_detail_retention_cycles: u32,
) {
    // Guard: if fewer than K cycles reviewed, no pruning has occurred.
    // No gap is possible — skip the check entirely (ADR-003).
    let oldest = match oldest_retained_computed_at {
        Some(ts) => *ts,
        None => return,   // fewer than K cycles: no warning, no action
    };

    // Compute the lookback window cutoff in Unix seconds.
    // query_log_lookback_days is in days; convert to seconds for comparison.
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let lookback_cutoff_secs = now_secs - (query_log_lookback_days as i64) * 86_400;

    // Comparison (ADR-003 step 3/4):
    // If oldest_retained_computed_at > lookback_cutoff_secs:
    //   The oldest retained cycle was reviewed WITHIN the lookback window.
    //   Data coverage is sufficient (or the window is conservative). No warning.
    //
    // If oldest_retained_computed_at <= lookback_cutoff_secs:
    //   The oldest retained cycle was reviewed BEFORE the lookback window started.
    //   PhaseFreqTable may query for data that has been pruned. Emit warn.
    if oldest <= lookback_cutoff_secs {
        tracing::warn!(
            query_log_lookback_days = query_log_lookback_days,
            activity_detail_retention_cycles = activity_detail_retention_cycles,
            oldest_retained_cycle_computed_at = oldest,
            lookback_cutoff_secs = lookback_cutoff_secs,
            "PhaseFreqTable lookback window ({} days) extends beyond retention window; \
             oldest retained cycle reviewed at {}, lookback cutoff is {}. \
             Consider reducing query_log_lookback_days or increasing \
             activity_detail_retention_cycles.",
            query_log_lookback_days,
            oldest,
            lookback_cutoff_secs,
        );
    }
    // If oldest > lookback_cutoff_secs: no action. Correct coverage.
}
```

---

## Integration in run_maintenance() Step 4

This check is called immediately after `list_purgeable_cycles()` returns successfully,
before the per-cycle loop begins:

```
let (purgeable_cycles, oldest_retained_computed_at) =
    self.store.list_purgeable_cycles(k, max_per_tick).await?;  // [error handling shown in run-maintenance-gc-block.md]

// Alignment guard: advisory only, does not block GC (ADR-003).
run_phase_freq_table_alignment_check(
    &oldest_retained_computed_at,
    inference_config.query_log_lookback_days,
    retention_config.activity_detail_retention_cycles,
);

// ... continue with purgeable cycle loop ...
```

The function can be a private method on `StatusService`, a free function in the same
module, or an inline block. A private function is preferred for testability.

---

## InferenceConfig.query_log_lookback_days Type Note

Verify the type of `inference_config.query_log_lookback_days` in the existing
`InferenceConfig` definition. The pseudocode uses `u32` but the actual type must
be confirmed from the struct definition. If it is `usize` or `i64`, cast accordingly
before the multiplication. The multiplication `(query_log_lookback_days as i64) * 86_400`
must not overflow for values in the validated range — max value ~3650 days * 86400 = 315,360,000
which fits comfortably in i64.

---

## Edge Cases

- `oldest_retained_computed_at = None` (fewer than K cycles): function returns immediately,
  no warning emitted. This is the correct behavior — no data has been pruned yet.
- `query_log_lookback_days = 0`: Would imply a lookback window of "now". In practice,
  `InferenceConfig::validate()` already enforces a minimum for this field. If somehow 0,
  the computation `now - 0 = now`, and `oldest <= now` would be true (almost always),
  emitting a spurious warning. This is harmless — the field is validated at startup.
- `oldest_retained_computed_at` is in the future (clock skew): `oldest > lookback_cutoff`
  → no warning. Correct and safe.
- `list_purgeable_cycles` fails: the alignment check is skipped (it uses the return value
  of `list_purgeable_cycles`). The `break 'gc_cycle_block` early exit in the parent
  block bypasses this check. Acceptable — a failing read query is a more urgent issue.

---

## Error Handling

This function has no error paths. It reads two values and either emits a warn or does
nothing. `SystemTime::now()` failure falls back to `0` via `unwrap_or_default()`, which
would produce an ancient lookback cutoff and potentially suppress the warning. This is
acceptable — a SystemTime failure on a running system is an extreme edge case.

---

## Key Test Scenarios

- **Warning fires**: K = 5, all 5 review cycles within past 7 days, `query_log_lookback_days = 365`.
  Assert `tracing::warn!` emitted containing `"query_log_lookback_days"` and `"retention window"`.
  (AC-17, R-11 scenario 1)

- **Warning suppressed**: K = 5, all 5 review cycles within past 7 days, `query_log_lookback_days = 3`.
  Assert no warn emitted for mismatch (sufficient coverage). (R-11 scenario 2)

- **Fewer than K cycles**: `cycle_review_index` has 3 rows with K = 5.
  `list_purgeable_cycles` returns `oldest_retained = None`.
  Assert no warn emitted (guard skipped). (R-11 scenario 3, R-16 scenario 2)

- **Boundary accuracy**: Insert exactly K cycles. Verify `oldest_retained_computed_at`
  in the warn log matches the actual K-th cycle's timestamp, not the K+1th or 1st.
  (R-16 scenario 1 — off-by-one check)

- **Correct comparison direction**: Both `oldest <= lookback_cutoff` (warn fires) and
  `oldest > lookback_cutoff` (no warn) must be tested to confirm the predicate is not
  inverted. (R-11 non-negotiable — both branches tested)
