# Component: status-diagnostics
# Files: `crates/unimatrix-server/src/services/status.rs`
#        `crates/unimatrix-server/src/background.rs`

---

## Purpose

Two changes across two files:

1. **`status.rs`**: Update the crt-036 diagnostic function `run_phase_freq_table_alignment_check`
   to reference `phase_freq_lookback_days` (field rename) and update warning message text.
   Add a new `run_observations_coverage_check` function (AC-11 diagnostic warning).

2. **`background.rs`**: Update the single field access from
   `inference_config.query_log_lookback_days` → `inference_config.phase_freq_lookback_days`
   at the confirmed site (line 622). Pass `min_phase_session_pairs` to `rebuild()` if the
   implementer chose Option A signature extension (see `phase-freq-table.md`).

---

## File: status.rs

### Change 1: run_phase_freq_table_alignment_check — field rename

#### Current signature (~line 1644)

```rust
fn run_phase_freq_table_alignment_check(
    oldest_retained_computed_at: &Option<i64>,
    query_log_lookback_days: u32,
    activity_detail_retention_cycles: u32,
)
```

#### After

```rust
/// Emit `tracing::warn!` when `phase_freq_lookback_days` implies a data window
/// older than the oldest retained cycle's `computed_at`.
///
/// This check applies to the observations source (crt-050): observations linked
/// to sessions that belong to pruned cycles are still counted in freq (by Query A)
/// but contribute no outcome weight (Query B sees no live cycle_events rows for
/// pruned cycles). The diagnostic remains an advisory signal for the operator.
///
/// Advisory only — does not block GC or alter config. Called at the start of
/// the cycle-based GC block each tick (crt-036 ADR-003, updated crt-050).
///
/// Skipped when `oldest_retained_computed_at` is None (fewer than K cycles reviewed).
fn run_phase_freq_table_alignment_check(
    oldest_retained_computed_at: &Option<i64>,
    phase_freq_lookback_days: u32,
    activity_detail_retention_cycles: u32,
)
```

#### Body changes

```
FUNCTION run_phase_freq_table_alignment_check(
    oldest_retained_computed_at: &Option<i64>,
    phase_freq_lookback_days: u32,                  // RENAMED parameter
    activity_detail_retention_cycles: u32,
):

  oldest: i64 = match oldest_retained_computed_at:
    None → RETURN  // fewer than K cycles; no pruning occurred; skip
    Some(ts) → *ts

  now_secs: i64 = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64

  lookback_cutoff_secs: i64 = now_secs - (phase_freq_lookback_days as i64) * 86_400

  IF oldest <= lookback_cutoff_secs:
    tracing::warn!(
      phase_freq_lookback_days = phase_freq_lookback_days,     // RENAMED field
      activity_detail_retention_cycles = activity_detail_retention_cycles,
      oldest_retained_cycle_computed_at = oldest,
      lookback_cutoff_secs = lookback_cutoff_secs,
      "PhaseFreqTable lookback window ({} days) extends beyond retention window; \
       oldest retained cycle reviewed at {}, lookback cutoff is {}. \
       Consider reducing phase_freq_lookback_days or increasing \
       activity_detail_retention_cycles.",                      // RENAMED in message
      phase_freq_lookback_days,
      oldest,
      lookback_cutoff_secs,
    )
  // If oldest > lookback_cutoff_secs: no action.

END FUNCTION
```

#### Call site update (~line 1405 region)

```
// Before:
run_phase_freq_table_alignment_check(
    &oldest_retained_computed_at,
    inference_config.query_log_lookback_days,    // OLD field name
    retention_config.activity_detail_retention_cycles,
);

// After:
run_phase_freq_table_alignment_check(
    &oldest_retained_computed_at,
    inference_config.phase_freq_lookback_days,   // NEW field name
    retention_config.activity_detail_retention_cycles,
);
```

---

### Change 2: run_observations_coverage_check — NEW function

```rust
/// Emit advisory `tracing::warn!` when the distinct (phase, session_id) pair
/// count in observations falls below the configured minimum threshold.
///
/// This is a DIAGNOSTIC WARNING ONLY — it does not gate any operation or modify
/// any state. The actual coverage gate that sets use_fallback=true lives in
/// PhaseFreqTable::rebuild() (FR-17). This function is the tick-time advisory
/// diagnostic described in AC-11 and the ARCHITECTURE.md coverage section.
///
/// Called during run_maintenance step 4 (alongside the alignment check).
/// Uses the same SQL window as Query A in rebuild() to ensure consistency.
fn run_observations_coverage_check(
    coverage_count: u64,
    min_phase_session_pairs: u32,
    phase_freq_lookback_days: u32,
)
```

#### Body

```
FUNCTION run_observations_coverage_check(
    coverage_count: u64,
    min_phase_session_pairs: u32,
    phase_freq_lookback_days: u32,
):

  IF coverage_count < min_phase_session_pairs as u64:
    tracing::warn!(
      observations_phase_session_pairs = coverage_count,
      min_phase_session_pairs = min_phase_session_pairs,
      phase_freq_lookback_days = phase_freq_lookback_days,
      "PhaseFreqTable observations coverage: only {} distinct (phase, session_id) \
       pairs found in the {} day lookback window; minimum threshold is {}. \
       Phase affinity scoring may be sparse.",
      coverage_count,
      phase_freq_lookback_days,
      min_phase_session_pairs,
    )
  // If coverage_count >= threshold: no action.

END FUNCTION
```

#### Where the coverage_count comes from

`run_observations_coverage_check` requires `coverage_count` as an input. This count
is NOT re-queried in `status.rs` — it is computed in `PhaseFreqTable::rebuild()` via
`store.count_phase_session_pairs(lookback_days)` (see `phase-freq-table.md`).

There are two wiring options:

**Option A (preferred for separation of concerns):** `PhaseFreqTable` exposes the last
coverage count as a field: `pub last_coverage_count: u64`. `rebuild()` writes it.
`run_maintenance` reads it from the handle's read lock and passes to the diagnostic.

**Option B:** `run_maintenance` calls a new store fn `count_phase_session_pairs` directly.
This duplicates the query (once in rebuild, once in status).

Option A is preferred. The `last_coverage_count` field on `PhaseFreqTable` is set to 0
on cold-start and updated each tick. It does not affect `use_fallback` state.

If Option A is chosen, add to `PhaseFreqTable` struct:

```rust
// crt-050: last distinct (phase, session_id) pair count from most recent rebuild.
// Advisory-only — used by status diagnostics. 0 on cold-start.
pub last_coverage_count: u64,
```

And in `PhaseFreqTable::rebuild()`, set:
```
new_table.last_coverage_count = coverage_count
```

#### Call site in run_maintenance (~line 1380 region, after the alignment check)

```
// After the existing alignment check call, add:

{
  let pft_guard = phase_freq_table_handle.read().unwrap_or_else(|e| e.into_inner());
  run_observations_coverage_check(
      pft_guard.last_coverage_count,
      inference_config.min_phase_session_pairs,
      inference_config.phase_freq_lookback_days,
  );
}
```

The `phase_freq_table_handle` must be available in `run_maintenance` scope. Verify
the existing signature includes it; if not, it must be threaded through.

Alternatively, if `run_maintenance` does not currently receive the `PhaseFreqTableHandle`,
Option B (store query in status.rs) may be simpler. The implementer must choose based
on the actual `run_maintenance` signature.

---

### Existing test module updates (crt_036_phase_freq_table_guard_tests)

The test module at ~line 2964 tests `run_phase_freq_table_alignment_check` with the
old parameter name `query_log_lookback_days`. After the rename, update:

1. All calls to `run_phase_freq_table_alignment_check` in the test module to pass the
   parameter by position (unchanged — it's positional).
2. All `logs_contain("query_log_lookback_days")` assertions to `logs_contain("phase_freq_lookback_days")`.

```
// Before (line ~2993):
assert!(
    logs_contain("query_log_lookback_days"),
    "WARN must mention query_log_lookback_days (AC-17)"
);

// After:
assert!(
    logs_contain("phase_freq_lookback_days"),
    "WARN must mention phase_freq_lookback_days (AC-17)"
);
```

---

## File: background.rs

### Single field access update (confirmed line 622)

```
// Before:
let lookback_days = inference_config.query_log_lookback_days;

// After:
let lookback_days = inference_config.phase_freq_lookback_days;
```

### Pass min_phase_session_pairs to rebuild()

If the `rebuild()` signature was extended with `min_phase_session_pairs` (Option A from
`phase-freq-table.md`), the call site in background.rs must pass it:

```
// Before (inferred from existing code):
match PhaseFreqTable::rebuild(&store_ref, lookback_days).await { ... }

// After (if Option A signature):
let min_pairs = inference_config.min_phase_session_pairs;
match PhaseFreqTable::rebuild(&store_ref, lookback_days, min_pairs).await { ... }
```

No other changes to `background.rs` are required.

---

## Error Handling

| Failure | Behavior |
|---------|----------|
| `run_phase_freq_table_alignment_check` — parameter is None | Early return, no warning emitted |
| `run_observations_coverage_check` — coverage above threshold | No action |
| `run_observations_coverage_check` — coverage below threshold | `tracing::warn!` with count, threshold, lookback; no state change |
| RwLock on PhaseFreqTableHandle (for last_coverage_count) | `.unwrap_or_else(|e| e.into_inner())` — poison recovery, consistent with all other lock acquisitions |

---

## Key Test Scenarios

**T-SD-01: Alignment check fires when lookback exceeds retention (AC-11, crt-036)**
- Same as existing `test_gc_phase_freq_table_mismatch_warning_fires`.
- After rename: assert `logs_contain("phase_freq_lookback_days")`, not the old name.

**T-SD-02: Alignment check suppressed when coverage sufficient**
- Same as existing `test_gc_phase_freq_table_no_warning_when_sufficient_coverage`.
- No log assertion change needed (test checks absence of "retention window" log).

**T-SD-03: Alignment check skipped when None**
- Same as existing `test_gc_phase_freq_table_skipped_when_fewer_than_k_cycles`.
- No change needed.

**T-SD-04: Coverage check — warning fires below threshold**
- Call `run_observations_coverage_check(coverage_count=3, min_phase_session_pairs=5, phase_freq_lookback_days=30)`.
- Assert: `tracing::warn!` emitted containing "3", "5", "30".

**T-SD-05: Coverage check — no warning above threshold**
- Call `run_observations_coverage_check(coverage_count=10, min_phase_session_pairs=5, phase_freq_lookback_days=30)`.
- Assert: no warning log emitted.

**T-SD-06: Coverage check — boundary (count == threshold)**
- Call `run_observations_coverage_check(coverage_count=5, min_phase_session_pairs=5, ...)`.
- Assert: no warning (count >= threshold, strict less-than gate).

**T-SD-07: background.rs field reference compiles**
- After renaming `query_log_lookback_days` → `phase_freq_lookback_days` in config.rs,
  verify `cargo build --workspace` succeeds. The background.rs line 622 update is
  required for the build to pass — this is a compile-time verification, not a
  separate unit test.

**T-SD-08: Warning message references phase_freq_lookback_days (renamed field)**
- After `run_phase_freq_table_alignment_check` fires, assert the log contains
  "phase_freq_lookback_days", not "query_log_lookback_days".
  (Updates existing AC-17 test assertion.)

---

## Open Question: run_maintenance signature for PhaseFreqTableHandle

The current `run_maintenance` signature in `status.rs` must be checked to determine
if `PhaseFreqTableHandle` is already a parameter. From the context grep, the signature
includes `inference_config: &InferenceConfig` and `retention_config: &RetentionConfig`,
but does not visibly include the handle.

If `PhaseFreqTableHandle` is NOT currently a parameter of `run_maintenance`, the
implementer must either:
- Add it as a parameter and update all call sites.
- Use Option B (store query in status.rs) instead.
- Defer the `last_coverage_count` field approach and have `rebuild()` emit the warning
  directly (no separate status diagnostic fn).

The rebuild() already emits a `tracing::warn!` at the coverage gate (step 3 of the
modified `rebuild()`). The `run_observations_coverage_check` in status.rs is described
in the architecture as a SEPARATE diagnostic at tick-time advisory level. If threading
the handle is too invasive, emitting only from `rebuild()` is architecturally defensible
and covers AC-11's intent. Flag this decision to the SM if discovered during implementation.
