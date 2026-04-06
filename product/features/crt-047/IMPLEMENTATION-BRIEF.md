# crt-047: Curation Health Metrics — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-047/SCOPE.md |
| Architecture | product/features/crt-047/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-047/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-047/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-047/ALIGNMENT-REPORT.md |
| ADR-001 | product/features/crt-047/architecture/ADR-001-baseline-ordering-key.md |
| ADR-002 | product/features/crt-047/architecture/ADR-002-trust-source-bucketing.md |
| ADR-003 | product/features/crt-047/architecture/ADR-003-orphan-deprecation-attribution.md |
| ADR-004 | product/features/crt-047/architecture/ADR-004-migration-strategy.md |
| ADR-005 | product/features/crt-047/architecture/ADR-005-curation-health-module-extraction.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| schema / cycle_review_index | pseudocode/cycle_review_index.md | test-plan/cycle_review_index.md |
| migration v23→v24 | pseudocode/migration.md | test-plan/migration.md |
| services/curation_health | pseudocode/curation_health.md | test-plan/curation_health.md |
| context_cycle_review handler | pseudocode/context_cycle_review.md | test-plan/context_cycle_review.md |
| context_status Phase 7c | pseudocode/context_status_phase7c.md | test-plan/context_status_phase7c.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Add per-cycle curation health metrics to Unimatrix — measuring whether the correction
process is functioning — by computing a `CurationSnapshot` at `context_cycle_review` time,
persisting it as seven new columns on `cycle_review_index` (schema v24), and surfacing a
rolling σ baseline comparison in both `context_cycle_review` and `context_status` output.
The signal distinguishes agent in-flow corrections from human post-hoc corrections and
flags orphan deprecations, enabling detection of silent drift accumulation without
domain-specific thresholds.

---

## Critical Contradiction Resolutions

Two contradictions between ADRs and SPECIFICATION were identified (FAIL-01, FAIL-02) and
are authoritatively resolved here. **The ADRs govern. The SPECIFICATION sections listed
below are superseded and must not be followed.**

### Resolution 1 — FAIL-01: Orphan Deprecation Attribution (ADR-003 is authoritative)

**Superseded specification sections**: FR-05, FR-06, AC-04 (AUDIT_LOG join approach),
NFR-03 (single SQL query joining audit_log), Domain Models "Attribution" section,
Workflow 3 step 2.

**Authoritative approach (ADR-003)**: Use ENTRIES-only queries. No AUDIT_LOG join.

Source code analysis proves that every write path that produces an orphan deprecation
(`superseded_by IS NULL`) goes through explicit `context_deprecate`. The two
chain-deprecation paths (`context_correct` and lesson-learned auto-supersede) always set
`superseded_by` to the new entry's ID and can never produce orphans. Therefore the
AUDIT_LOG join is redundant and adds JSON-array parse complexity with no correctness
benefit.

**`orphan_deprecations` SQL**:
```sql
SELECT COUNT(*) FROM entries
WHERE status = 'deprecated'
  AND superseded_by IS NULL
  AND updated_at >= ?1   -- cycle_start_ts
  AND updated_at <= ?2   -- review_ts
```

**`deprecations_total` SQL** (all deprecations in window, including chain-deprecations):
```sql
SELECT COUNT(*) FROM entries
WHERE status = 'deprecated'
  AND updated_at >= ?1
  AND updated_at <= ?2
```

`cycle_start_ts` is derived from `MIN(timestamp)` for `event_type = 'cycle_start'` in
`cycle_events` for the current `cycle_id`. The caller (`context_cycle_review`) already
reads `cycle_events` via `get_cycle_start_goal`. If no `cycle_start` event exists,
fall back to `0` (documents the over-count risk; log a warning).

**OQ-SPEC-01 disposition**: Vacuous. The ENTRIES-only approach does not join AUDIT_LOG,
so the question of filtering `outcome = 'Success'` on audit rows does not apply.

**WARN-02 disposition**: Closed. No AUDIT_LOG join; no outcome filter needed.

---

### Resolution 2 — FAIL-02: Baseline Window Ordering Key (ADR-001 is authoritative)

**Superseded specification sections**: FR-08 (lists 5 columns), FR-10 (`ORDER BY
feature_cycle DESC`), AC-14 (references 5 new columns), Domain Models "CurationBaseline"
and "Cycle Window" ordering prose.

**Authoritative approach (ADR-001 + ADR-002)**: Seven new columns in v24. Order by
`first_computed_at DESC`.

`feature_cycle` sorts alphabetically by phase prefix and does not equal temporal order
across phases (`alc`, `col`, `crt`, `nxs`, `vnc` do not sort chronologically). `computed_at`
is mutable on `force=true` recompute. `first_computed_at` is set once on first insert and
preserved on all subsequent overwrites via a two-step upsert.

**Seven new columns for v24** (`cycle_review_index`):
1. `corrections_total INTEGER NOT NULL DEFAULT 0`
2. `corrections_agent INTEGER NOT NULL DEFAULT 0`
3. `corrections_human INTEGER NOT NULL DEFAULT 0`
4. `corrections_system INTEGER NOT NULL DEFAULT 0` (informational; per ADR-002)
5. `deprecations_total INTEGER NOT NULL DEFAULT 0`
6. `orphan_deprecations INTEGER NOT NULL DEFAULT 0`
7. `first_computed_at INTEGER NOT NULL DEFAULT 0`

`corrections_total = corrections_agent + corrections_human` (computed, NOT stored as a
separate column). `corrections_system` IS a stored column. The `corrections_total`
value written to `cycle_review_index` is computed at review time as
`corrections_agent + corrections_human` before the INSERT.

`get_curation_baseline_window()` query:
```sql
SELECT corrections_total, corrections_agent, corrections_human,
       corrections_system, deprecations_total, orphan_deprecations,
       feature_cycle, schema_version
FROM cycle_review_index
WHERE first_computed_at > 0
ORDER BY first_computed_at DESC
LIMIT ?1
```

**upsert pattern for `first_computed_at` preservation**:
`store_cycle_review()` must NOT use plain `INSERT OR REPLACE` for the full row.
Instead, use a two-step approach:
1. Check whether the row already exists: `SELECT first_computed_at FROM cycle_review_index WHERE feature_cycle = ?`.
2. If row does not exist: INSERT with `first_computed_at = cycle_start_ts` (or `now` if no cycle_start event).
3. If row exists: UPDATE all columns except `first_computed_at`.

---

### Resolution 3 — WARN-01: `corrections_system` is a Stored Column (ADR-002 is authoritative)

**Superseded specification section**: FR-08 omits `corrections_system`; OQ-SPEC-02 marks
it as ADR-gated. Both are superseded.

**Authoritative approach (ADR-002)**: `corrections_system` is included in:
- `CurationSnapshot` struct as `corrections_system: u32`
- `CycleReviewRecord` struct as `corrections_system: i64`
- `cycle_review_index` DDL as `corrections_system INTEGER NOT NULL DEFAULT 0` (column 4 of 7)
- `store_cycle_review()` INSERT/UPDATE bind
- `get_curation_baseline_window()` SELECT

It is surfaced in `context_cycle_review` and `context_status` output as informational,
but excluded from `corrections_total` and from the σ baseline computation.

**OQ-SPEC-02 disposition**: Closed. ADR-002 resolved to include the column. It is stored.

---

## Resolved Decisions Table

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Baseline window ordering key | `first_computed_at DESC`; new column set on first insert, preserved on overwrite | ADR-001 | architecture/ADR-001-baseline-ordering-key.md |
| `trust_source` bucketing | `"agent"` → `corrections_agent`; `"human"`/`"privileged"` → `corrections_human`; all other values → `corrections_system` (informational); `corrections_total = agent + human` | ADR-002 | architecture/ADR-002-trust-source-bucketing.md |
| Orphan attribution mechanism | ENTRIES-only: `status = 'deprecated' AND superseded_by IS NULL AND updated_at IN [cycle_start_ts, review_ts]`. No AUDIT_LOG join. | ADR-003 | architecture/ADR-003-orphan-deprecation-attribution.md |
| Migration atomicity and paths | Single outer-transaction v24 block; `pragma_table_info` pre-check per column; two active paths: `migration.rs` + `db.rs` (migration_compat.rs not relevant for v24) | ADR-004 | architecture/ADR-004-migration-strategy.md |
| `services/curation_health.rs` extraction | Pre-planned new file; all curation types and pure compute functions live there; `status.rs` Phase 7c is ~15-20 lines | ADR-005 | architecture/ADR-005-curation-health-module-extraction.md |
| σ threshold | `CURATION_SIGMA_THRESHOLD = 1.5`; both directions flagged; matches `unimatrix_observe::baseline` ADR-003 | SCOPE OQ-03 | — |
| Cold-start thresholds | σ at MIN_HISTORY=3 cycles; trend at MIN_TREND_HISTORY=6 cycles; independent | SCOPE OQ-04/OQ-05 | — |
| `force=true` semantics | Case A (current cycle): recompute snapshot from ENTRIES; Case B (historical): same code path; Case C (rolling aggregate): always recomputed from snapshot window on each call, never cached | SPEC Constraints § force=true | — |
| `SUMMARY_SCHEMA_VERSION` bump | 1 → 2; all historical rows trigger advisory on `force=false`; designed behavior per crt-033 ADR-002 | SPEC FR-15, AC-11 | — |
| Out-of-cycle deprecations | Excluded from all cycle counts (updated_at falls outside all cycle windows); documented exclusion, not a bug; not separately surfaced in this feature | ADR-003, ARCH § SR-08 | — |
| `corrections_system` stored | Included as stored column in DDL and struct; informational only; excluded from σ baseline | ADR-002 | architecture/ADR-002-trust-source-bucketing.md |
| OQ-SPEC-01 (outcome filter) | Vacuous — ENTRIES-only approach does not query AUDIT_LOG | ADR-003 resolution | — |

---

## Files to Create / Modify

### New files

| Path | Summary |
|------|---------|
| `crates/unimatrix-server/src/services/curation_health.rs` | All curation types (`CurationSnapshot`, `CurationBaseline`, `CurationBaselineComparison`, `CurationHealthSummary`, `CurationHealthBlock`, `TrendDirection`), all pure compute functions, and the async `compute_curation_snapshot()` |

### Modified files

| Path | Change |
|------|--------|
| `crates/unimatrix-store/src/cycle_review_index.rs` | Add 7 fields to `CycleReviewRecord`; new `CurationBaselineRow` struct; update `store_cycle_review()` with two-step upsert preserving `first_computed_at`; update `get_cycle_review()`; add `get_curation_baseline_window(n: usize)`; bump `SUMMARY_SCHEMA_VERSION` to `2` |
| `crates/unimatrix-store/src/migration.rs` | Add `v23 → v24` block (7 columns, `pragma_table_info` pre-check each, outer transaction); bump `CURRENT_SCHEMA_VERSION` to `24` |
| `crates/unimatrix-store/src/db.rs` | Add 7 new columns to `CREATE TABLE IF NOT EXISTS cycle_review_index` DDL |
| `crates/unimatrix-server/src/services/status.rs` | Add Phase 7c (~15-20 lines): call `get_curation_baseline_window(CURATION_BASELINE_WINDOW)` then `curation_health::compute_curation_summary()`; add `CURATION_BASELINE_WINDOW: usize = 10` constant |
| `crates/unimatrix-server/src/services/mod.rs` | Add `pub mod curation_health` |
| `crates/unimatrix-server/src/mcp/tools.rs` | Extend `context_cycle_review` step 8a: call `compute_curation_snapshot()` (before `store_cycle_review()`), then pass snapshot into the updated `store_cycle_review()` |
| `crates/unimatrix-server/src/mcp/response/cycle_review.rs` | Add `curation_health: Option<CurationHealthBlock>` field to `RetrospectiveReport` |
| `crates/unimatrix-server/src/mcp/response/status.rs` | Add `curation_health: Option<CurationHealthSummary>` field to `StatusReport` |
| Existing migration tests (cascade) | Update `test_summary_schema_version_is_one` → assert 2; update `sqlite_parity.rs` schema version and column count assertions; update `server.rs` schema version assertions to 24 |

---

## Data Structures

### `CurationSnapshot` (domain type — `services/curation_health.rs`)
```rust
pub struct CurationSnapshot {
    pub corrections_total: u32,    // = corrections_agent + corrections_human (intentional curation only)
    pub corrections_agent: u32,    // trust_source = 'agent'
    pub corrections_human: u32,    // trust_source IN ('human', 'privileged')
    pub corrections_system: u32,   // all other trust_source values (informational, not in total)
    pub deprecations_total: u32,   // all entries with status='deprecated' in window
    pub orphan_deprecations: u32,  // deprecated AND superseded_by IS NULL in window
}
```

### `CurationBaselineRow` (slim projection — `services/curation_health.rs`)
```rust
pub struct CurationBaselineRow {
    pub corrections_total: i64,
    pub corrections_agent: i64,
    pub corrections_human: i64,
    pub deprecations_total: i64,
    pub orphan_deprecations: i64,
    pub schema_version: i64,   // used to exclude legacy DEFAULT-0 rows (schema_version < 2)
}
```

### `CurationBaseline` (rolling aggregate — `services/curation_health.rs`)
```rust
pub struct CurationBaseline {
    pub corrections_total_mean: f64,
    pub corrections_total_stddev: f64,
    pub orphan_ratio_mean: f64,    // orphan_deprecations / deprecations_total; 0.0 when denom=0
    pub orphan_ratio_stddev: f64,
    pub history_cycles: usize,     // number of rows that contributed (annotation in output)
}
```

### `CurationBaselineComparison` (per-cycle σ position — `services/curation_health.rs`)
```rust
pub struct CurationBaselineComparison {
    pub corrections_total_sigma: f64,
    pub orphan_ratio_sigma: f64,
    pub history_cycles: usize,
    pub within_normal_range: bool,  // false if either sigma > CURATION_SIGMA_THRESHOLD
}
```

### `TrendDirection` (enum — `services/curation_health.rs`)
```rust
pub enum TrendDirection { Increasing, Decreasing, Stable }
```

### `CurationHealthSummary` (status output — `services/curation_health.rs`)
```rust
pub struct CurationHealthSummary {
    pub correction_rate_mean: f64,
    pub correction_rate_stddev: f64,
    pub agent_pct: f64,            // corrections_agent / corrections_total (%)
    pub human_pct: f64,            // corrections_human / corrections_total (%)
    pub orphan_ratio_mean: f64,
    pub orphan_ratio_stddev: f64,
    pub trend: Option<TrendDirection>,  // None when fewer than 6 cycles
    pub cycles_in_window: usize,
}
```

### `CurationHealthBlock` (cycle_review output — `services/curation_health.rs`)
```rust
pub struct CurationHealthBlock {
    pub snapshot: CurationSnapshot,
    pub baseline: Option<CurationBaselineComparison>,  // None when fewer than MIN_HISTORY cycles
}
```

### `CycleReviewRecord` changes (7 new fields — `cycle_review_index.rs`)
Seven `i64` fields added: `corrections_total`, `corrections_agent`, `corrections_human`,
`corrections_system`, `deprecations_total`, `orphan_deprecations`, `first_computed_at`.

---

## Function Signatures

### `services/curation_health.rs`

```rust
pub const CURATION_SIGMA_THRESHOLD: f64 = 1.5;
pub const CURATION_MIN_HISTORY: usize = 3;
pub const CURATION_MIN_TREND_HISTORY: usize = 6;

/// Queries ENTRIES for the given feature_cycle using the cycle window bounds.
/// Uses read_pool(). Returns CurationSnapshot with all six fields populated.
pub async fn compute_curation_snapshot(
    store: &SqlxStore,
    feature_cycle: &str,
    cycle_start_ts: i64,
    review_ts: i64,
) -> Result<CurationSnapshot, ServiceError>

/// Pure function. Returns None when fewer than CURATION_MIN_HISTORY rows have
/// real snapshot data (schema_version >= 2 OR any non-zero snapshot field).
pub fn compute_curation_baseline(
    rows: &[CurationBaselineRow],
    n: usize,
) -> Option<CurationBaseline>

/// Pure function. Returns CurationBaselineComparison including σ distance and
/// history annotation. Zero stddev produces sigma=None (NoVariance equivalent).
pub fn compare_to_baseline(
    snapshot: &CurationSnapshot,
    baseline: &CurationBaseline,
    history_count: usize,
) -> CurationBaselineComparison

/// Pure function. Returns None when fewer than CURATION_MIN_TREND_HISTORY rows.
/// Computes mean of last 5 rows vs mean of rows 6-10 in the ordered window.
pub fn compute_trend(rows: &[CurationBaselineRow]) -> Option<TrendDirection>

/// Pure function. Aggregates window into CurationHealthSummary for context_status.
/// Returns None when the window is empty.
pub fn compute_curation_summary(rows: &[CurationBaselineRow]) -> Option<CurationHealthSummary>
```

### `cycle_review_index.rs`

```rust
/// Reads the last n rows ordered by first_computed_at DESC, excluding rows
/// where first_computed_at = 0 (legacy pre-v24 rows with no temporal anchor).
pub async fn get_curation_baseline_window(
    &self,
    n: usize,
) -> Result<Vec<CurationBaselineRow>, StoreError>
```

### `services/status.rs`

```rust
const CURATION_BASELINE_WINDOW: usize = 10;
```

---

## Constraints

1. **`write_pool_server()` is a single-connection serializer.** `compute_curation_snapshot()`
   uses `read_pool()`. The snapshot INSERT/UPDATE in `store_cycle_review()` uses `write_pool_server()`.
   The compute step (read) must complete before the write step begins.

2. **`store_cycle_review()` must not clobber `first_computed_at`.** Plain `INSERT OR REPLACE`
   deletes then reinserts the row, resetting `first_computed_at`. Use the two-step upsert: read
   existing `first_computed_at`, then INSERT (new row) or UPDATE (existing row).

3. **`corrections_total` is computed, not a standalone count.** It equals
   `corrections_agent + corrections_human`. `corrections_system` entries are excluded.
   The value stored in the column is this sum, not `COUNT(*) WHERE supersedes IS NOT NULL`.

4. **Schema v24 migration uses `pragma_table_info` pre-check per column** (SQLite has no
   `ADD COLUMN IF NOT EXISTS`). All seven columns are added in a single version block.
   Version counter is updated only after all columns are verified/added.

5. **`first_computed_at = 0` rows are excluded from `get_curation_baseline_window()`.** These
   are legacy pre-v24 rows with no temporal anchor. Operators who want them in the baseline
   must call `context_cycle_review force=true` for each such cycle.

6. **Baseline computation excludes legacy DEFAULT-0 rows from `MIN_HISTORY` count.** Rows
   where `schema_version < 2` AND all snapshot fields are zero are treated as missing data.
   A real zero-correction cycle (schema_version = 2, all zeros) IS included.

7. **`orphan_ratio` division guard.** When `deprecations_total = 0`, the ratio is defined
   as `0.0`. No NaN may propagate into σ baseline or output.

8. **`compute_curation_snapshot()` is called before `store_cycle_review()`.** Read from
   ENTRIES first, then write the complete record including snapshot columns.

9. **Schema cascade.** Bumping `CURRENT_SCHEMA_VERSION` to 24 and `SUMMARY_SCHEMA_VERSION`
   to 2 cascades to multiple test files. Pre-delivery grep check:
   `grep -r 'schema_version.*== 23' crates/` must return zero matches after bumping.

10. **Max 500 lines per file.** `services/status.rs` already exceeds 500 lines; all curation
    logic goes in `services/curation_health.rs`. Phase 7c in `status.rs` is ~15-20 lines only.

11. **Pre-delivery schema version check (SR-02, ADR-004).** SM must run
    `grep CURRENT_SCHEMA_VERSION crates/unimatrix-store/src/migration.rs` before delivery
    begins to confirm v24 has not been claimed by a parallel feature.

---

## Dependencies

### Crates
- `crates/unimatrix-store` — `cycle_review_index.rs`, `migration.rs`, `db.rs`
- `crates/unimatrix-server` — `services/curation_health.rs` (new), `services/status.rs`,
  `mcp/tools.rs`, `mcp/response/cycle_review.rs`, `mcp/response/status.rs`, `services/mod.rs`
- `unimatrix_observe::baseline` — referenced for convention alignment (population stddev,
  `BaselineStatus` enum pattern); the new baseline function is a separate implementation
  in `unimatrix-server` to preserve the server → store dependency direction

### External tables (read-only)
- `ENTRIES` — columns: `supersedes`, `superseded_by`, `trust_source`, `feature_cycle`,
  `status`, `updated_at`
- `cycle_events` — columns: `cycle_id`, `event_type`, `timestamp` (for `cycle_start_ts`)
- `cycle_review_index` — read (baseline window) and write (snapshot columns)

### No new crate dependencies required.

---

## NOT in Scope

- Individual entry lifecycle decisions (dead or never-injected entries) — #363 / #370
- Lambda modification or Lambda freshness redesign — #520
- Per-topic σ baselines — corpus-wide only; topic segmentation is a follow-on
- Intentional curation burst suppression flag
- Backfilling historical curation data — pre-v24 cycles retain DEFAULT 0 snapshots
- Changes to `context_correct` or `context_deprecate` write paths
- Unattributed orphan count surfaced separately in `context_status` (documented exclusion)
- Batch `force=true` tooling for historical cycles

---

## Alignment Status

**Overall**: PASS with two resolved FAIL variances and one resolved WARN.

| Check | Status | Resolution |
|-------|--------|------------|
| Vision Alignment | PASS | Curation health is complementary to Lambda; serves "self-learning integrity engine" without modifying Lambda or pulling in future-milestone work |
| Milestone Fit | PASS | Correctly scoped to Cortical phase; operates within existing Wave 1 infrastructure |
| Architecture Consistency | FAIL → RESOLVED | ADR-001 (ordering key) and ADR-003 (orphan attribution) supersede conflicting SPEC sections; Implementation Brief is the authoritative resolution document |
| Scope Additions | WARN → RESOLVED | `corrections_system` column (ADR-002) and `first_computed_at` column (ADR-001) are warranted additions beyond SCOPE.md; both are included in v24 DDL, structs, and migration |
| Risk Completeness | PASS | RISK-TEST-STRATEGY identified all FAIL/WARN variances at Critical priority before handoff |

### FAIL-01 resolution (orphan attribution)
ADR-003 ENTRIES-only approach governs. The SPEC's AUDIT_LOG join in FR-05, FR-06, AC-04,
and NFR-03 is superseded. The two approaches are equivalent given write-path analysis (only
`context_deprecate` produces orphans), but the ENTRIES-only approach is simpler and avoids
JSON array parsing. OQ-SPEC-01 (outcome filter) and WARN-02 (AUDIT_LOG outcome) are closed
as vacuous.

### FAIL-02 resolution (baseline ordering key)
ADR-001 `first_computed_at DESC` ordering governs. The SPEC's FR-10 `feature_cycle DESC`
ordering is superseded. Column count is seven (not five in FR-08): five snapshot columns +
`corrections_system` (ADR-002) + `first_computed_at` (ADR-001). AC-14 test must assert
all seven columns.

### WARN-01 resolution (`corrections_system` stored)
ADR-002 governs. The field is a stored column in DDL, migration, struct, and all I/O.
It is surfaced in output as informational only and excluded from σ baseline computation.

---

## Tracking

GH Issue: #529
