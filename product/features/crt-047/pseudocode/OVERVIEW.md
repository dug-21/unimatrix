# crt-047: Curation Health Metrics — Pseudocode Overview

## Components Involved

| Component | File(s) Modified/Created | Role |
|-----------|--------------------------|------|
| schema/cycle_review_index | `crates/unimatrix-store/src/cycle_review_index.rs` | Add 7 new fields to `CycleReviewRecord`; two-step upsert; `get_curation_baseline_window()` |
| migration v23→v24 | `crates/unimatrix-store/src/migration.rs`, `db.rs` | Add 7 columns to `cycle_review_index`; `CURRENT_SCHEMA_VERSION = 24` |
| services/curation_health | `crates/unimatrix-server/src/services/curation_health.rs` (new) | All curation types and pure compute functions |
| context_cycle_review handler | `crates/unimatrix-server/src/mcp/tools.rs` | Extend Step 8a: compute snapshot before store; pass into store |
| context_status Phase 7c | `crates/unimatrix-server/src/services/status.rs` | ~15-20 lines delegating to curation_health |

Additional files modified (cascade only, no pseudocode required):
- `crates/unimatrix-server/src/services/mod.rs` — add `pub(crate) mod curation_health`
- `crates/unimatrix-server/src/mcp/response/cycle_review.rs` — add `curation_health: Option<CurationHealthBlock>`
- `crates/unimatrix-server/src/mcp/response/status.rs` — add `curation_health: Option<CurationHealthSummary>`

---

## Data Flow Between Components

```
context_cycle_review (tools.rs)
  Step 8a-pre: compute_curation_snapshot()        [curation_health.rs / read_pool()]
    SQL reads: ENTRIES (corrections, deprecations, orphans)
    Returns: CurationSnapshot
  Step 8a-post: store_cycle_review(record)         [cycle_review_index.rs / write_pool_server()]
    Two-step upsert: read first_computed_at → INSERT (new) or UPDATE (existing)
    Snapshot fields written atomically with rest of record
  Post-store: get_curation_baseline_window(N)      [cycle_review_index.rs / read_pool()]
    Returns: Vec<CurationBaselineRow>
  compute_curation_baseline(rows, N)               [curation_health.rs / pure]
    Returns: Option<CurationBaseline>
  compare_to_baseline(snapshot, baseline, count)   [curation_health.rs / pure]
    Returns: CurationBaselineComparison
  Output: RetrospectiveReport.curation_health = Some(CurationHealthBlock { snapshot, baseline })

context_status (status.rs Phase 7c)
  get_curation_baseline_window(CURATION_BASELINE_WINDOW)   [cycle_review_index.rs / read_pool()]
    Returns: Vec<CurationBaselineRow>
  compute_curation_summary(rows)                            [curation_health.rs / pure]
    Returns: Option<CurationHealthSummary>
  Output: StatusReport.curation_health = curation_health
```

---

## Shared Types (all defined in `services/curation_health.rs`)

### Constants
```
CURATION_SIGMA_THRESHOLD: f64 = 1.5
CURATION_MIN_HISTORY: usize = 3       -- minimum rows for sigma comparison
CURATION_MIN_TREND_HISTORY: usize = 6 -- minimum rows for trend direction
```

### Structs (fields named exactly as in architecture Integration Surface)

**CurationSnapshot** — raw per-cycle counts computed from ENTRIES
```
corrections_total: u32    -- = corrections_agent + corrections_human (computed sum, NOT count(*))
corrections_agent: u32    -- trust_source = 'agent'
corrections_human: u32    -- trust_source IN ('human', 'privileged')
corrections_system: u32   -- all other trust_source values (informational only)
deprecations_total: u32   -- all deprecated entries in window (orphan + chain)
orphan_deprecations: u32  -- deprecated AND superseded_by IS NULL in window
```

**CurationBaselineRow** — slim projection from cycle_review_index for baseline computation
```
corrections_total: i64
corrections_agent: i64
corrections_human: i64
deprecations_total: i64
orphan_deprecations: i64
schema_version: i64       -- used to exclude legacy DEFAULT-0 rows (schema_version < 2)
```

**CurationBaseline** — rolling aggregate over N rows
```
corrections_total_mean: f64
corrections_total_stddev: f64
orphan_ratio_mean: f64    -- 0.0 when deprecations_total = 0 (NFR-02)
orphan_ratio_stddev: f64
history_cycles: usize     -- count of rows that contributed to baseline
```

**CurationBaselineComparison** — sigma position for context_cycle_review output
```
corrections_total_sigma: f64
orphan_ratio_sigma: f64
history_cycles: usize
within_normal_range: bool -- false if either sigma > CURATION_SIGMA_THRESHOLD
```

**TrendDirection** — enum
```
Increasing | Decreasing | Stable
```

**CurationHealthSummary** — aggregate view for context_status
```
correction_rate_mean: f64
correction_rate_stddev: f64
agent_pct: f64            -- corrections_agent / corrections_total (%)
human_pct: f64            -- corrections_human / corrections_total (%)
orphan_ratio_mean: f64
orphan_ratio_stddev: f64
trend: Option<TrendDirection>   -- None when fewer than 6 cycles
cycles_in_window: usize
```

**CurationHealthBlock** — context_cycle_review output container
```
snapshot: CurationSnapshot
baseline: Option<CurationBaselineComparison>   -- None when < MIN_HISTORY cycles
```

### CycleReviewRecord new fields (in `cycle_review_index.rs`)
Seven new `i64` fields added to the existing struct:
```
corrections_total: i64
corrections_agent: i64
corrections_human: i64
corrections_system: i64
deprecations_total: i64
orphan_deprecations: i64
first_computed_at: i64    -- set once on INSERT, never overwritten on UPDATE
```

---

## Build Sequencing Constraints

1. **`cycle_review_index.md`** (store layer) must be complete before `context_cycle_review.md`
   and `context_status_phase7c.md` — both call store methods.
2. **`migration.md`** (store layer) must be implemented before integration tests can run.
3. **`curation_health.md`** (services layer) must be complete before `context_cycle_review.md`
   and `context_status_phase7c.md` — both call its pure functions.
4. **`context_cycle_review.md`** and **`context_status_phase7c.md`** depend on 1, 2, 3.
5. Schema cascade: after bumping `CURRENT_SCHEMA_VERSION = 24` and `SUMMARY_SCHEMA_VERSION = 2`,
   pre-delivery grep checks must pass:
   - `grep -r 'schema_version.*== 23' crates/` → zero matches
   - `grep CURRENT_SCHEMA_VERSION crates/unimatrix-store/src/migration.rs` → confirms 24

---

## Pool Discipline

| Operation | Pool |
|-----------|------|
| `compute_curation_snapshot()` SQL reads | `read_pool()` |
| `store_cycle_review()` INSERT/UPDATE | `write_pool_server()` |
| `get_curation_baseline_window()` SELECT | `read_pool()` |
| `context_status` Phase 7c reads | `read_pool()` |

`write_pool_server()` is a single-connection serializer. The snapshot read (`compute_curation_snapshot`) MUST complete before `store_cycle_review()` acquires the write connection.
