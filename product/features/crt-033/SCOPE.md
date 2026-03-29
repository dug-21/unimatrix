# crt-033: CYCLE_REVIEW_INDEX — Memoized Cycle Review Summaries

## Problem Statement

`context_cycle_review` recomputes the full retrospective pipeline on every call for a given
feature cycle. This is expensive (observation load, hotspot detection, baseline comparison,
session queries) and non-idempotent — results can differ between calls as raw signals age
out. GH #409 (intelligence-driven retention) must not purge raw observations for a cycle
until a durable review record exists. Today there is no such record, so #409 has no safe
purge gate. Additionally, `context_status` cannot answer "which cycles have accumulated
signals but no review yet," blocking operational visibility into pending retrospective work.

Two audiences are affected:
- **Agents**: calling `context_cycle_review` twice produces potentially inconsistent results
  (signals may have been cleaned up between calls).
- **#409 retention pass**: has no safe gate to confirm a review was computed before
  purging raw signal tables (observations, co_access rows, query_log rows).

## Goals

1. Introduce `CYCLE_REVIEW_INDEX` — a durable memoization table keyed by `feature_cycle`.
2. On first call to `context_cycle_review` for a cycle: compute the full summary, store it,
   return it.
3. On subsequent calls (idempotent path): return the stored record without recomputation.
4. Add `force: Option<bool>` to `RetrospectiveParams`: when `true`, recompute and overwrite
   the stored record.
5. When `force=true` but raw signals have been purged: return the stored record with an
   explanatory note rather than failing.
6. Add `pending_cycle_reviews: Vec<String>` to `StatusReport`: cycles that have
   `cycle_events` rows (raw signals exist) but no `CYCLE_REVIEW_INDEX` row.
7. Gate GH #409 purge pass: #409 must not delete signals for a cycle until a
   `CYCLE_REVIEW_INDEX` row exists for that cycle.

## Non-Goals

- This feature does NOT implement the #409 retention/purge pass itself — it only provides
  the prerequisite gate.
- `CYCLE_REVIEW_INDEX` does NOT replace `observation_metrics` (`store_metrics`/`get_metrics`)
  — the existing MetricVector cache remains; `CYCLE_REVIEW_INDEX` stores the richer
  `summary_json` (metrics + hotspots full depth + patterns).
- `evidence_limit` truncation is NOT applied at storage time — stored `summary_json` keeps
  full-depth hotspot evidence. Truncation happens at render time only.
- This feature does NOT change any hotspot detection rule logic or scoring.
- This feature does NOT implement schema version auto-upgrade for stored `summary_json`
  (i.e., no re-compute on `schema_version` mismatch in v1). Schema version is stored for
  future use.
- This feature does NOT migrate pre-existing computed `MetricVector` rows from
  `observation_metrics` into `CYCLE_REVIEW_INDEX`.

## Background Research

### Current context_cycle_review implementation

`tools.rs:1258–1913` — the handler is a single large async function with these steps:

1. Identity resolution + validation (`tools.rs:1269–1277`)
2. Three-path observation load (cycle_events-first, sessions.feature_cycle, content-scan)
   (`tools.rs:1280–1339`)
3. If `attributed.is_empty()`: check `observation_metrics` cache via `store.get_metrics()`
   for a cached `MetricVector`; return cached report or `ERROR_NO_OBSERVATION_DATA`
   (`tools.rs:1342–1407`)
4. Full computation path: `list_all_metrics()`, `detect_hotspots()`,
   `compute_metric_vector()`, `store_metrics()` (`tools.rs:1409–1440`)
5. 60-day observation cleanup (`tools.rs:1442–1454`)
6. Baseline, entries_analysis, report build, recommendations, narratives, lesson-learned
   write (`tools.rs:1456–1503`)
7. Multi-session steps: session summaries, reload%, knowledge reuse, rework count,
   attribution metadata, topic_delivery counters (`tools.rs:1505–1684`)
8. Phase narrative (crt-025), PhaseStats, goal/cycle_type/is_in_progress (col-026)
   (`tools.rs:1687–1866`)
9. Audit + format dispatch (`tools.rs:1868–1912`)

The memoization check-first / store-if-missing logic will slot in as a new **step 2.5**
(after observation load, before full computation) and a corresponding **store step**
after report assembly (after step 8, before step 9).

### RetrospectiveParams

`tools.rs:241–252` — current struct has four fields: `feature_cycle`, `agent_id`,
`evidence_limit`, `format`. New `force: Option<bool>` adds as the fifth field.

### Validation

`infra/validation.rs:507–521` — `validate_retrospective_params` checks only `feature_cycle`
length/emptiness. No changes needed unless we add `force` validation (it's a boolean, so
no validation required).

### Schema migration pattern

`migration.rs` — current `CURRENT_SCHEMA_VERSION = 17` (col-028). Each version bump follows
this pattern:
- Add `if current_version < N { ... }` block in `run_main_migrations()`
- Use `CREATE TABLE IF NOT EXISTS` for new tables, `ALTER TABLE ADD COLUMN` (with
  `pragma_table_info` pre-check) for columns
- Update `counters.schema_version` to N at end of the block
- Increment `CURRENT_SCHEMA_VERSION` constant
- Mirror DDL in `create_tables_if_needed()` in `db.rs` (both must be kept in sync)

Entry #3539 (schema version cascade checklist) identifies additional test touchpoints:
column-count structural tests and SQLite parity tests must also be updated.

### OUTCOME_INDEX reference pattern

`db.rs:570–578` — `outcome_index (feature_cycle TEXT, entry_id INTEGER, PK(feature_cycle, entry_id))`.
Written via `INSERT OR IGNORE` (analytics queue path in `analytics.rs:730`). Simple
join-table with no rich content. `CYCLE_REVIEW_INDEX` is richer — it stores `summary_json`
(TEXT) and metadata fields — but follows the same "analytical record, not knowledge entry"
rationale.

### observation_metrics reference pattern

`db.rs:580–609` — `observation_metrics` is the closest structural analog: it is a
`feature_cycle TEXT PRIMARY KEY` table storing computed numeric metrics. `CYCLE_REVIEW_INDEX`
mirrors this PK design but stores JSON rather than typed columns, enabling schema evolution
without column migrations.

### StatusReport struct

`mcp/response/status.rs:11–132` — a large non-`Serialize` struct. New field
`pending_cycle_reviews: Vec<String>` appends after `category_lifecycle`. Both the
`Default` impl (`status.rs:134`) and `StatusReportJson` (`status.rs:805`) must be extended.
The JSON formatter's `From<&StatusReport>` impl (`status.rs:1302`) also requires a new
field. Summary formatter conditionally renders the list when non-empty.

### Status service (pending_cycle_reviews computation)

`services/status.rs:819–824` — Phase 7 of `compute_report()` currently counts
`retrospected_feature_count` via `list_all_metrics()`. The new `pending_cycle_reviews`
query can be added as a Phase 7b: cycles with `cycle_events` rows but no
`cycle_review_index` row.

SQL:
```sql
SELECT DISTINCT cycle_id
FROM cycle_events
WHERE cycle_id NOT IN (SELECT feature_cycle FROM cycle_review_index)
ORDER BY cycle_id
```

### Migration integration tests

`migration_compat.rs` and `infra-001` integration tests cover migration paths. The new
migration (v17→v18) must have a test verifying the table exists post-migration and that
pre-v18 databases upgrade correctly.

### Existing cycle_review tests

`tools.rs:4083–4299` — four tests (`T-CCR-01` through `T-CCR-04`) cover the three-path
fallback logic using a `MockObservationSource`. These tests operate on extracted pure
functions and do not exercise the full handler. New tests for the memoization path will
need a live store or a new mock for the `CYCLE_REVIEW_INDEX` read/write operations.

### summary_json content

The stored JSON represents a `RetrospectiveReport` (full struct from
`unimatrix-observe/src/lib.rs`). Key fields that must survive round-trip: `metrics`
(`MetricVector`), `hotspots` (full `Vec<Hotspot>` including full `evidence` — `evidence_limit`
NOT applied), `recommendations`, `narratives`, `phase_narrative`. Schema version stored
alongside JSON enables future detection of rule staleness.

## Proposed Approach

### New table: `cycle_review_index`

```sql
CREATE TABLE IF NOT EXISTS cycle_review_index (
    feature_cycle         TEXT    PRIMARY KEY,
    schema_version        INTEGER NOT NULL,
    computed_at           INTEGER NOT NULL,
    raw_signals_available INTEGER NOT NULL DEFAULT 1,
    summary_json          TEXT    NOT NULL
)
```

- `feature_cycle`: FK-equivalent to `cycle_events.cycle_id` (no FK enforced, SQLite
  idiom consistent with other tables).
- `schema_version`: stores `CURRENT_DETECTION_RULES_VERSION` (a new constant in
  `unimatrix-observe`) at compute time. Load-bearing for future stale-detection.
- `computed_at`: unix timestamp seconds.
- `raw_signals_available`: `1` when computed from live signals, `0` when `force=true`
  was attempted after signals were purged (stored record returned with note).
- `summary_json`: full `RetrospectiveReport` serialized as JSON (serde_json).

### Migration: v17 → v18

Add `if current_version < 18` block in `run_main_migrations()` and mirror DDL in
`create_tables_if_needed()`. Schema version constant bumped to 18.

### Store-layer API (unimatrix-store)

New module `crates/unimatrix-store/src/cycle_review_index.rs`. `SqlxStore` already does too
much; `CYCLE_REVIEW_INDEX` is a distinct concern (keyed archive ops, not entry CRUD). The
module boundary makes this separation explicit.

Module exports:
- `pub struct CycleReviewRecord` — fields mirroring the table columns, `summary_json: String`
  (serialized/deserialized at handler layer)
- `pub const SUMMARY_SCHEMA_VERSION: u32 = 1;` — single unified version for both detection
  rules and JSON structure; bumped when either changes (no cross-crate coupling)
- `async fn get_cycle_review(&self, feature_cycle: &str) -> Result<Option<CycleReviewRecord>>`
- `async fn store_cycle_review(&self, record: &CycleReviewRecord) -> Result<()>` — uses
  `INSERT OR REPLACE` to support `force=true` overwrite
- `async fn pending_cycle_reviews(&self, k_window_cutoff: i64) -> Result<Vec<String>>` —
  cycles within K-window that have `query_log` rows with `feature_cycle` set but no
  `cycle_review_index` row

### Handler modifications (tools.rs)

**Step 2.5 — check stored record (new):**
After the three-path observation load and before full computation, if `force` is `false`
or `None`: query `get_cycle_review(&feature_cycle)`. If a row exists, deserialize
`summary_json` and return immediately (bypassing all computation steps). If the stored
record's `schema_version` differs from `SUMMARY_SCHEMA_VERSION`, include an advisory in the
response: "computed with schema_version N, current is M — use force=true to recompute." The
caller drives recompute explicitly; the tool never silently recomputes on version mismatch
(that would break idempotency).

**Step 8a — store computed record (new):**
After report assembly (after PhaseStats / goal step, before audit): serialize the full
report to JSON (full hotspot evidence, no truncation) and call `store_cycle_review()`.
Set `raw_signals_available = 1`, `schema_version = SUMMARY_SCHEMA_VERSION`.

**force=true + no raw signals path:**
When `force=true` and attributed observations are empty (step 6 empty check): instead of
proceeding to the `get_metrics()` cached MetricVector path, check `get_cycle_review()`.
If a stored record exists, return it with `raw_signals_available=false` and an appended
explanatory note. If no stored record exists either, return `ERROR_NO_OBSERVATION_DATA`
as before.

**RetrospectiveParams change:**
Add `pub force: Option<bool>` field.

### StatusReport + status service

Add `pub pending_cycle_reviews: Vec<String>` to `StatusReport` (and `Default` impl,
`StatusReportJson`, `From<&StatusReport>` conversion). In `services/status.rs` Phase 7b,
run `pending_cycle_reviews(k_window_cutoff)` and populate the field. The query is scoped
to the K-window (matching #409's purge window) so it stays fast as `query_log` grows.
Always computed — pending reviews are a health signal, not an opt-in diagnostic.
Summary and JSON formatters render the list when non-empty.

The K-window cutoff value is determined by the same constant used by #409's retention
policy (to be coordinated at delivery time; use a config default if #409 is not yet merged).

### Version constant

`SUMMARY_SCHEMA_VERSION: u32 = 1` lives in `cycle_review_index.rs`. Unified: covers both
the detection rules version and the `summary_json` serialization format. No cross-crate
dependency on `unimatrix-observe` for this integer. Bump it when either detection rules
or JSON structure changes.

## Acceptance Criteria

- AC-01: Schema version bumps from 17 to 18. `cycle_review_index` table exists in fresh
  databases created at v18.
- AC-02: Existing databases at v17 or below are migrated to v18 without data loss; the
  `cycle_review_index` table is created by the migration.
- AC-03: First call to `context_cycle_review` for a cycle (no stored record) computes the
  full report and writes a row to `cycle_review_index` with `raw_signals_available=1`.
- AC-04: Second call to `context_cycle_review` for the same cycle (stored record exists,
  `force` absent or `false`) returns the stored record without re-running computation.
- AC-04b: When the stored record's `schema_version` differs from `SUMMARY_SCHEMA_VERSION`,
  the response includes an advisory message: "computed with schema_version N, current is M
  — use force=true to recompute." The tool does NOT silently recompute on version mismatch.
- AC-05: `force=true` on a call where raw signals exist: recomputes and overwrites the
  stored row; returns the freshly computed report.
- AC-06: `force=true` on a call where raw signals have been purged but a stored record
  exists: returns the stored record with `raw_signals_available=false` and an explanatory
  note in the response.
- AC-07: `force=true` on a call where raw signals have been purged AND no stored record
  exists: returns `ERROR_NO_OBSERVATION_DATA` (unchanged behavior).
- AC-08: Stored `summary_json` preserves full hotspot evidence (no `evidence_limit`
  truncation). `evidence_limit` truncation is applied only at render time.
- AC-09: `context_status` response includes `pending_cycle_reviews` — a list of cycle IDs
  that have `query_log` rows (with `feature_cycle` set) within the K-window but no
  `cycle_review_index` row. Scoped to K-window only; pre-cycle_events cycles excluded.
- AC-10: `context_status` `pending_cycle_reviews` returns an empty list when all
  K-window cycles with `query_log` rows also have `cycle_review_index` rows.
- AC-11: `cycle_review_index.schema_version` is populated with `SUMMARY_SCHEMA_VERSION`
  (the unified const in `cycle_review_index.rs`) at write time.
- AC-12: The `force: Option<bool>` field is accepted in `RetrospectiveParams` JSON;
  omitting it is equivalent to `force=false`.
- AC-13: Migration integration test confirms `cycle_review_index` table is created when
  migrating a v17 database to v18.
- AC-14: Unit test for memoization hit path (stored record returned without DB read of
  observations).
- AC-15: Unit test for `force=true` with purged signals (stored record returned with note).
- AC-16: `RetrospectiveReport` (or a dedicated DTO) is fully `Serialize + Deserialize`
  — compile-time verification at delivery. If any field is non-serializable, a dedicated
  serializable DTO must be introduced; no partial serialization.
- AC-17: `SUMMARY_SCHEMA_VERSION` const is defined in `cycle_review_index.rs` (not in
  `unimatrix-observe` or as a hardcoded literal in the handler).

## Constraints

- **Schema version**: current `CURRENT_SCHEMA_VERSION = 17` (`migration.rs:19`). This
  feature increments it to 18. The cascade checklist (entry #3539) must be followed:
  update the constant, add migration block, mirror DDL in `create_tables_if_needed()`,
  update any column-count structural tests and SQLite parity tests.
- **write_pool_server() vs analytics queue**: `cycle_review_index` writes are synchronous
  (blocking the handler return until stored), not fire-and-forget via the analytics queue.
  Rationale: the stored row must exist before the handler returns so that #409 can safely
  gate on its presence.
- **SUMMARY_SCHEMA_VERSION const**: defined in `cycle_review_index.rs` only. No cross-crate
  coupling. Unified: covers both detection rules version and JSON structure version. Bump
  when either changes.
- **Stale-detection behavior**: always return stored record regardless of `schema_version`
  mismatch. Include advisory in response when versions differ. Caller uses `force=true` to
  recompute. Silent recompute on version mismatch is prohibited (breaks idempotency).
- **pending_cycle_reviews scope**: K-window only, keyed on `query_log.feature_cycle`.
  Pre-cycle_events cycles (old `observation_metrics`-based cycles) are excluded — they are
  not subject to #409's purge and including them would flood the list irremedially.
- **pending_cycle_reviews always computed**: the query is a set difference against two small
  K-window-bounded tables; no parameter gate. Pending reviews are a health signal that
  belongs in the always-on status report.
- **summary_json size**: `RetrospectiveReport` with full hotspot evidence can be large for
  complex features. Accepted — this is a memoization table, not a hot-path read. The
  largest observed cycles have ~20 hotspots with ~30 evidence items each; estimated JSON
  size well under 1MB.
- **serde_json for summary_json**: consistent with `domain_metrics_json` (schema v14,
  ADR-006) and `keywords` (TEXT JSON array). Do not use bincode for this column.
- **No FK enforcement**: consistent with all other reference patterns in the schema
  (SQLite FK enforcement is off by default and not enabled in pool config).
- **File size limit**: `tools.rs` is already large. The memoization check (step 2.5) and
  store (step 8a) should be kept minimal in the handler; extract helper functions where
  possible.
- **evidence_limit**: truncation at render time only, never at storage time. This must be
  enforced even if the stored JSON is larger as a result.
- **#409 dependency direction**: crt-033 does NOT implement #409. It only provides the
  gate. The #409 author must add a pre-purge check against `cycle_review_index`.

## Tracking

https://github.com/dug-21/unimatrix/issues/453 — Upstream issue: GH #451.
