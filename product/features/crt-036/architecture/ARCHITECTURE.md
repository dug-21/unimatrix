# crt-036: Intelligence-Driven Retention Framework — Architecture

## System Overview

Unimatrix's activity tables (`observations`, `query_log`, `sessions`, `injection_log`)
currently grow unbounded or are pruned by wall-clock age. The 60-day hard DELETE on
`observations` (two sites: `status.rs` line 1380 and `tools.rs` line 1638) is the
only retention mechanism, and it has no awareness of whether the retrospective pipeline
has consumed the data. This feature replaces that age-based policy with a cycle-based
GC that retains the last K reviewed cycles of activity data and prunes everything older.

`audit_log` is handled separately: it is an accountability record, not a learning
signal. A 180-day time-based DELETE is appropriate and is added as a new independent
step.

The GC runs inside the existing `run_maintenance()` background tick in
`unimatrix-server/src/services/status.rs`. No new background thread is introduced.
Schema stays at v19 — no migration is needed.

## Component Breakdown

### 1. RetentionConfig (unimatrix-server / infra/config.rs)

A new top-level config section `[retention]` that is added to `UnimatrixConfig`
alongside the existing `InferenceConfig`, `ObservationConfig`, etc. Follows the
`#[serde(default)]` pattern used by every other section.

Fields:
- `activity_detail_retention_cycles: u32` — number of reviewed feature cycles whose
  activity data is retained. Default: 50. Range: [1, 10000].
- `audit_log_retention_days: u32` — retention window for `audit_log` rows. Default:
  180. Range: [1, 3650].

`RetentionConfig::validate()` is a new method following the same pattern as
`InferenceConfig::validate()`: takes `path: &Path`, returns `Result<(), ConfigError>`,
aborts startup on out-of-range values with a structured error naming the field.

`activity_detail_retention_cycles` is the governing ceiling for the `PhaseFreqTable`
frequency table lookback window (col-031 ADR-002 / entry #3686) and any future GNN
training window. Code comment and docstring must state this explicitly (AC-13).

### 2. CycleGcPass (unimatrix-store / sessions.rs or a new retention.rs)

A new set of store methods on `SqlxStore` that implement the per-cycle GC logic. The
natural home is `unimatrix-store` to keep all SQL alongside the table definitions.

New methods:
- `list_purgeable_cycles(k: u32) -> Result<Vec<String>>`
  Queries `cycle_review_index ORDER BY computed_at DESC LIMIT k` for the retain set,
  then returns cycles with a review row that are NOT in the retain set.
- `gc_cycle_activity(feature_cycle: &str) -> Result<CycleGcStats>`
  Deletes all activity for one cycle in a single `pool.begin()` transaction:
  1. DELETE observations WHERE session_id IN (SELECT session_id FROM sessions WHERE feature_cycle = ?)
  2. DELETE query_log WHERE session_id IN (SELECT session_id FROM sessions WHERE feature_cycle = ?)
  3. DELETE injection_log WHERE session_id IN (SELECT session_id FROM sessions WHERE feature_cycle = ?)
  4. DELETE sessions WHERE feature_cycle = ?
  Commits, then returns row counts per table.
No `mark_signals_purged()` method is added. Instead, the gate-check record fetched in
step 3a is reused directly:
```
store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record }).await?
```
where `record` is the `CycleReviewRecord` returned by `get_cycle_review()`. The struct
update syntax preserves `summary_json` and all other fields; `store_cycle_review()`
uses INSERT OR REPLACE, which is safe here because the full record is supplied.

**Critical dependency**: the `CycleReviewRecord` retrieved in step 3a (gate check)
must NOT be discarded after the gate passes. It must be retained in scope and passed to
the `store_cycle_review()` call in step 3c. Discarding the record and reconstructing it
from partial data (e.g. only setting `raw_signals_available`) would clobber
`summary_json` — the original SR-05 risk.
- `gc_unattributed_activity() -> Result<UnattributedGcStats>`
  Deletes rows whose session is absent from `sessions`:
  1. DELETE observations WHERE session_id NOT IN (SELECT session_id FROM sessions)
  2. DELETE query_log WHERE session_id NOT IN (SELECT session_id FROM sessions)
  Runs as a single statement (no transaction needed — each DELETE is atomic).
  Also prunes unattributed sessions (feature_cycle IS NULL, status != Active):
  `DELETE FROM injection_log WHERE session_id IN (SELECT session_id FROM sessions WHERE feature_cycle IS NULL AND status != 0)`
  `DELETE FROM sessions WHERE feature_cycle IS NULL AND status != 0`
- `gc_audit_log(retention_days: u32) -> Result<u64>`
  `DELETE FROM audit_log WHERE timestamp < (strftime('%s','now') - ?1 * 86400)`

### 3. run_maintenance() GC Block (unimatrix-server / services/status.rs)

Step 4 is rewritten. The existing 60-day DELETE is removed entirely and replaced with
a call to the CycleGcPass methods. Step 4 is renamed from "Observation retention
cleanup" to "Cycle-based activity GC". A new step 4f handles `audit_log` (using "4f"
to avoid collision with sub-steps 4a–4e inside the cycle loop).

Step 6 (`gc_sessions`, existing 30-day time-based cascade) is **unchanged**. It handles
sessions with no `feature_cycle` attribution and timed-out sessions. The cycle-based GC
at step 4 and the time-based GC at step 6 target disjoint session populations; both
are necessary.

The GC block receives `retention_config: &RetentionConfig` as a parameter to
`run_maintenance()` (same threading pattern as `inference_config: &Arc<InferenceConfig>`
in background.rs).

Per-cycle loop:
1. Call `list_purgeable_cycles(k)` — small bounded read.
2. Apply `max_cycles_per_tick` cap: take at most `retention_config.max_cycles_per_tick`
   from the purgeable list this tick.
3. For each purgeable cycle:
   a. Fetch `let record = get_cycle_review(feature_cycle)` — crt-033 gate. Skip and log
      if `Ok(None)` (defense-in-depth). **Retain `record` in scope.**
   b. Call `gc_cycle_activity(feature_cycle)`.
   c. Call `store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record })`
      — uses the record fetched in step 3a. Must not reconstruct from scratch.
   d. Log `tracing::info!` with cycle ID and per-table row counts.
4. Call `gc_unattributed_activity()` — runs after the cycle loop regardless of cap.
5. (Step 4f) Call `gc_audit_log(retention_config.audit_log_retention_days)`.

### 4. Removal of Legacy DELETE Sites

Both of the following are removed entirely (not conditionally):
- `status.rs` lines 1372–1384 — the 60-day DELETE inside the step 4 block
- `tools.rs` lines 1630–1642 — the FR-07 in-tool 60-day DELETE

Neither is left in place, guarded, or conditionalized.

### 5. PhaseFreqTable / K-cycle Alignment Guard (services/status.rs or phase_freq_table.rs)

A tick-time diagnostic emits `tracing::warn!` when `inference_config.query_log_lookback_days`
exceeds the data coverage implied by the K-cycle retention window. Because the exact
data age per cycle is not tracked in a column, the guard uses an approximation:
compare the wall-clock age of the oldest retained cycle's `computed_at` timestamp
against the `query_log_lookback_days` window.

```
if oldest_retained_computed_at < now - query_log_lookback_days * 86400 {
    tracing::warn!(
        query_log_lookback_days = inference_config.query_log_lookback_days,
        activity_detail_retention_cycles = retention_config.activity_detail_retention_cycles,
        "PhaseFreqTable lookback window ({} days) extends beyond retained \
         cycle data (oldest retained cycle computed at {}); frequency table \
         may operate on a truncated window",
        inference_config.query_log_lookback_days,
        oldest_retained_computed_at,
    );
}
```

This check runs at the start of step 4, after resolving the purgeable set (the retain
set's oldest entry is a by-product of that query).

## Component Interactions

```
background.rs (run_single_tick)
    │
    └─► run_maintenance(&retention_config)    [step 4 + 4f; step 6 gc_sessions unchanged]
            │
            ├─► SqlxStore::list_purgeable_cycles(k)
            │       └─ reads: cycle_review_index (read_pool)
            │
            ├─► [for each purgeable cycle, capped at max_cycles_per_tick]
            │       ├─► SqlxStore::get_cycle_review(cycle_id)   [crt-033 gate]
            │       ├─► SqlxStore::gc_cycle_activity(cycle_id)  [write_pool, transaction]
            │       │       DELETE observations WHERE session_id IN (...)
            │       │       DELETE query_log WHERE session_id IN (...)
            │       │       DELETE injection_log WHERE session_id IN (...)
            │       │       DELETE sessions WHERE feature_cycle = ?
            │       └─► store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record })
            │
            ├─► SqlxStore::gc_unattributed_activity()           [write_pool, no transaction]
            │
            └─► SqlxStore::gc_audit_log(days)                   [write_pool, no transaction]
```

## Technology Decisions

| Decision | Choice | ADR |
|----------|--------|-----|
| Transaction granularity | Per-cycle `pool.begin()` transactions, not a single spanning transaction | ADR-001 |
| Batch cap placement | `max_cycles_per_tick` in `RetentionConfig`, not `InferenceConfig` | ADR-002 |
| PhaseFreqTable alignment | Tick-time `tracing::warn!` guard comparing `computed_at` timestamps | ADR-003 |
| raw_signals_available update | `store_cycle_review()` with struct update `{ raw_signals_available: 0, ..record }` — record from gate check retained in scope | SCOPE-RISK SR-05 |
| Unattributed session guard | Skip if `status = Active` to protect in-flight sessions | SCOPE-RISK SR-06 |

See individual ADR files for full rationale.

## Integration Points

### Existing components consumed

- `SqlxStore::gc_sessions()` — reference pattern for per-cycle transaction structure
  (injection_log first, sessions second). The new `gc_cycle_activity()` follows this
  cascade order within its transaction.
- `SqlxStore::get_cycle_review()` — crt-033 gate check per cycle.
- `SqlxStore::write_pool_server()` — all GC writes use this directly (not analytics drain).
  Pattern established by crt-033 ADR-001 (entry #3793) and the `gc_sessions` precedent.
- `InferenceConfig::validate()` — template for `RetentionConfig::validate()`.
- `background.rs` `run_single_tick()` — threads `inference_config` as
  `Arc<InferenceConfig>` into `run_maintenance()`; `retention_config` follows the
  identical threading pattern as `Arc<RetentionConfig>`.

### Existing components removed

- The 60-day DELETE block at `status.rs` lines 1372–1384.
- The FR-07 60-day DELETE block at `tools.rs` lines 1630–1642.

### New store methods added

All new methods are on `SqlxStore` in `unimatrix-store`. They use `write_pool_server()`
for writes and `read_pool()` for `list_purgeable_cycles`. GC stats structs (`CycleGcStats`,
`UnattributedGcStats`) are new types in the same module.

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|-----------------|--------|
| `RetentionConfig` | `pub struct RetentionConfig { activity_detail_retention_cycles: u32, audit_log_retention_days: u32, max_cycles_per_tick: u32 }` | `infra/config.rs` (new) |
| `RetentionConfig::validate` | `fn validate(&self, path: &Path) -> Result<(), ConfigError>` | `infra/config.rs` (new) |
| `UnimatrixConfig::retention` | `pub retention: RetentionConfig` with `#[serde(default)]` | `infra/config.rs` (new field) |
| `SqlxStore::list_purgeable_cycles` | `async fn list_purgeable_cycles(&self, k: u32) -> Result<Vec<String>>` | `unimatrix-store/retention.rs` (new) |
| `SqlxStore::gc_cycle_activity` | `async fn gc_cycle_activity(&self, feature_cycle: &str) -> Result<CycleGcStats>` | `unimatrix-store/retention.rs` (new) |
| `store_cycle_review()` (reused) | Called with `&CycleReviewRecord { raw_signals_available: 0, ..record }` — no new method | `unimatrix-store/cycle_review_index.rs` (existing) |
| `SqlxStore::gc_unattributed_activity` | `async fn gc_unattributed_activity(&self) -> Result<UnattributedGcStats>` | `unimatrix-store/retention.rs` (new) |
| `SqlxStore::gc_audit_log` | `async fn gc_audit_log(&self, retention_days: u32) -> Result<u64>` | `unimatrix-store/retention.rs` (new) |
| `CycleGcStats` | `pub struct CycleGcStats { observations_deleted: u64, query_log_deleted: u64, injection_log_deleted: u64, sessions_deleted: u64 }` | `unimatrix-store/retention.rs` (new) |
| `UnattributedGcStats` | `pub struct UnattributedGcStats { observations_deleted: u64, query_log_deleted: u64, sessions_deleted: u64, injection_log_deleted: u64 }` | `unimatrix-store/retention.rs` (new) |
| `run_maintenance()` | Adds `retention_config: &RetentionConfig` parameter | `services/status.rs` (changed) |
| `run_single_tick()` | Adds `retention_config: &Arc<RetentionConfig>` parameter | `background.rs` (changed) |

## Data Flow

```
Each maintenance tick:

1. list_purgeable_cycles(K)
   → cycle_review_index (read): SELECT feature_cycle NOT IN (top-K by computed_at)

2. [PhaseFreqTable alignment check]
   → If oldest retained computed_at < now - query_log_lookback_days * 86400: WARN

3. For each purgeable cycle (up to max_cycles_per_tick):
   a. get_cycle_review(cycle_id) → verify review exists (crt-033 gate)
   b. gc_cycle_activity(cycle_id):
      BEGIN TRANSACTION
        DELETE FROM observations WHERE session_id IN
          (SELECT session_id FROM sessions WHERE feature_cycle = cycle_id)
        DELETE FROM query_log WHERE session_id IN
          (SELECT session_id FROM sessions WHERE feature_cycle = cycle_id)
        DELETE FROM injection_log WHERE session_id IN
          (SELECT session_id FROM sessions WHERE feature_cycle = cycle_id)
        DELETE FROM sessions WHERE feature_cycle = cycle_id
      COMMIT
   c. store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record })
      -- record is the CycleReviewRecord returned in step 3a; struct update preserves
      -- summary_json and all other fields (SR-05 mitigation)

4. gc_unattributed_activity():
   DELETE FROM observations WHERE session_id NOT IN (SELECT session_id FROM sessions)
   DELETE FROM query_log WHERE session_id NOT IN (SELECT session_id FROM sessions)
   DELETE FROM injection_log WHERE session_id IN
     (SELECT session_id FROM sessions WHERE feature_cycle IS NULL AND status != 0)
   DELETE FROM sessions WHERE feature_cycle IS NULL AND status != 0

5. gc_audit_log(audit_log_retention_days):
   DELETE FROM audit_log WHERE timestamp < (strftime('%s','now') - days * 86400)
```

## File Placement

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/infra/config.rs` | Add `RetentionConfig`, `default_*` fns, `validate()`, wire into `UnimatrixConfig` |
| `crates/unimatrix-store/src/retention.rs` | New file: all GC store methods + stats types |
| `crates/unimatrix-store/src/lib.rs` | Add `pub mod retention` |
| `crates/unimatrix-server/src/services/status.rs` | Replace step 4 block; add `retention_config` param; add PhaseFreqTable guard |
| `crates/unimatrix-server/src/background.rs` | Thread `Arc<RetentionConfig>` into tick loop and `run_maintenance` call |
| `crates/unimatrix-server/src/mcp/tools.rs` | Remove FR-07 60-day DELETE block (lines 1630–1642) |

## Open Questions

None. All implementation details are resolved. SR-08 (cycles never reviewed → data
retained forever) is a documented operational constraint, not a code gap: operators
must call `context_cycle_review` to advance the K window.
