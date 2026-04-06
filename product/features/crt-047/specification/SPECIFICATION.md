# SPECIFICATION: crt-047 — Curation Health Metrics

GH Issue: #529

---

## Objective

Unimatrix currently has no visibility into whether the curation process is functioning:
zero corrections over many cycles is indistinguishable from agents ignoring errors versus
finding none. This feature introduces a `CurationSnapshot` struct capturing per-cycle
correction and orphan-deprecation counts, persists those counts as new columns on
`cycle_review_index`, and surfaces aggregate curation health in both
`context_cycle_review` and `context_status` output. A rolling σ baseline trained on
the corpus's own history flags deviation in either direction without domain-specific
thresholds.

---

## Functional Requirements

**FR-01** — `CurationSnapshot` struct is defined with six fields: `corrections_total`
(u32), `corrections_agent` (u32), `corrections_human` (u32),
`corrections_system` (u32, informational/optional — ADR-gated by architect),
`deprecations_total` (u32), `orphan_deprecations` (u32).

**FR-02** — `context_cycle_review` computes a `CurationSnapshot` at review call time
by querying ENTRIES for corrections (`supersedes IS NOT NULL AND feature_cycle = ?`)
and deprecations/orphans (`status = 'deprecated' AND updated_at IN [cycle_start_ts, review_ts]`).
No AUDIT_LOG join. See FR-05 and FR-06 for the authoritative SQL.

**FR-03** — `corrections_total` is the count of ENTRIES rows where
`supersedes IS NOT NULL AND feature_cycle = <current_cycle>`.

**FR-04** — `corrections_agent` counts entries where
`trust_source = 'agent' AND supersedes IS NOT NULL AND feature_cycle = <current_cycle>`.
`corrections_human` counts entries where
`trust_source IN ('human', 'privileged') AND supersedes IS NOT NULL AND feature_cycle = <current_cycle>`.
`corrections_system` counts entries where
`trust_source NOT IN ('agent', 'human', 'privileged') AND supersedes IS NOT NULL AND feature_cycle = <current_cycle>`.
`corrections_total = corrections_agent + corrections_human` — system writes are excluded from the total
and from the σ baseline. `corrections_system` is stored as an informational field only.

**FR-05** — `orphan_deprecations` counts entries using ENTRIES-only query (no AUDIT_LOG join):
```sql
SELECT COUNT(*) FROM entries
WHERE status = 'deprecated'
  AND superseded_by IS NULL
  AND updated_at >= ?1   -- cycle_start_ts
  AND updated_at <= ?2   -- review_ts
```
`cycle_start_ts` is derived from `MIN(timestamp)` for `event_type = 'cycle_start'` in
`cycle_events` for the current `cycle_id`. Attribution uses `updated_at` on ENTRIES —
the only write path that produces `superseded_by IS NULL` entries is explicit
`context_deprecate`, which is also the path that sets `updated_at`. The AUDIT_LOG join
is redundant because `context_correct` chain-deprecations and lesson-learned auto-supersedes
always set `superseded_by IS NOT NULL` and are excluded by the filter.

**FR-06** — `deprecations_total` counts all deprecations in the cycle window via ENTRIES:
```sql
SELECT COUNT(*) FROM entries
WHERE status = 'deprecated'
  AND updated_at >= ?1
  AND updated_at <= ?2
```
This counts both orphan and non-orphan deprecations (chain-deprecations with
`superseded_by IS NOT NULL` are included in the total). No AUDIT_LOG join.

**FR-07** — The `CurationSnapshot` columns are written to `cycle_review_index` atomically
within the existing `INSERT OR REPLACE` in `store_cycle_review()`. No separate write
call is issued.

**FR-08** — `cycle_review_index` gains seven new INTEGER columns at schema v24:
1. `corrections_total INTEGER NOT NULL DEFAULT 0`
2. `corrections_agent INTEGER NOT NULL DEFAULT 0`
3. `corrections_human INTEGER NOT NULL DEFAULT 0`
4. `corrections_system INTEGER NOT NULL DEFAULT 0` (informational; excluded from total and σ)
5. `deprecations_total INTEGER NOT NULL DEFAULT 0`
6. `orphan_deprecations INTEGER NOT NULL DEFAULT 0`
7. `first_computed_at INTEGER NOT NULL DEFAULT 0` (set once on first insert; preserved on overwrite)

**FR-09** — A pure function `compute_curation_baseline(rows: &[CycleReviewRow], n: usize)
-> Option<CurationBaseline>` is implemented in `unimatrix-server/src/services/`
(not `unimatrix-observe`). It returns `None` when fewer than 3 rows have non-NULL
(non-zero-from-DEFAULT) snapshot data. When 3 or more rows are present it returns
`CurationBaseline` containing mean and stddev for `corrections_total` and
`orphan_ratio` (orphan_deprecations / deprecations_total, with 0.0 for zero denominator).

**FR-10** — The baseline window is `CURATION_BASELINE_WINDOW = 10` cycles (named
constant, not inlined). Rows are selected ordered by `first_computed_at DESC` with
rows where `first_computed_at = 0` excluded (`WHERE first_computed_at > 0`).
`computed_at` is mutable on `force=true` recompute and MUST NOT be used as the
ordering key. `feature_cycle` sorts alphabetically by phase prefix and is not
temporally ordered across phases. `first_computed_at` is set once on first insert and
never overwritten, making the baseline window stable across reruns.

**FR-11** — `context_cycle_review` response includes a `curation_health` field on
`RetrospectiveReport`. When fewer than 3 prior cycles have non-NULL snapshot data
(cold start), the field contains raw snapshot counts only — no σ value is emitted
and no error is returned. When 3 or more prior cycles have data, the field also
includes σ position for `corrections_total` and `orphan_ratio` relative to the
rolling N-cycle baseline, annotated with the history length:
e.g., `"2.1σ (4 cycles of history)"`.

**FR-12** — σ anomaly threshold is `CURATION_SIGMA_THRESHOLD = 1.5`, defined as a
named constant. Both directions are flagged (unexpectedly low and unexpectedly high).
The threshold matches the existing `unimatrix_observe::baseline` ADR-003 value.

**FR-13** — `context_status` response includes a `curation_health` field on
`StatusReport`. This field is computed by reading the last N `cycle_review_index` rows
ordered by `first_computed_at DESC` (excluding `first_computed_at = 0` rows)
using `read_pool()`.

**FR-14** — The `context_status` curation health block includes:
- Per-cycle correction rate: mean and stddev of `corrections_total` over the window.
- Source breakdown: `corrections_agent / corrections_total` (%) and
  `corrections_human / corrections_total` (%), both as rounded percentages.
- Orphan ratio: mean and stddev of `orphan_deprecations / deprecations_total`
  (0.0 when denominator is zero).
- Trend direction: available only when at least 6 cycles have snapshot data.
  Computed as the difference between the mean of the last 5 cycles and the mean
  of the prior 5 cycles (positions 6–10 in the ordered window).
  A positive delta indicates increasing correction rate; negative indicates decrease.
  When fewer than 6 cycles exist, the trend field is absent/`None`.

**FR-15** — `SUMMARY_SCHEMA_VERSION` in `cycle_review_index.rs` is bumped from `1`
to `2`. When `context_cycle_review` is called with `force=false` and encounters a
stored record with `schema_version = 1`, it returns the cached report with the
advisory: `"computed with schema_version 1, current is 2 — use force=true to recompute"`.
It does NOT silently recompute.

**FR-16** — `force=true` behavior is defined per the three cases below (see also
Constraints § force=true semantics). The current cycle's snapshot columns are
recomputed from ENTRIES only (no AUDIT_LOG). The rolling baseline aggregate (mean/stddev/trend)
is always recomputed from the current snapshot window on each call — it is never cached.

**FR-17** — Curation baseline reads in `context_status` use `read_pool()`.
The snapshot write in `context_cycle_review` uses `write_pool_server()` within the
existing `store_cycle_review` async context (ADR-001, crt-033).

**FR-18** — When `services/status.rs` approaches the 500-line cap, curation health
logic is extracted to `services/curation_health.rs`. The baseline window constant
`CURATION_BASELINE_WINDOW` is defined in the same file as the function that uses it
(analogous to `PENDING_REVIEWS_K_WINDOW_SECS` in `status.rs`).

---

## Non-Functional Requirements

**NFR-01** — `compute_curation_baseline` must handle NULL/missing rows gracefully.
Pre-v24 rows in `cycle_review_index` have DEFAULT 0 for all five snapshot columns;
the function must not mistake zero-DEFAULT rows for real zero-correction cycles.
Rows with all five snapshot columns equal to zero AND computed on schema_version < 2
are treated as missing data (not included in the `n` count toward the MIN_HISTORY=3
threshold).

**NFR-02** — `compute_curation_baseline` must not produce NaN or +∞. Zero-stddev
(all values identical) is handled without panic. Division by zero in orphan ratio
produces 0.0.

**NFR-03** — Orphan attribution and deprecation counting use ENTRIES-only queries
(no AUDIT_LOG join). Both the `orphan_deprecations` query and the `deprecations_total`
query are single SQL statements with parameterized `updated_at` window bounds (not N+1).
See FR-05 and FR-06 for the authoritative SQL.

**NFR-04** — The curation health phase in `context_status` must not re-run the full
retrospective pipeline. It reads only from `cycle_review_index` snapshot columns.

**NFR-05** — All new SQL queries use parameterized binds (no string interpolation of
user-provided values).

**NFR-06** — `corrections_total`, `corrections_agent`, `corrections_human`,
`deprecations_total`, `orphan_deprecations` columns on `cycle_review_index` are typed
`INTEGER NOT NULL DEFAULT 0`. The Rust struct fields are `i64` (sqlx binding) or
`u32` (domain type) as appropriate.

---

## Acceptance Criteria

**AC-01** — `cycle_review_index` has seven new columns (`corrections_total`,
`corrections_agent`, `corrections_human`, `corrections_system`, `deprecations_total`,
`orphan_deprecations`, `first_computed_at`) at schema v24; migration runs idempotently
from v23 via `pragma_table_info` pre-check; all seven columns appear on pre-existing
rows with value `0` after migration.
*Verification: integration test opens a synthetic v23 database through `Store::open()`
(not the migration function in isolation) and asserts all seven columns present with DEFAULT 0.*

**AC-02** — `context_cycle_review` computes `CurationSnapshot` at review time by
querying ENTRIES for corrections (`supersedes IS NOT NULL AND feature_cycle = ?`) and
ENTRIES for deprecations/orphans (`status = 'deprecated' AND updated_at IN [cycle_start_ts, review_ts]`).
No AUDIT_LOG join.
*Verification: unit test stores entries with and without `supersedes`, with `updated_at`
inside and outside the cycle window, calls `compute_curation_snapshot()`, asserts correct counts.*

**AC-03** — `corrections_agent` counts `trust_source = 'agent'` entries with
`supersedes IS NOT NULL`; `corrections_human` counts `trust_source IN ('human',
'privileged')` entries with `supersedes IS NOT NULL`; `trust_source IN ('system',
'direct')` entries are excluded from both buckets (and optionally counted in
`corrections_system`).
*Verification: unit test with entries spanning all four `trust_source` values asserts
exact bucket counts.*

**AC-04** — `orphan_deprecations` counts entries with `status = 'deprecated' AND
superseded_by IS NULL AND updated_at IN [cycle_start_ts, review_ts]`. ENTRIES-only —
no AUDIT_LOG join. Entries deprecated via `context_correct` always have
`superseded_by IS NOT NULL` and are excluded by the filter regardless of AUDIT_LOG state.
*Verification: unit test stores (a) chain-deprecated entry with `superseded_by` set,
(b) orphan deprecation with `updated_at` in window, (c) orphan with `updated_at` outside
window; asserts only (b) is counted.*

**AC-05** — `CurationSnapshot` columns are written to `cycle_review_index` atomically
with the existing `INSERT OR REPLACE` in `store_cycle_review()`. No separate write
call is issued.
*Verification: code inspection + round-trip test: store review, retrieve row, assert
all five snapshot columns match.*

**AC-06** — `context_cycle_review` response includes a `curation_health` block
containing this cycle's raw snapshot counts in all cases (cold start or not).
*Verification: integration test on a fresh DB (zero history) asserts the field is
present and contains the raw counts.*

**AC-07** — When at least 3 prior cycles have non-NULL curation snapshot data (schema
version = 2), `context_cycle_review` response includes σ position for
`corrections_total` and `orphan_ratio` annotated with the history length:
e.g., `"2.1σ (4 cycles of history)"`.
*Verification: seed 3 prior cycle_review_index rows with snapshot data, call review,
assert σ values and annotation present.*

**AC-08** — When fewer than 3 prior cycles have snapshot data (cold start),
`context_cycle_review` includes raw numbers only; no σ comparison is surfaced and no
error is returned.
*Verification: seed 2 prior cycle_review_index rows with snapshot data, call review,
assert no σ field.*

**AC-09** — `context_status` includes a `curation_health` block reading from the last
N `cycle_review_index` rows ordered by `first_computed_at DESC` (excluding rows where
`first_computed_at = 0`).
*Verification: seed N rows with `first_computed_at > 0`, call `context_status`, assert block present.*

**AC-10** — `context_status` curation health block includes: per-cycle correction rate
(mean and stddev), source breakdown (agent%, human%), orphan deprecation ratio (mean
and stddev), and trend direction for correction rate when at least 6 cycles have data.
Trend is absent/`None` when fewer than 6 cycles are available.
*Verification: separate tests for 5-cycle case (no trend) and 7-cycle case (trend present).*

**AC-11** — `SUMMARY_SCHEMA_VERSION` in `cycle_review_index.rs` is `2`; stale
memoized records (`schema_version = 1`) trigger the advisory on cache hit; the
existing test `test_summary_schema_version_is_one` must be updated to assert `2`.
*Verification: unit test stores a record with `schema_version = 1`, calls
`context_cycle_review force=false`, asserts advisory string in response.*

**AC-12** — `context_cycle_review` with `force=false` on a cycle with a stale cached
record (`schema_version = 1`) returns the advisory alongside the cached report. It does
NOT silently recompute the curation snapshot.
*Verification: same test as AC-11 — assert no recomputation occurred (snapshot columns
remain as originally stored, not refreshed).*

**AC-13** — All new SQL queries reading snapshot data in `context_status` use
`read_pool()`; the snapshot write in `context_cycle_review` uses `write_pool_server()`
within the async context of `store_cycle_review` (not in `spawn_blocking`).
*Verification: code inspection and consistency with existing ADR-001 (crt-033).*

**AC-14** — Migration integration test: a synthetic v23 database opened via
`Store::open()` (not the migration function in isolation) has all seven new columns
(`corrections_total`, `corrections_agent`, `corrections_human`, `corrections_system`,
`deprecations_total`, `orphan_deprecations`, `first_computed_at`) with DEFAULT 0
on pre-existing rows after migration completes.
*Verification: creates in-memory or tempfile DB at schema v23, calls `Store::open()`,
queries `pragma_table_info('cycle_review_index')`, asserts all seven columns present
and `CURRENT_SCHEMA_VERSION = 24`.*

**AC-15** — Unit tests for `compute_curation_baseline`:
(a) empty input returns `None`;
(b) 2 entries (< MIN_HISTORY) returns `None`;
(c) 3 entries returns correct mean/stddev;
(d) zero stddev (all identical) handled without NaN;
(e) zero `deprecations_total` produces `orphan_ratio = 0.0`, not division-by-zero;
(f) rows with all-zero snapshot data from schema_version < 2 are excluded from
   the count toward MIN_HISTORY.
*Verification: direct unit tests on the pure function.*

**AC-16** — σ anomaly threshold is `CURATION_SIGMA_THRESHOLD = 1.5`, defined as a
named constant.
*Verification: code inspection — no inlined `1.5` in σ comparison logic.*

**AC-17** — `deprecations_total` reflects only deprecations whose `updated_at` falls
within `[cycle_start_ts, review_ts]`, not total lifetime deprecations in ENTRIES.
*Verification: unit test with deprecated entries whose `updated_at` falls outside the
cycle window confirms they do not inflate the count.*

**AC-18** — Unattributed orphan deprecations (those where `context_deprecate` was
called outside any active cycle, so `updated_at` falls outside all known cycle windows)
are not counted in any cycle's `orphan_deprecations`. Their exclusion is documented,
not silent.
*Verification: unit test stores a deprecated entry with `superseded_by IS NULL` and
`updated_at` set to a timestamp before any cycle start; calls `compute_curation_snapshot()`
for the current cycle; asserts the entry does not appear in `orphan_deprecations`.*

---

## Domain Models

**CurationSnapshot** — Aggregate curation activity for one feature cycle, computed
at `context_cycle_review` call time. Fields:
- `corrections_total: u32` — count of new entries created via correction
  (`supersedes IS NOT NULL`) attributed to this cycle by the correcting entry's
  `feature_cycle` column.
- `corrections_agent: u32` — subset where `trust_source = 'agent'`.
- `corrections_human: u32` — subset where `trust_source IN ('human', 'privileged')`.
- `corrections_system: u32` (optional/informational) — subset where
  `trust_source IN ('system', 'direct')`; excluded from agent + human buckets.
- `deprecations_total: u32` — count of ENTRIES with `status = 'deprecated'` and
  `updated_at` within the cycle window (both orphan and chain-deprecated).
- `orphan_deprecations: u32` — subset of `deprecations_total` where the deprecated
  entry has `superseded_by IS NULL` (ENTRIES-only, no AUDIT_LOG join).

**CurationBaseline** — Rolling aggregate over the last N cycles. Contains mean and
stddev for `corrections_total` (absolute counts) and `orphan_ratio`
(`orphan_deprecations / deprecations_total`, range [0.0, 1.0]).

**CurationBaselineComparison** — The per-cycle σ position output attached to
`context_cycle_review`. Contains the raw snapshot plus σ distance from
`CurationBaseline`, annotated with the count of cycles contributing to the baseline,
e.g., `"2.1σ (4 cycles of history)"`.

**CurationHealthSummary** — Aggregate view for `context_status`. Contains per-cycle
mean/stddev for correction rate, source breakdown percentages (agent%, human%),
orphan ratio mean/stddev, and optional trend direction.

**Orphan Deprecation** — An entry with `status = 'deprecated' AND superseded_by IS NULL`
whose `updated_at` falls within a cycle window. Only explicit `context_deprecate` produces
orphans — `context_correct` and lesson-learned auto-supersedes always set `superseded_by`
to the new entry's ID. The `superseded_by IS NULL` filter therefore cleanly separates
orphans from chain-deprecations without any AUDIT_LOG join.

**Cycle Window** — The time interval `[cycle_start_ts, review_call_ts]` derived from
`cycle_events` for the current `cycle_id`. `cycle_start_ts` is the `timestamp` of
the `cycle_start` event for the cycle; `review_call_ts` is the unix timestamp at
the moment `context_cycle_review` is invoked.

**Attribution** — Two different mechanisms apply to the two metric families:
- Corrections: attributed by the correcting entry's `feature_cycle` column
  (the new entry is created during the active cycle, so `feature_cycle` is reliable).
- Orphan deprecations: attributed by `updated_at` on ENTRIES within the cycle window
  `[cycle_start_ts, review_ts]`. The deprecated entry's `feature_cycle` records when it
  was *created*, not when it was *deprecated* — `updated_at` is the correct source.
  No AUDIT_LOG join is required because only `context_deprecate` produces orphans, and
  `context_deprecate` always sets `updated_at` on the deprecated entry.

---

## User Workflows

### Workflow 1: Agent calls context_cycle_review at end of delivery cycle

1. Agent calls `context_cycle_review` with the current `feature_cycle`.
2. Server computes `CurationSnapshot` by querying ENTRIES only (corrections via
   `feature_cycle`, deprecations/orphans via `updated_at` window — no AUDIT_LOG join).
3. Server reads up to N prior `cycle_review_index` rows ordered by `first_computed_at DESC`
   (excluding `first_computed_at = 0`).
4. If 3 or more prior rows have snapshot data, σ comparison is computed and annotated.
5. `CurationSnapshot` columns are written atomically with the full review record
   via `INSERT OR REPLACE` into `cycle_review_index`.
6. Response includes `curation_health` block with raw counts, and σ comparison if
   available.
7. If the prior stored record (same cycle, `force=false`) has `schema_version = 1`,
   the cached report is returned with the advisory; no snapshot recomputation occurs.

### Workflow 2: Operator calls context_status to check corpus health

1. Operator calls `context_status`.
2. Status computation includes a new curation health phase that reads the last N
   `cycle_review_index` rows from `read_pool()`.
3. Response includes `curation_health` block with per-cycle rate, source breakdown,
   orphan ratio, and trend (if at least 6 cycles available).

### Workflow 3: Operator force-recomputes a stale cycle review

1. Operator calls `context_cycle_review` with `force=true` for a historical cycle.
2. Server recomputes `CurationSnapshot` from ENTRIES only (no AUDIT_LOG) for that cycle.
3. Server updates `cycle_review_index` with the new snapshot and schema_version = 2.
   `first_computed_at` is preserved (two-step upsert; not reset on overwrite).
4. σ comparison uses the updated snapshot in the rolling window.

---

## Constraints

### SR-01 (resolved): Orphan attribution — ENTRIES-only, no AUDIT_LOG join

Source code analysis proves the AUDIT_LOG join is unnecessary. Write-path analysis:
- `context_correct` always sets `superseded_by` on the deprecated entry → can never
  produce orphans; excluded by `superseded_by IS NULL` filter without AUDIT_LOG.
- Lesson-learned auto-supersedes always set `superseded_by` → same exclusion.
- Only `context_deprecate` can produce `superseded_by IS NULL` entries, and
  `context_deprecate` always sets `updated_at` on the deprecated entry.

Therefore: `status = 'deprecated' AND superseded_by IS NULL AND updated_at IN [cycle_start_ts, review_ts]`
is a complete and correct orphan count. Simpler SQL, no JSON array parsing, no
dependency on AUDIT_LOG schema stability. SR-01 is closed.

### SR-03 (migration paths): All migration paths must be updated

Schema v23 → v24 migration must update all active locations:
1. `crates/unimatrix-store/src/db.rs` — initial schema DDL (fresh-schema path)
2. `crates/unimatrix-store/src/migration.rs` — version-gated migration runner

Each ADD COLUMN must be guarded with a `pragma_table_info` pre-check (SQLite has no
`ADD COLUMN IF NOT EXISTS`). All seven columns are added in a single version block for
atomicity. The integration test for AC-14 must exercise the full `Store::open()` path,
not the migration function in isolation.

### force=true semantics (three cases, SR-05 resolved)

- **Case A — current cycle raw snapshot**: `force=true` recomputes `CurationSnapshot`
  from ENTRIES (no AUDIT_LOG) for the requested cycle and overwrites the stored
  snapshot columns in `cycle_review_index` with `schema_version = 2`.
  `first_computed_at` is preserved via the two-step upsert.
- **Case B — historical cycle raw snapshot**: same code path as Case A. `force=true`
  on any historical cycle recomputes its snapshot from ENTRIES. This is how operators
  upgrade stale schema_version=1 records. `first_computed_at` remains 0 on pre-v24
  rows after migration — this is intentional (no backfill).
- **Case C — rolling baseline aggregate**: the rolling baseline (mean/stddev/trend)
  is always computed fresh from the current snapshot window on every call. It is never
  cached separately. `force=true` affects Case A/B (snapshot columns); the aggregate
  is unaffected in isolation but benefits because the snapshot being recomputed feeds
  into the window.

### SUMMARY_SCHEMA_VERSION blast radius (SR-04)

Bumping `SUMMARY_SCHEMA_VERSION` to `2` surfaces the advisory on every
`cycle_review_index` row written before crt-047 merges (all historical cycles). Any
agent or operator calling `context_cycle_review force=false` on a past cycle will
receive the advisory until they explicitly recompute with `force=true`. This is the
designed behavior per ADR-002 (crt-033). The spec documents it explicitly so operators
can plan: after deploying v24, run `context_cycle_review force=true` for any historical
cycles whose curation metrics matter. There is no batch migration path in scope.

### Unattributed orphan deprecations (SR-08)

A `context_deprecate` call made outside any active cycle (no `cycle_start` event with
an open window at the time of the call) produces an AUDIT_LOG entry whose timestamp
does not fall within any cycle window. These deprecations are silently excluded from
all cycle `orphan_deprecations` counts — they are unattributed. The `context_status`
aggregate view does not surface a separate unattributed orphan count in this feature.
The spec documents this exclusion so operators interpret the count as
"orphan deprecations during known cycle windows only."

### Baseline ordering key (SR-07 resolved)

The rolling baseline window uses `first_computed_at` as the ordering key — not
`computed_at` (mutable on `force=true`) and not `feature_cycle` (alphabetical by phase
prefix, not temporal across phases). `first_computed_at` is set once at first INSERT
and preserved on all subsequent overwrites via the two-step upsert. Rows with
`first_computed_at = 0` (legacy pre-v24 rows with migration DEFAULT) are excluded:
`WHERE first_computed_at > 0 ORDER BY first_computed_at DESC LIMIT N`.

**Edge case (pseudocode note)**: A pre-crt-047 cycle force-recomputed after deployment
will get fresh curation data but retain `first_computed_at = 0` from the migration
DEFAULT, so it remains excluded from the baseline window. This is intentional per the
non-goal of no backfilling. Implementors must not "fix" this by setting
`first_computed_at` on force-recompute of historical rows.

### write_pool_server constraint (ADR-001, crt-033)

`store_cycle_review()` uses `write_pool_server()` in the caller's async context.
It must not be called from `spawn_blocking`. All five new snapshot columns are part
of the same `INSERT OR REPLACE` statement — no additional write connection acquisitions.

### File length cap

`services/status.rs` has a 500-line cap. If the curation health phase pushes it over,
extract to `services/curation_health.rs`. The architect should pre-plan the extraction
boundary rather than reacting mid-delivery.

### Schema version pre-delivery check (SR-02)

Immediately before the delivery pseudocode phase, the SM must run:
`grep CURRENT_SCHEMA_VERSION crates/unimatrix-store/src/migration.rs`
to confirm v24 has not been claimed by a parallel in-flight feature. If claimed,
all version references in design artifacts must be updated before pseudocode begins.

---

## Dependencies

- `crates/unimatrix-store` — `cycle_review_index.rs` (schema version constant,
  `CycleReviewRecord`, `store_cycle_review`, `get_cycle_review`); migration paths in
  `db.rs` and `migration.rs`
- `crates/unimatrix-server/src/services/status.rs` — new curation health phase
  (Phase 7c or equivalent in `compute_report()`)
- `crates/unimatrix-server/src/services/store_correct.rs` — no changes; referenced
  for SR-01 codebase audit
- `crates/unimatrix-server/src/mcp/tools.rs` — `context_cycle_review` handler;
  `context_deprecate` operation string confirmed at line ~901
- `unimatrix_observe::baseline` — `BaselineEntry`, `BaselineStatus`, population stddev
  implementation; referenced for convention alignment but the new baseline function
  lives in `unimatrix-server/src/services/` (server → store dependency direction)
- Existing ENTRIES table columns: `supersedes`, `superseded_by`, `trust_source`,
  `feature_cycle`, `status`
- Existing `audit_log` table columns: `operation`, `timestamp`, `target_ids`
- Existing `cycle_events` table: `cycle_id`, `event_type`, `timestamp`
  (for cycle window derivation)

---

## NOT In Scope

- **Individual entry lifecycle decisions** — identifying and removing dead or
  never-injected entries (#363 / #370).
- **Lambda modification** — curation health is a separate behavioral signal; no
  Lambda dimension additions or removals. Lambda / freshness redesign is #520.
- **Per-topic σ baselines** — baseline computation is corpus-wide only.
- **Intentional curation burst suppression** — no flag to silence σ anomalies during
  deliberate remediation runs.
- **Backfilling historical curation data** — pre-v24 cycles retain NULL (DEFAULT 0)
  snapshot columns; no backfill pipeline.
- **Changes to `context_correct` or `context_deprecate` write paths** — read-only at
  ENTRIES / AUDIT_LOG level.
- **Unattributed orphan count in context_status** — deprecations outside cycle windows
  are excluded and not surfaced separately (documented exclusion, not a bug).
- **Batch force-recompute tooling** — running `force=true` on all historical cycles
  is a manual operator action; no automated batch migration in this feature.
---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — returned 17 entries; top matches were
  crt-033 ADR-002 (SUMMARY_SCHEMA_VERSION policy, entry #3794), cycle_review_index
  column addition pattern (entry #4178), crt-033 ADR-001 (write_pool_server, entry
  #3793), and vnc-003 ADR-003 (deprecation idempotency, entry #93). These confirmed
  the constraint and pattern selections in this specification.
- Key codebase finding (SR-01, ADR-003): Write-path analysis proves the AUDIT_LOG
  join is entirely unnecessary. Only `context_deprecate` can produce `superseded_by IS NULL`
  entries; `context_correct` always sets `superseded_by`. ENTRIES-only query using
  `updated_at` window is complete and correct. Simpler SQL, no JSON array parsing,
  no AUDIT_LOG schema dependency. OQ-SPEC-01 (outcome filter) and OQ-SPEC-02
  (`corrections_system`) are both resolved — no open questions remain.
