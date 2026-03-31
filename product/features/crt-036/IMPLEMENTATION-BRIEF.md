# crt-036: Intelligence-Driven Retention Framework — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-036/SCOPE.md |
| Architecture | product/features/crt-036/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-036/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-036/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-036/ALIGNMENT-REPORT.md |
| ADR-001 | product/features/crt-036/architecture/ADR-001-per-cycle-transaction-granularity.md |
| ADR-002 | product/features/crt-036/architecture/ADR-002-max-cycles-per-tick-in-retention-config.md |
| ADR-003 | product/features/crt-036/architecture/ADR-003-phase-freq-table-k-cycle-alignment.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| RetentionConfig | product/features/crt-036/pseudocode/retention-config.md | product/features/crt-036/test-plan/retention-config.md |
| CycleGcPass (store methods) | product/features/crt-036/pseudocode/cycle-gc-pass.md | product/features/crt-036/test-plan/cycle-gc-pass.md |
| run_maintenance GC block | product/features/crt-036/pseudocode/run-maintenance-gc-block.md | product/features/crt-036/test-plan/run-maintenance-gc-block.md |
| Legacy DELETE removal | product/features/crt-036/pseudocode/legacy-delete-removal.md | product/features/crt-036/test-plan/legacy-delete-removal.md |
| PhaseFreqTable alignment guard | product/features/crt-036/pseudocode/phase-freq-table-guard.md | product/features/crt-036/test-plan/phase-freq-table-guard.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | product/features/crt-036/pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | product/features/crt-036/test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Replace Unimatrix's 60-day wall-clock observation DELETE with a cycle-aligned GC policy that retains activity data (`observations`, `query_log`, `sessions`, `injection_log`) for the most recently reviewed K feature cycles and prunes all older reviewed cycles, gated on `cycle_review_index` existence. A new `[retention]` config block exposes `activity_detail_retention_cycles` (default 50), `audit_log_retention_days` (default 180), and `max_cycles_per_tick` (default 10), making retention policy configurable without code changes. The two existing 60-day DELETE sites in `status.rs` and `tools.rs` are removed entirely.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Transaction granularity for GC pass | Per-cycle `pool.begin()` / `tx.commit()` transactions; connection released between cycles; not a single spanning transaction | SR-01, SR-02 | architecture/ADR-001-per-cycle-transaction-granularity.md |
| `max_cycles_per_tick` config placement | Belongs in `RetentionConfig`, not `InferenceConfig`; GC throughput is a retention concern, not an ML inference concern | SR-01 scope | architecture/ADR-002-max-cycles-per-tick-in-retention-config.md |
| PhaseFreqTable / K-cycle alignment | Tick-time `tracing::warn!` comparing oldest retained cycle `computed_at` against `query_log_lookback_days` window; no breaking config change | SR-07 | architecture/ADR-003-phase-freq-table-k-cycle-alignment.md |
| `raw_signals_available` update path | Call `store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record })` using the record already fetched in the gate check. Struct update preserves `summary_json` and all other fields. No new `mark_signals_purged()` method. | SR-05 | architecture/ADR-001-per-cycle-transaction-granularity.md (consequences) |
| Gate-check record retention | The `CycleReviewRecord` returned by `get_cycle_review()` must be retained in scope and passed to the `store_cycle_review()` call after commit. Must NOT be discarded and reconstructed — reconstruction clobbers `summary_json`. | VARIANCE-01 (resolved) | architecture/ADR-001-per-cycle-transaction-granularity.md |
| Unattributed session guard | `gc_unattributed_activity()` skips sessions with `status = Active` to protect in-flight retrospectives | SR-06 | ARCHITECTURE.md component 2 |

---

## Files to Create/Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-store/src/retention.rs` | Create | All five GC store methods + `CycleGcStats` / `UnattributedGcStats` types |
| `crates/unimatrix-store/src/lib.rs` | Modify | Add `pub mod retention` |
| `crates/unimatrix-server/src/infra/config.rs` | Modify | Add `RetentionConfig` struct, default fns, `validate()`, wire into `UnimatrixConfig` |
| `crates/unimatrix-server/src/services/status.rs` | Modify | Replace step 4 (60-day DELETE) with cycle-based GC block; add `retention_config` param; add PhaseFreqTable alignment guard |
| `crates/unimatrix-server/src/background.rs` | Modify | Thread `Arc<RetentionConfig>` into tick loop and `run_maintenance` call |
| `crates/unimatrix-server/src/mcp/tools.rs` | Modify | Remove 60-day DELETE block at lines ~1630–1642 (FR-07 in-tool path) |
| `config.toml` | Modify | Add `[retention]` block with documented fields |

---

## Data Structures

```rust
// crates/unimatrix-store/src/retention.rs

pub struct CycleGcStats {
    pub observations_deleted: u64,
    pub query_log_deleted: u64,
    pub injection_log_deleted: u64,
    pub sessions_deleted: u64,
}

pub struct UnattributedGcStats {
    pub observations_deleted: u64,
    pub query_log_deleted: u64,
    pub sessions_deleted: u64,
    pub injection_log_deleted: u64,
}
```

```rust
// crates/unimatrix-server/src/infra/config.rs

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(default)]
pub struct RetentionConfig {
    /// Number of completed (reviewed) feature cycles to retain activity data for.
    /// This value is the governing ceiling for PhaseFreqTable lookback and the future
    /// GNN training window. Reducing this value will truncate the data available to
    /// PhaseFreqTable::rebuild. Range: [1, 10000]. Default: 50.
    pub activity_detail_retention_cycles: u32,
    /// Retention window in days for audit_log rows. Range: [1, 3650]. Default: 180.
    pub audit_log_retention_days: u32,
    /// Maximum purgeable cycles to process per maintenance tick. Range: [1, 1000]. Default: 10.
    pub max_cycles_per_tick: u32,
}
```

---

## Function Signatures

```rust
// crates/unimatrix-store/src/retention.rs (methods on SqlxStore)

/// Returns feature_cycle IDs for all reviewed cycles outside the K-window,
/// ordered oldest-first (lowest computed_at). Result is capped to max_per_tick.
async fn list_purgeable_cycles(&self, k: u32, max_per_tick: u32) -> Result<Vec<String>>;

/// Executes per-cycle DELETE transaction: observations → query_log → injection_log → sessions.
/// Uses pool.begin() / tx.commit(). Connection released on return.
async fn gc_cycle_activity(&self, feature_cycle: &str) -> Result<CycleGcStats>;

/// NOT a new method — use store_cycle_review() with struct update syntax instead:
///   store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record })
/// where `record` is the CycleReviewRecord returned by get_cycle_review() in the gate check.
/// Runs after gc_cycle_activity() commits — outside the per-cycle transaction.
/// The `record` from the gate check MUST be retained in scope for this call.

/// Deletes observation/query_log rows with no matching session, and unattributed
/// (feature_cycle IS NULL) sessions/injection_log rows where status != Active.
async fn gc_unattributed_activity(&self) -> Result<UnattributedGcStats>;

/// Deletes audit_log rows older than retention_days (Unix seconds comparison).
async fn gc_audit_log(&self, retention_days: u32) -> Result<u64>;
```

```rust
// crates/unimatrix-server/src/infra/config.rs

impl RetentionConfig {
    pub fn validate(&self, path: &Path) -> Result<(), ConfigError>;
}
```

```rust
// crates/unimatrix-server/src/services/status.rs

// run_maintenance() signature change:
pub async fn run_maintenance(
    &self,
    inference_config: &Arc<InferenceConfig>,
    retention_config: &RetentionConfig,   // NEW
    // ... existing params
) -> Result<()>;
```

---

## Key SQL

```sql
-- Purgeable cycle resolution (list_purgeable_cycles)
SELECT feature_cycle FROM cycle_review_index
WHERE feature_cycle NOT IN (
    SELECT feature_cycle FROM cycle_review_index
    ORDER BY computed_at DESC
    LIMIT :k
)
ORDER BY computed_at ASC
LIMIT :max_per_tick

-- Per-cycle DELETEs (inside pool.begin() transaction, in this order)
DELETE FROM observations
  WHERE session_id IN (SELECT session_id FROM sessions WHERE feature_cycle = ?);
DELETE FROM query_log
  WHERE session_id IN (SELECT session_id FROM sessions WHERE feature_cycle = ?);
DELETE FROM injection_log
  WHERE session_id IN (SELECT session_id FROM sessions WHERE feature_cycle = ?);
DELETE FROM sessions WHERE feature_cycle = ?;

-- raw_signals_available flag update (AFTER transaction commit, separate write)
-- Use store_cycle_review() with struct update: { raw_signals_available: 0, ..record }
-- where `record` is the CycleReviewRecord fetched in the gate check above.

-- Unattributed cleanup (gc_unattributed_activity, no transaction)
DELETE FROM observations WHERE session_id NOT IN (SELECT session_id FROM sessions);
DELETE FROM query_log WHERE session_id NOT IN (SELECT session_id FROM sessions);
DELETE FROM injection_log
  WHERE session_id IN (
    SELECT session_id FROM sessions WHERE feature_cycle IS NULL AND status != 0
  );
DELETE FROM sessions WHERE feature_cycle IS NULL AND status != 0;

-- audit_log time-based GC
DELETE FROM audit_log
  WHERE timestamp < (strftime('%s', 'now') - ?1 * 86400);
```

---

## Constraints

1. **No schema migration.** Schema remains at v19. All GC operates on existing indexed columns. `raw_signals_available` update is a data write, not a schema change.
2. **observations and query_log have no `feature_cycle` column.** Every cycle-scoped DELETE must join through `sessions`. Two-hop pattern is mandatory.
3. **Delete order within per-cycle transaction is fixed:** `observations` → `query_log` → `injection_log` → `sessions`. Deleting `sessions` last ensures the `IN (SELECT session_id FROM sessions WHERE feature_cycle = ?)` subquery resolves within the same transaction.
4. **`pool.begin()` / `tx.commit()` API required.** Never issue raw `BEGIN`/`COMMIT` SQL (entry #2159 pattern: sqlx does not guarantee connection identity across `.execute()` calls without a transaction handle).
5. **`write_pool_server()` max_connections = 1.** Per-cycle transaction must acquire, operate, commit, and release the connection before the next cycle. The entire multi-cycle loop must not hold the connection.
6. **`raw_signals_available` update uses `store_cycle_review()` with struct update syntax.** Call `store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record })` using the gate-check record retained in scope. Do NOT discard the record and reconstruct it — reconstruction clobbers `summary_json`.
7. **Both 60-day DELETE sites removed unconditionally.** `status.rs` ~1372–1384 and `tools.rs` ~1630–1642. No flags, no conditions. Running both GC policies concurrently is not supported.
8. **crt-033 gate is unconditional.** `get_cycle_review()` must return `Ok(Some(_))` before any data for a cycle is deleted. No bypass.
9. **`RetentionConfig` loaded once at startup, passed by value.** Must not be re-read from `config.toml` on each tick (SR-09 mitigation, NFR-06).
10. **Active sessions excluded from unattributed cleanup.** Sessions with `feature_cycle IS NULL` and `status = Active` (status numeric 0) must not be pruned (SR-06 mitigation).

---

## Dependencies

### Crate Dependencies (all existing)

| Dependency | Reason |
|------------|--------|
| `sqlx` (sqlite, runtime-tokio, macros) | All GC queries |
| `tracing` | Structured log output (FR-09) |
| `serde` | `RetentionConfig` deserialization |

### Internal Component Dependencies

| Component | Location | Role |
|-----------|----------|------|
| `cycle_review_index` | `crates/unimatrix-store/src/cycle_review_index.rs` | Gate check (`get_cycle_review`), `list_purgeable_cycles` source |
| `gc_sessions()` | `crates/unimatrix-store/src/sessions.rs` | Reference implementation for per-cycle cascade delete pattern |
| `write_pool_server()` | `crates/unimatrix-store/src/lib.rs` | All GC write operations |
| `InferenceConfig::validate()` | `crates/unimatrix-server/src/infra/config.rs` | Template for `RetentionConfig::validate()` pattern |
| `background.rs` `run_single_tick()` | `crates/unimatrix-server/src/background.rs` | Threads `Arc<RetentionConfig>` into tick loop |

### Feature Dependencies

| Feature | Status | Reason |
|---------|--------|--------|
| crt-033 | Shipped | Provides `cycle_review_index` table and `get_cycle_review()` API — the crt-033 gate |

---

## NOT in Scope

- `co_access` table pruning — handled by 1-year staleness threshold from GH #408
- Entry auto-deprecation — separate knowledge lifecycle concern
- Changes to `cycle_events`, `cycle_review_index` schema, `observation_phase_metrics`, `entries`, or `GRAPH_EDGES` — none of these tables are pruned
- `cycle_events` lifecycle hook rows (`cycle_start`, `cycle_stop`, `cycle_phase_end`) — structural record of cycle history, explicitly excluded
- Scoring or confidence pipeline changes — this is pure data pruning
- Opt-in feature flag — GC is always-on when `activity_detail_retention_cycles > 0`
- Cycle-based filter in `PhaseFreqTable::rebuild` — deferred follow-on (ADR #3686); crt-036 delivers data retention boundary only; the warning guard (FR-10) is advisory
- NLI model or scoring changes

---

## Alignment Status

**Overall: PASS with one pre-resolved variance.**

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — directly protects data quality for GNN (W3-1) and PhaseFreqTable (WA-2) consumers |
| Milestone Fit | PASS — correctly scoped Cortical housekeeping; no future-wave capability introduced; schema stays at v19 |
| Scope Gaps | PASS — all SCOPE.md goals, ACs, and constraints addressed |
| Scope Additions | WARN (accepted) — `max_cycles_per_tick` and FR-10 PhaseFreqTable guard are not in SCOPE.md but directly resolve SR-01 and SR-07; low-risk additions |
| Architecture Consistency | VARIANCE-01: RESOLVED before synthesis — SPECIFICATION FR-03/FR-06 incorrectly described `raw_signals_available` UPDATE as inside the per-cycle transaction; ARCHITECTURE and ADR-001 are correct: `mark_signals_purged()` runs OUTSIDE the transaction after commit. Implementers must follow the architecture design, not the now-corrected spec language. |
| Risk Completeness | PASS — 16 risks registered; all 9 scope risks traced; 8 non-negotiable gate blockers identified |

**Implementer note on VARIANCE-01 (resolved):** After `gc_cycle_activity()` commits and the connection is released, call `store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record })` where `record` is the `CycleReviewRecord` fetched during the gate check (`get_cycle_review()`). Retain that record in scope across the `gc_cycle_activity()` call. Do not include this call inside `gc_cycle_activity()`'s transaction block — `store_cycle_review()` takes `&self`, not a transaction handle.

---

## run_maintenance() Step Ordering (post-crt-036)

```
0a. Prune quarantined vectors
0b. Heal pass (re-embed)
1.  Co-access stale pair cleanup
2.  Confidence refresh
2b. Empirical prior computation
3.  Graph compaction
4.  Cycle-based activity GC  ← replaces old 60-day DELETE (step 4)
    - PhaseFreqTable alignment check (warn if query_log_lookback_days > retention coverage)
    - list_purgeable_cycles(k, max_per_tick)
    - for each purgeable cycle: gate → gc_cycle_activity → mark_signals_purged
    - gc_unattributed_activity()
4f. audit_log time-based GC  ← new (4f avoids collision with sub-steps 4a–4e)
5.  Stale session sweep
6.  Session GC (existing time-based gc_sessions — continues for sessions not covered by cycle GC)
```
