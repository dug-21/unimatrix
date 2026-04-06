# crt-047: Curation Health Metrics

## Problem Statement

Unimatrix's Lambda metric measures structural integrity of the knowledge graph (embedding
consistency, graph connectivity, contradiction density). What it does not measure is whether
the *curation process* is functioning — are agents encountering stale or wrong entries and
correcting them in-flow, or is drift accumulating unnoticed?

This gap is invisible in the current data surface. A corpus with zero corrections in the
last 10 cycles looks identical whether agents are finding everything correct or finding
errors and not fixing them. The `trust_source` split between agent-in-flow corrections
and human-post-hoc corrections makes the distinction visible. Orphan deprecations (entries
removed without a successor) reveal whether correction chain discipline is holding.

Both metrics are self-calibrating: a rolling σ baseline trained on this corpus's own
history flags deviation in either direction without requiring domain-specific thresholds.

GH issue: #529.

---

## Goals

1. Add a `CurationSnapshot` struct containing per-cycle curation metrics:
   `corrections_total`, `corrections_agent`, `corrections_human`, `deprecations_total`,
   `orphan_deprecations` — computed at `context_cycle_review` call time.

2. Persist the snapshot as new columns on `cycle_review_index` (schema v24) so
   `context_status` can read aggregate curation health without re-running the pipeline.

3. Add a rolling σ baseline computation for curation metrics (N=10 cycles, configurable)
   analogous to the existing `unimatrix_observe::baseline` pattern, and include the σ
   position in `context_cycle_review` output.

4. Add a curation health block to `context_cycle_review` output — this cycle's raw
   numbers plus the σ deviation from the rolling baseline.

5. Add a curation health block to `context_status` output — corpus-wide aggregate view
   over the last N cycles: per-cycle rate, trend, source breakdown, orphan ratio.

6. Bump `SUMMARY_SCHEMA_VERSION` in `cycle_review_index.rs` so stale memoized records
   are detected and an advisory is surfaced (consistent with crt-033 ADR-002 bump policy).

---

## Non-Goals

- **Individual entry lifecycle decisions** — identifying and removing dead or never-injected
  entries is #363 / #370, not this feature.
- **Lambda modification** — curation health is a complementary corpus-level behavioral
  signal, not a Lambda dimension replacement or extension. The Lambda / freshness redesign
  is #520.
- **Per-topic σ baselines** — baseline computation is corpus-wide only. Topic-level
  segmentation is a follow-on.
- **Intentional curation burst suppression** — no override flag to silence σ anomalies
  during deliberate remediation runs. Operators interpret the signal in context.
- **Backfilling historical curation data** — cycles that predate crt-047 will have NULL
  snapshot columns; cold-start handling already applies (fewer than N cycles → raw numbers
  only, no σ comparison).
- **Changes to `context_correct` or `context_deprecate` call paths** — the feature reads
  from the ENTRIES table at review time; it does not instrument the write path of either
  tool.

---

## Background Research

### cycle_events table (current)

```
id             INTEGER PRIMARY KEY AUTOINCREMENT
cycle_id       TEXT    NOT NULL
seq            INTEGER NOT NULL
event_type     TEXT    NOT NULL
phase          TEXT
outcome        TEXT
next_phase     TEXT
timestamp      INTEGER NOT NULL
goal           TEXT
goal_embedding BLOB    -- crt-043
```

No curation columns. Current schema version: **23** (v22→v23 in bugfix-509).
Migration pattern: `pragma_table_info` pre-check per ADD COLUMN, outer transaction
atomicity boundary (crt-043 ADR-003, entry #4088).

### cycle_review_index table (current)

```
feature_cycle         TEXT    PRIMARY KEY
schema_version        INTEGER NOT NULL
computed_at           INTEGER NOT NULL
raw_signals_available INTEGER NOT NULL DEFAULT 1
summary_json          TEXT    NOT NULL
```

This is where curation snapshot columns belong. Rationale: curation metrics are
computed at review time (not during the cycle), mirroring the existing write path
in `context_cycle_review` step 8a. Reading from here in `context_status` avoids
re-running the full pipeline per status call.

### trust_source values in ENTRIES

Observed: `"agent"` (most agent-stored corrections), `"human"` (human-stored),
`"system"` (cortical implant lesson-learned writes), `"direct"` (embed_reconstruct).
`context_correct` hard-codes `trust_source: "agent"` for agent-called corrections
(tools.rs line ~637 and ~830). The correcting entry's `trust_source` field is the
reliable discriminator for agent vs. human attribution.

### Corrections and deprecations are on ENTRIES

- Corrections: entries where `supersedes IS NOT NULL` (the new entry supersedes the
  old). The correcting entry carries `trust_source` and `feature_cycle`.
- Orphan deprecations: entries where `status = Deprecated AND superseded_by IS NULL`.
  These were deprecated without a replacement. Attribution to a cycle comes from the
  `feature_cycle` column on the entry.

### Existing σ baseline pattern

`unimatrix_observe::baseline` already implements population stddev, `compute_entry()`,
`BaselineEntry { mean, stddev, sample_count }`, and `BaselineStatus` with four modes:
`Normal`, `Outlier`, `NoVariance`, `NewSignal`. The threshold is 1.5σ (ADR-003 in
`unimatrix-observe`).

The new curation baseline diverges in one dimension: it uses a configurable N-cycle
window (default 10) over `cycle_review_index` rows, reading the new snapshot columns
rather than MetricVector history. A parallel helper function in
`unimatrix-server/services/` (not `unimatrix-observe`) is appropriate — the dependency
direction from server → store is already established.

### context_status phase structure

compute_report() runs 8+ phases. The new curation health block is a new phase (Phase 7c
or equivalent) that:
1. Reads the last N `cycle_review_index` rows ordered by `computed_at DESC`.
2. Computes rolling mean/stddev for `corrections_total`, `corrections_agent`, `corrections_human`,
   and `orphan_deprecations / deprecations_total` (ratio).
3. Appends a `CurationHealthSummary` to `StatusReport`.

### SUMMARY_SCHEMA_VERSION

Currently `1` in `cycle_review_index.rs`. Must be bumped to `2` when curation snapshot
columns are added to `cycle_review_index`, since stored `summary_json` does not include
the new fields. Advisory on version mismatch: `"computed with schema_version {stored},
current is 2 — use force=true to recompute."` (same pattern as crt-033 ADR-002).

### #520 relationship

#520 proposes dropping the Lambda freshness dimension. crt-047 is explicitly
complementary: it introduces a *different* health signal (curation behavior) that does
not overlap with Lambda dimensions. No dependency — crt-047 can ship before or after #520.

---

## Proposed Approach

**Schema change (v23 → v24):** Add five INTEGER columns to `cycle_review_index`:
```
corrections_total     INTEGER NOT NULL DEFAULT 0
corrections_agent     INTEGER NOT NULL DEFAULT 0
corrections_human     INTEGER NOT NULL DEFAULT 0
deprecations_total    INTEGER NOT NULL DEFAULT 0
orphan_deprecations   INTEGER NOT NULL DEFAULT 0
```
Using `cycle_review_index` rather than `cycle_events` avoids fragmenting curation data
across the event stream; the snapshot is a derived aggregate, not a per-event record.
Migration uses `pragma_table_info` pre-check + outer transaction (established pattern).

**Snapshot computation at review time:** In `context_cycle_review`, after the existing
step 8a (store_cycle_review), compute `CurationSnapshot` by querying ENTRIES WHERE
`feature_cycle = ?` for corrections (`supersedes IS NOT NULL`) and deprecations
(`status = Deprecated`), grouped by `trust_source`. Store as additional columns in the
`CycleReviewRecord` INSERT OR REPLACE.

**Rolling baseline:** Pure function `compute_curation_baseline(rows: &[CycleReviewRow],
n: usize) -> Option<CurationBaseline>` — reads the snapshot columns, computes mean/stddev
for each metric over the window. Returns `None` when fewer than 3 rows have non-NULL
snapshot data (consistent with `unimatrix_observe::baseline::MIN_HISTORY`). The `n`
parameter is configurable, defaulting to `CURATION_BASELINE_WINDOW = 10`.

**context_cycle_review output:** New `curation_health` field on `RetrospectiveReport`
containing `CurationSnapshot` (this cycle's raw counts) and `CurationBaselineComparison`
(σ position relative to rolling baseline). When within normal range the format is brief;
when σ deviation exceeds the ADR-gated threshold (proposed 1.5σ, same as existing
baseline), a flag phrase is included.

**context_status output:** New `curation_health` field on `StatusReport` containing
`CurationHealthSummary`: per-cycle mean and stddev, trend direction (comparing last-5
mean to prior-5 mean), source breakdown percentages, orphan ratio stats.

---

## Acceptance Criteria

- AC-01: `cycle_review_index` has five new columns (`corrections_total`,
  `corrections_agent`, `corrections_human`, `deprecations_total`, `orphan_deprecations`)
  at schema v24; migration runs idempotently from v23 via `pragma_table_info` pre-check.

- AC-02: `context_cycle_review` computes the `CurationSnapshot` at review time by querying
  ENTRIES for corrections (`supersedes IS NOT NULL, feature_cycle = ?`) and deprecations
  (`status = Deprecated, feature_cycle = ?`), grouped by `trust_source`.

- AC-03: `corrections_agent` counts entries where `trust_source` is any non-human/non-system
  value (specifically `"agent"`) and `supersedes IS NOT NULL`; `corrections_human` counts
  entries where `trust_source` is `"human"` or `"privileged"` and `supersedes IS NOT NULL`.
  (ADR-gated: exact trust_source bucketing rules.)

- AC-04: `orphan_deprecations` counts entries with `status = Deprecated AND superseded_by
  IS NULL` where the deprecation timestamp (from AUDIT_LOG) falls within the current cycle's
  window (cycle start event timestamp → review call timestamp), joined via AUDIT_LOG.

- AC-05: `CurationSnapshot` columns are written to `cycle_review_index` atomically with
  the existing `INSERT OR REPLACE` in `store_cycle_review()`. No separate write.

- AC-06: `context_cycle_review` response includes a `curation_health` block with this
  cycle's raw snapshot counts.

- AC-07: When at least 3 prior cycles have non-NULL curation snapshot data, `context_cycle_review`
  response includes σ position for `corrections_total` and `orphan_deprecations` ratio
  relative to the rolling N-cycle baseline.

- AC-08: When fewer than N prior cycles have snapshot data (cold start), `context_cycle_review`
  includes raw numbers only; no σ comparison is surfaced and no error is returned.

- AC-09: `context_status` includes a `curation_health` block reading from the last N
  `cycle_review_index` rows ordered by `computed_at DESC`.

- AC-10: `context_status` curation health block includes: per-cycle correction rate
  (mean and stddev), source breakdown (agent%, human%), orphan deprecation ratio (mean
  and stddev), and trend direction for correction rate.

- AC-11: `SUMMARY_SCHEMA_VERSION` in `cycle_review_index.rs` is bumped to `2`; stale
  memoized records (schema_version=1) trigger the existing advisory on cache hit.

- AC-12: `context_cycle_review` with `force=false` on a cycle with a stale cached record
  (schema_version=1) returns the advisory alongside the report; it does NOT silently
  recompute (consistent with crt-033 ADR-002 behavior).

- AC-13: All new SQL queries use `read_pool()` for the curation baseline reads in
  `context_status`; the snapshot write in `context_cycle_review` uses `write_pool_server()`
  (consistent with `store_cycle_review` ADR-001).

- AC-14: Migration from v23 → v24 integration test: verify all five columns appear and
  have DEFAULT 0 on pre-existing `cycle_review_index` rows.

- AC-15: Unit tests for `compute_curation_baseline`: empty input returns `None`; fewer
  than 3 entries returns `None`; 3+ entries return correct mean/stddev; zero-stddev
  handled without NaN; zero deprecations_total produces defined orphan ratio (0.0, not
  division-by-zero).

- AC-16: σ anomaly threshold (proposed 1.5σ) is defined as a named constant, not inlined.

---

## Constraints

- **Schema v24 migration** must use `pragma_table_info` pre-check per new column (SQLite
  has no `ADD COLUMN IF NOT EXISTS`). The five-column migration is a single version bump;
  all five columns go in one block for atomicity.

- **write_pool_server() is a single-connection serializer** — the snapshot computation
  (SQL SELECT) must use `read_pool()`; only the INSERT OR REPLACE uses `write_pool_server()`.
  Consistent with existing `store_cycle_review` pattern (ADR-001 crt-033).

- **trust_source bucketing (resolved)**: `"agent"` → `corrections_agent`; `"human"` →
  `corrections_human`; `"system"` and `"direct"` excluded from both (automated infrastructure).
  An optional `corrections_system` informational field may surface the excluded count —
  ADR-gated by the architect.

- **Orphan deprecation attribution uses ENTRIES `updated_at` window**: orphan deprecations
  are attributed to the cycle during which the deprecation *occurred* (not when the entry
  was created). Attribution uses `updated_at` on ENTRIES within `[cycle_start_ts, review_ts]`
  — no AUDIT_LOG join required. Write-path analysis (ADR-003) proves only `context_deprecate`
  produces `superseded_by IS NULL` entries, and `context_deprecate` always sets `updated_at`.
  SQL: `WHERE status = 'deprecated' AND superseded_by IS NULL AND updated_at >= ? AND updated_at <= ?`.

- **SUMMARY_SCHEMA_VERSION bump triggers advisory on all existing memoized records** —
  operators who call `context_cycle_review force=false` on any historical cycle will see
  the advisory until they force-recompute. This is the existing designed behavior (ADR-002
  crt-033), but the scope of impact should be documented in the spec.

- **No changes to `context_correct` or `context_deprecate` write paths** — the feature
  is read-only at the ENTRIES table level; no instrumentation of in-flight MCP calls.

- **Baseline window constant** (`CURATION_BASELINE_WINDOW = 10`) should be in
  `services/status.rs` (consistent with `PENDING_REVIEWS_K_WINDOW_SECS` location).

- **Max 500 lines per file** — if `services/status.rs` approaches the limit with new
  curation logic, extract into `services/curation_health.rs`.

---

## Resolved Decisions

- **OQ-01 → RESOLVED: Exclude `"system"` and `"direct"` from both buckets.** Only `"agent"`
  counts toward `corrections_agent`; only `"human"` (and `"privileged"` if it exists) counts
  toward `corrections_human`. Automated infrastructure writes do not reflect curation behavior.
  Optionally surface a `corrections_system` informational field (total including system/direct)
  so operators can see where all corrections originated — ADR-gated.

- **OQ-02 → RESOLVED: Orphan deprecation attribution uses ENTRIES `updated_at` window.**
  The entry's `feature_cycle` records when it was *created*, not when it was *deprecated*.
  Architect analysis (ADR-003) found no AUDIT_LOG join is needed: only `context_deprecate`
  produces `superseded_by IS NULL` entries, and it always sets `updated_at` on the deprecated
  entry. Attribution: `updated_at IN [cycle_start_ts, review_ts]` on ENTRIES directly.
  For corrections, the correcting entry's `feature_cycle` remains correct (the new entry is
  created during the active cycle). The two mechanisms differ intentionally.

- **OQ-03 → RESOLVED: 1.5σ threshold, flagging both directions.** Matches existing
  `unimatrix_observe::baseline` ADR-003. Flag both unexpectedly low (disengagement) and
  unexpectedly high (systemic bad knowledge ingestion).

- **OQ-04 → RESOLVED: Last-5 vs. prior-5 within the N-cycle window. Two independent
  cold-start thresholds:** σ comparison available at 3 cycles; trend direction available
  at 6 cycles (3+3 required for the split). Trend is absent/`None` until 6 cycles. The
  two thresholds are independent — surfacing σ does not require trend to be available.

- **OQ-05 → RESOLVED: MIN_HISTORY=3 (matching existing baseline), with history length
  annotation.** Output includes the number of cycles in the baseline window alongside the
  σ value — e.g., `"2.1σ (4 cycles of history)"` — so operators can calibrate their trust
  in early readings.

- **OQ-06 → RESOLVED: `force=true` recomputes the aggregated view (mean/stddev/trend) from
  stored `cycle_review_index` snapshots.** The snapshot columns themselves (`corrections_total`,
  etc.) are write-once at review time and are re-computed from ENTRIES on `force=true` for
  the current cycle only. The rolling baseline aggregate (mean/stddev) is always recomputed
  from the current snapshot window on each call.

## Open Questions

None. All scope-level questions resolved above.

---

## Tracking

GH Issue: #529
