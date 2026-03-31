# crt-036: Intelligence-Driven Retention Framework — Pseudocode Overview

## Components Involved

| Component | File | Action |
|-----------|------|--------|
| RetentionConfig | `crates/unimatrix-server/src/infra/config.rs` | Add struct + defaults + validate() + wire into UnimatrixConfig |
| CycleGcPass (store methods) | `crates/unimatrix-store/src/retention.rs` (new) + `lib.rs` | All GC store methods + stats types |
| run_maintenance GC block | `crates/unimatrix-server/src/services/status.rs` | Replace step 4 with cycle-based GC block; add retention_config param |
| Legacy DELETE removal | `crates/unimatrix-server/src/services/status.rs` + `src/mcp/tools.rs` | Remove both 60-day DELETE sites unconditionally |
| PhaseFreqTable alignment guard | `crates/unimatrix-server/src/services/status.rs` | Tick-time warn! when query_log_lookback_days exceeds K-cycle coverage |
| background.rs threading | `crates/unimatrix-server/src/background.rs` | Thread Arc<RetentionConfig> into run_single_tick + run_maintenance |
| config.toml | `config.toml` | Add [retention] block with documented fields |

## Data Flow (per maintenance tick)

```
background.rs::run_single_tick(retention_config: &Arc<RetentionConfig>)
    |
    v
status.rs::run_maintenance(..., retention_config: &RetentionConfig)
    |
    +--> [step 4: PhaseFreqTable alignment check]
    |        read: cycle_review_index K-th oldest computed_at
    |        compare against: inference_config.query_log_lookback_days
    |        emit: tracing::warn! if lookback exceeds coverage
    |
    +--> store.list_purgeable_cycles(k, max_per_tick)
    |        read: cycle_review_index via read_pool()
    |        returns: Vec<String> of feature_cycle IDs, oldest-first, capped
    |
    +--> [for each purgeable cycle]
    |        store.get_cycle_review(cycle_id)       -- crt-033 gate
    |            Ok(None) -> warn + skip
    |            Err(_)   -> warn + skip
    |            Ok(Some(record)) -> retain record in scope
    |
    |        store.gc_cycle_activity(cycle_id)      -- per-cycle transaction
    |            pool.begin() -> txn
    |            DELETE observations via session join
    |            DELETE query_log via session join
    |            DELETE injection_log via session join
    |            DELETE sessions WHERE feature_cycle = cycle_id
    |            txn.commit() -> connection released
    |            returns: CycleGcStats
    |
    |        store.store_cycle_review(                -- OUTSIDE transaction
    |            &CycleReviewRecord { raw_signals_available: 0, ..record }
    |        )
    |        tracing::info! with cycle_id + row counts
    |
    +--> store.gc_unattributed_activity()           -- after cycle loop
    |        DELETE observations WHERE session_id NOT IN sessions
    |        DELETE query_log WHERE session_id NOT IN sessions
    |        DELETE injection_log WHERE session IS unattributed + non-active
    |        DELETE sessions WHERE feature_cycle IS NULL AND status != Active
    |        returns: UnattributedGcStats
    |
    +--> [step 4f: audit_log GC]
         store.gc_audit_log(audit_log_retention_days)
             DELETE audit_log WHERE timestamp < now_secs - days * 86400
             returns: u64 rows deleted
```

## Shared Types Introduced

Defined in `crates/unimatrix-store/src/retention.rs`:

```
CycleGcStats {
    observations_deleted: u64,
    query_log_deleted:    u64,
    injection_log_deleted: u64,
    sessions_deleted:     u64,
}

UnattributedGcStats {
    observations_deleted:  u64,
    query_log_deleted:     u64,
    sessions_deleted:      u64,
    injection_log_deleted: u64,
}
```

Defined in `crates/unimatrix-server/src/infra/config.rs`:

```
RetentionConfig {
    activity_detail_retention_cycles: u32,   // default 50, range [1, 10000]
    audit_log_retention_days:         u32,   // default 180, range [1, 3650]
    max_cycles_per_tick:              u32,   // default 10, range [1, 1000]
}
```

New `ConfigError` variant (added to existing enum):

```
RetentionFieldOutOfRange {
    path:   PathBuf,
    field:  &'static str,
    value:  String,
    reason: &'static str,
}
```

## Sequencing Constraints

1. `retention.rs` (store crate) must be written before `run-maintenance-gc-block` because
   it defines the types consumed by status.rs.
2. `RetentionConfig` in `config.rs` must be written before `background.rs` threading
   and `run_maintenance` signature change.
3. Legacy DELETE sites (`status.rs` lines 1372–1384 and `tools.rs` lines 1630–1642) are
   removed in the same pass as the GC block insertion. They are not independent removals —
   the new step 4 block replaces the old one atomically.
4. `lib.rs` (`pub mod retention`) is a one-line addition with no dependencies.

## Implementation Wave Recommendation

Wave 1: retention.rs (store) + lib.rs + RetentionConfig (server config)
Wave 2: run_maintenance GC block + legacy DELETE removal + background.rs threading
Wave 3: config.toml [retention] block

## Key Constraints Summary

- write_pool_server() max_connections = 1: transaction must be acquired, used, and
  released within each gc_cycle_activity() call — never across multiple cycles.
- pool.begin() / txn.commit() API required (entry #2159: raw BEGIN SQL is unsafe in sqlx).
- Delete order inside per-cycle transaction is fixed:
  observations -> query_log -> injection_log -> sessions
- raw_signals_available update uses store_cycle_review() with struct update syntax;
  the record from get_cycle_review() is retained in scope across gc_cycle_activity().
- Both 60-day DELETE sites removed unconditionally — no flags, no guards.
- status = Active sessions (numeric 0) excluded from unattributed cleanup.
- No schema migration — all SQL operates on existing indexed columns.
