# crt-047: Curation Health Metrics — Architecture

## System Overview

crt-047 adds a complementary health signal to Unimatrix alongside the existing Lambda
metric. Where Lambda measures structural integrity (embedding consistency, graph
connectivity, contradiction density), curation health measures whether the *correction
process* is functioning: are agents catching and fixing stale entries in-flow, or is
drift accumulating silently?

The feature is read-only at the write path. It queries ENTRIES at review time and reads
`cycle_review_index` at status time. No MCP tool write paths are changed.

The feature ships entirely within two layers:

- **`unimatrix-store`**: schema migration (v23 → v24), `CycleReviewRecord` struct
  extension, store method for baseline window reads
- **`unimatrix-server`**: `CurationSnapshot` computation at review time,
  `services/curation_health.rs` (new file), `context_cycle_review` output block,
  `context_status` curation health phase

---

## Component Breakdown

### 1. Schema layer (`unimatrix-store`)

**`cycle_review_index.rs`**

- `CycleReviewRecord` gains seven new fields: `corrections_total`, `corrections_agent`,
  `corrections_human`, `corrections_system`, `deprecations_total`, `orphan_deprecations`,
  `first_computed_at` (all `i64`, default 0) — per ADR-001 and ADR-002
- `store_cycle_review()` updated to use a two-step upsert preserving `first_computed_at`
  (plain `INSERT OR REPLACE` would reset it — ADR-001)
- `get_cycle_review()` updated to select and map all seven new columns
- `SUMMARY_SCHEMA_VERSION` bumped from `1` to `2`
- New method: `get_curation_baseline_window(n: usize) -> Result<Vec<CurationBaselineRow>>`
  ordered by `first_computed_at DESC` with `WHERE first_computed_at > 0` (ADR-001)

**`migration.rs`**

- New `v23 → v24` block using `pragma_table_info` pre-check per column (five `ADD COLUMN`
  statements) wrapped in the existing outer transaction atomicity boundary (ADR-002)

**`db.rs`** (fresh-schema DDL)

- `CREATE TABLE IF NOT EXISTS cycle_review_index` DDL updated with the five new columns
  (byte-identical to what migration adds)

### 2. Computation layer (`unimatrix-server/services/`)

**`services/curation_health.rs`** (new file, extracted per SR-06 pre-plan)

Contains:
- `CurationSnapshot` struct — five raw count fields
- `CurationBaselineRow` struct — slim projection from `CycleReviewRecord` for baseline
- `CurationBaseline` struct — mean/stddev per metric
- `CurationBaselineComparison` struct — σ position for each metric, history length
- `CurationHealthSummary` struct — aggregate view for `context_status`
- `compute_curation_snapshot()` — queries ENTRIES for a given `feature_cycle`
- `compute_curation_baseline()` — pure function, no I/O
- `compute_trend()` — last-5 vs prior-5 mean comparison, returns `None` below 6 cycles

`services/status.rs` adds Phase 7c calling into `curation_health.rs` for the aggregate
view. It does not inline curation logic.

### 3. Output layer (`unimatrix-server/mcp/response/`)

**`response/cycle_review.rs`** (or wherever `RetrospectiveReport` is defined)

- New `curation_health: Option<CurationHealthBlock>` field on `RetrospectiveReport`
- `CurationHealthBlock` contains `snapshot: CurationSnapshot` and
  `baseline: Option<CurationBaselineComparison>`

**`response/status.rs`**

- New `curation_health: Option<CurationHealthSummary>` field on `StatusReport`

---

## Component Interactions

```
context_cycle_review (tools.rs)
    |
    +--> compute_curation_snapshot() [services/curation_health.rs]
    |       SQL: SELECT trust_source, supersedes, status FROM entries
    |            WHERE feature_cycle = ?  [read_pool()]
    |
    +--> store_cycle_review() [cycle_review_index.rs]
    |       INSERT OR REPLACE with snapshot columns  [write_pool_server()]
    |
    +--> get_curation_baseline_window() [cycle_review_index.rs]
    |       SELECT N rows WHERE first_computed_at > 0
    |       ORDER BY first_computed_at DESC  [read_pool()]
    |
    +--> compute_curation_baseline() [services/curation_health.rs]
            pure function, returns Option<CurationBaseline>

context_status (tools.rs)
    |
    +--> Phase 7c: get_curation_baseline_window(N) [cycle_review_index.rs]
    |       SELECT N rows WHERE first_computed_at > 0
    |       ORDER BY first_computed_at DESC  [read_pool()]
    |
    +--> compute_curation_summary() [services/curation_health.rs]
            pure function, returns CurationHealthSummary
```

**Pool discipline** (consistent with ADR-001, crt-033):

| Operation | Pool |
|-----------|------|
| `compute_curation_snapshot()` SQL reads | `read_pool()` |
| `store_cycle_review()` INSERT OR REPLACE | `write_pool_server()` |
| `get_curation_baseline_window()` SELECT | `read_pool()` |
| `context_status` curation phase reads | `read_pool()` |

---

## Technology Decisions

| Decision | Choice | ADR |
|----------|--------|-----|
| Baseline window ordering key | `first_computed_at DESC` (not `computed_at` or `feature_cycle` — both unstable/non-temporal) | ADR-001 |
| `trust_source` bucketing | `"agent"` → agent bucket; `"human"` → human bucket; `"system"` / `"direct"` excluded; informational `corrections_system` field added | ADR-002 |
| Orphan attribution mechanism | ENTRIES-based at review time (not AUDIT_LOG join) | ADR-003 |
| Migration atomicity | Single outer transaction, `pragma_table_info` per column, three-path update | ADR-004 |
| σ threshold | 1.5σ, both directions, named constant | SCOPE.md OQ-03 resolved |
| Cold-start thresholds | σ at 3 cycles; trend at 6 cycles | SCOPE.md OQ-04 resolved |
| `corrections_system` field | Included as informational `u32` (not counted in agent or human totals) | ADR-002 |
| `services/curation_health.rs` extraction | Pre-planned extraction (not reactive) | SR-06 addressed |

---

## Integration Points

### Existing components touched

| Component | Change |
|-----------|--------|
| `unimatrix-store/src/cycle_review_index.rs` | New columns on `CycleReviewRecord`, new store method, `SUMMARY_SCHEMA_VERSION` → 2 |
| `unimatrix-store/src/migration.rs` | v23→v24 block, `CURRENT_SCHEMA_VERSION` → 24 |
| `unimatrix-store/src/db.rs` | DDL for fresh schema updated with new columns |
| `unimatrix-server/src/services/status.rs` | Phase 7c added (delegates to curation_health.rs) |
| `unimatrix-server/src/mcp/tools.rs` | `context_cycle_review` step 8a extended to compute + store snapshot |
| `RetrospectiveReport` | New optional `curation_health` field |
| `StatusReport` | New optional `curation_health` field |

### New components

| Component | Location |
|-----------|----------|
| `CurationSnapshot` + baseline types + compute functions | `crates/unimatrix-server/src/services/curation_health.rs` |

---

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `CycleReviewRecord.corrections_total` | `i64` | `unimatrix-store/src/cycle_review_index.rs` |
| `CycleReviewRecord.corrections_agent` | `i64` | same |
| `CycleReviewRecord.corrections_human` | `i64` | same |
| `CycleReviewRecord.corrections_system` | `i64` | same (informational) |
| `CycleReviewRecord.deprecations_total` | `i64` | same |
| `CycleReviewRecord.orphan_deprecations` | `i64` | same |
| `SqlxStore::get_curation_baseline_window(n: usize)` | `-> Result<Vec<CurationBaselineRow>>` | `unimatrix-store/src/cycle_review_index.rs` |
| `CurationSnapshot` | `{ corrections_total: u32, corrections_agent: u32, corrections_human: u32, corrections_system: u32, deprecations_total: u32, orphan_deprecations: u32 }` | `services/curation_health.rs` |
| `CurationBaselineComparison` | `{ corrections_total_sigma: f64, orphan_ratio_sigma: f64, history_cycles: usize, within_normal_range: bool }` | `services/curation_health.rs` |
| `CurationHealthSummary` | `{ correction_rate_mean: f64, correction_rate_stddev: f64, agent_pct: f64, human_pct: f64, orphan_ratio_mean: f64, orphan_ratio_stddev: f64, trend: Option<TrendDirection>, cycles_in_window: usize }` | `services/curation_health.rs` |
| `CURATION_BASELINE_WINDOW: usize` | `= 10` | `services/status.rs` |
| `CURATION_SIGMA_THRESHOLD: f64` | `= 1.5` | `services/curation_health.rs` |
| `CURATION_MIN_HISTORY: usize` | `= 3` | `services/curation_health.rs` |
| `CURATION_MIN_TREND_HISTORY: usize` | `= 6` | `services/curation_health.rs` |
| `RetrospectiveReport.curation_health` | `Option<CurationHealthBlock>` | `mcp/response/cycle_review.rs` |
| `StatusReport.curation_health` | `Option<CurationHealthSummary>` | `mcp/response/status.rs` |

---

## force=true Semantics (SR-05)

Three distinct cases, defined explicitly:

1. **Current-cycle raw snapshot** (`force=true` on the current cycle): `compute_curation_snapshot()`
   re-runs the ENTRIES queries and overwrites the snapshot columns via `INSERT OR REPLACE`.
   The correcting entry's `feature_cycle` is the join key; this is the only snapshot
   that can legitimately change after initial write.

2. **Historical-cycle raw snapshot** (`force=true` on a past cycle): same code path —
   the snapshot is recomputed from ENTRIES. This is intentional: entries may have been
   purged or corrected since the first write. The operator is signalling intent to
   recompute.

3. **Rolling aggregate** (mean/stddev/trend in `context_status`): always recomputed from
   the current `cycle_review_index` snapshot window on each call. There is no memoized
   aggregate for the rolling stats.

---

## `SUMMARY_SCHEMA_VERSION` Bump Impact (SR-04)

Bumping from 1 to 2 causes every existing `cycle_review_index` row to be detected as
stale when `context_cycle_review force=false` is called on historical cycles. This is the
designed advisory behavior from crt-033 ADR-002. The blast radius is all historical cycles.

Operators should run a batch `force=true` pass after deploying v24 if they want to
populate curation snapshots for historical cycles. Without a force-recompute pass,
historical cycles will permanently show schema_version=1 advisory and will have NULL
curation snapshot columns (handled as 0 by baseline logic).

---

## Cold-Start and NULL Handling

Pre-crt-047 cycles will have NULL in the five new snapshot columns (SQLite DEFAULT 0 on
existing rows via migration, but any row written before crt-047 will have been written
without the new columns — migration sets DEFAULT 0 on existing rows, so they will be 0,
not NULL). New cycles written after deployment will have real computed values.

Baseline computation:
- A row with `corrections_total = 0` and `deprecations_total = 0` (migrated legacy row)
  is indistinguishable from a cycle with zero corrections. These rows are counted in the
  window but have no curation signal.
- `compute_curation_baseline()` must accept rows with all-zero snapshots without NaN.
- Orphan ratio: when `deprecations_total = 0`, the ratio is defined as `0.0`.
- The history-length annotation in output (OQ-05) informs operators how many cycles
  contributed to the baseline, letting them judge early-window reliability.

---

## Out-of-Cycle Deprecations (SR-08)

Deprecations called outside an active cycle (human-initiated, no running feature cycle)
are intentionally excluded from all cycle counts. This is because orphan attribution is
cycle-scoped. The `context_status` aggregate view surfaces the total orphan count
across the stored snapshot window; out-of-cycle orphans that predate crt-047 or occur
between cycles are not attributed and are not separately surfaced. This is documented as
a known exclusion, not a silent drop.

---

## Open Questions

None. All scope and risk questions resolved through source code analysis and ADR decisions.
