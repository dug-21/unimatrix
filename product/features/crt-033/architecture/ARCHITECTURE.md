# crt-033: CYCLE_REVIEW_INDEX — Architecture

## System Overview

`context_cycle_review` is the retrospective pipeline tool in Unimatrix. It computes a full
`RetrospectiveReport` on demand: loading observations, running hotspot detection, producing
metrics, baselines, session summaries, phase stats, and narratives. Before crt-033 it has no
durable output — every call re-runs the pipeline from raw signals. This creates two problems:

1. **Non-idempotency**: results differ between calls as raw signal rows age out or are cleaned.
2. **No purge gate**: GH #409 (retention pass) cannot safely delete raw signals for a cycle
   because there is no record confirming a review was ever completed.

crt-033 introduces `CYCLE_REVIEW_INDEX` — a SQLite table keyed by `feature_cycle` that stores
the serialized `RetrospectiveReport` as JSON. The handler checks this table before computing;
stores into it after computing. `context_status` gains a `pending_cycle_reviews` field that
enumerates cycles with raw signals but no stored review, giving operators visibility into
retrospective backlog.

## Component Breakdown

### 1. `unimatrix-store/src/cycle_review_index.rs` (new module)

Owns all read/write operations for `CYCLE_REVIEW_INDEX`. Responsibilities:

- Define `CycleReviewRecord` struct (mirrors table columns exactly)
- Define `SUMMARY_SCHEMA_VERSION: u32 = 1` constant
- `get_cycle_review(feature_cycle) -> Result<Option<CycleReviewRecord>>`
- `store_cycle_review(record: &CycleReviewRecord) -> Result<()>` — uses `INSERT OR REPLACE`
- `pending_cycle_reviews(k_window_cutoff: i64) -> Result<Vec<String>>` — K-window set difference

The module is separate from `SqlxStore`'s existing `read.rs`/`write.rs`/`analytics.rs`
split because `CYCLE_REVIEW_INDEX` is a keyed archive (memoization store), not entry CRUD or
analytics. The boundary also keeps `write.rs` from growing further.

### 2. `unimatrix-store/src/migration.rs` (modified)

- `CURRENT_SCHEMA_VERSION` bumped from 17 → 18
- New `if current_version < 18` block in `run_main_migrations()` adds the `cycle_review_index`
  table DDL

### 3. `unimatrix-store/src/db.rs` (modified)

- `create_tables_if_needed()` gains the `cycle_review_index` DDL (must mirror migration block
  exactly — fresh-database path)
- Schema version INSERT in `create_tables_if_needed()` updated from 17 → 18

### 4. `unimatrix-server/src/mcp/tools.rs` (modified — handler)

Two new handler steps:

- **Step 2.5** — memoization check: after observation load, before full computation
- **Step 8a** — memoization store: after report assembly, before audit/format dispatch

Also: `RetrospectiveParams` gains `pub force: Option<bool>`.

All memoization logic is extracted into helper functions (`check_stored_review`,
`build_cycle_review_record`) to limit handler line growth.

### 5. `unimatrix-server/src/mcp/response/status.rs` (modified)

- `StatusReport` gains `pub pending_cycle_reviews: Vec<String>`
- `StatusReport::default()` initializes it as `vec![]`
- `StatusReportJson` gains a corresponding field
- `From<&StatusReport>` impl maps the field
- Summary formatter renders the list when non-empty

### 6. `unimatrix-server/src/services/status.rs` (modified)

- Phase 7b in `compute_report()`: call `store.pending_cycle_reviews(k_window_cutoff)` and
  populate `report.pending_cycle_reviews`

## Component Interactions

```
context_cycle_review handler (tools.rs)
    │
    ├── [Step 2.5] get_cycle_review(feature_cycle)
    │       └── cycle_review_index.rs → SELECT from cycle_review_index
    │
    ├── [IF miss] full pipeline (steps 3–8 unchanged)
    │       └── ... compute RetrospectiveReport ...
    │
    ├── [Step 8a] store_cycle_review(record)
    │       └── cycle_review_index.rs → INSERT OR REPLACE into cycle_review_index
    │
    └── [Step 9] audit + format dispatch (unchanged)

context_status handler
    │
    └── services/status.rs Phase 7b
            └── pending_cycle_reviews(k_window_cutoff)
                    └── cycle_review_index.rs → SQL set-difference query
```

## Technology Decisions

See individual ADRs for full rationale.

| Decision | Choice | ADR |
|----------|--------|-----|
| Synchronous write for store_cycle_review | write_pool_server(), not analytics queue | ADR-001 |
| SUMMARY_SCHEMA_VERSION placement | Unified const in cycle_review_index.rs only | ADR-002 |
| RetrospectiveReport serialization | Direct serde derives, no DTO | ADR-003 |
| pending_cycle_reviews K-window scoping | query_log.feature_cycle + 90-day default | ADR-004 |

## Handler Control Flow

### Normal first-call path (no stored record, force absent or false)

```
Step 1: identity resolution
Step 2: validate params
Step 3: three-path observation load → attributed: Vec<ObservationRecord>
Step 2.5: get_cycle_review(feature_cycle)
    → None → proceed to full pipeline
Step 4: if attributed.is_empty():
    → check observation_metrics cache (existing path, unchanged)
    → return cached MetricVector report OR ERROR_NO_OBSERVATION_DATA
Step 5: full computation (list_all_metrics, detect_hotspots, compute_metric_vector,
         store_metrics, 60-day cleanup, baselines, entries_analysis, report build,
         recommendations, narratives, lesson-learned, multi-session steps, phase narrative,
         PhaseStats, goal/cycle_type/is_in_progress)
Step 8a: build CycleReviewRecord {
    feature_cycle,
    schema_version: SUMMARY_SCHEMA_VERSION,
    computed_at: unix_timestamp_secs(),
    raw_signals_available: 1,
    summary_json: serde_json::to_string(&report)?
}
    → store_cycle_review(&record)
Step 9: audit + format dispatch
```

### Memoization hit path (stored record exists, force absent or false)

```
Step 2.5: get_cycle_review(feature_cycle)
    → Some(record)
    → if record.schema_version != SUMMARY_SCHEMA_VERSION:
        append advisory to response:
        "computed with schema_version {record.schema_version}, current is
         {SUMMARY_SCHEMA_VERSION} — use force=true to recompute"
    → deserialize record.summary_json → RetrospectiveReport
    → return immediately (skip steps 4–8a entirely)
Step 9: format dispatch
```

### force=true path, signals available

```
Step 2.5: force=true → SKIP get_cycle_review check entirely
Step 3–8: full pipeline (identical to normal first-call)
Step 8a: store_cycle_review (INSERT OR REPLACE overwrites prior record)
Step 9: audit + format dispatch
```

### force=true path, signals purged (SR-07 discriminator)

```
Step 3: three-path observation load → attributed: empty
Step 2.5: force=true → check is skipped on entry; but attributed is empty:
    → discriminator check:
        query: SELECT COUNT(*) FROM cycle_events WHERE cycle_id = ?
        IF count > 0: cycle had signals; they are purged
            → get_cycle_review(feature_cycle)
            → Some(record): return record with raw_signals_available=false + note
            → None: return ERROR_NO_OBSERVATION_DATA
        IF count == 0: cycle never had cycle_events signals
            → fall through to existing empty-attributed path:
              check observation_metrics cache → return cached or ERROR_NO_OBSERVATION_DATA
```

**SR-07 resolution**: the discriminator uses `cycle_events` row presence to distinguish
"signals were purged" (cycle_events rows exist but observations are empty) from "cycle never
participated in cycle_events" (no rows). This is a targeted COUNT query — cheap (indexed on
`cycle_id`), correct, and non-ambiguous. The existing `observation_metrics` fallback path
for pre-cycle_events cycles is unaffected.

## Integration Points

### Upstream dependencies (reading)

| Component | What crt-033 reads |
|-----------|-------------------|
| `cycle_events` | COUNT query for SR-07 discriminator |
| `query_log` | `feature_cycle` column for K-window pending set |
| `cycle_review_index` | Memoized record on hit path |
| `SqlxStore.write_pool_server()` | Pool for synchronous writes |

### Downstream consumers (writing to)

| Component | What crt-033 writes |
|-----------|-------------------|
| `cycle_review_index` | Full `RetrospectiveReport` JSON |
| `StatusReport.pending_cycle_reviews` | Cycles awaiting review |

### GH #409 gate

`#409` must add a pre-purge check: `SELECT COUNT(*) FROM cycle_review_index WHERE
feature_cycle = ?`. Only proceed with signal deletion when count > 0. The direction is:
#409 reads `cycle_review_index`; crt-033 writes it. No crt-033 code changes are needed for
this gate beyond creating the table and populating it.

## Integration Surface

| Integration Point | Type / Signature | Defined In |
|-------------------|-----------------|------------|
| `CycleReviewRecord` | `pub struct { feature_cycle: String, schema_version: u32, computed_at: i64, raw_signals_available: i32, summary_json: String }` | `unimatrix-store/src/cycle_review_index.rs` |
| `SUMMARY_SCHEMA_VERSION` | `pub const SUMMARY_SCHEMA_VERSION: u32 = 1` | `unimatrix-store/src/cycle_review_index.rs` |
| `get_cycle_review` | `async fn get_cycle_review(&self, feature_cycle: &str) -> Result<Option<CycleReviewRecord>>` | `SqlxStore` impl in `cycle_review_index.rs` |
| `store_cycle_review` | `async fn store_cycle_review(&self, record: &CycleReviewRecord) -> Result<()>` | `SqlxStore` impl in `cycle_review_index.rs` |
| `pending_cycle_reviews` | `async fn pending_cycle_reviews(&self, k_window_cutoff: i64) -> Result<Vec<String>>` | `SqlxStore` impl in `cycle_review_index.rs` |
| `RetrospectiveParams.force` | `pub force: Option<bool>` — new fifth field | `unimatrix-server/src/mcp/tools.rs` |
| `StatusReport.pending_cycle_reviews` | `pub pending_cycle_reviews: Vec<String>` | `unimatrix-server/src/mcp/response/status.rs` |
| `cycle_review_index` table | DDL: `(feature_cycle TEXT PK, schema_version INTEGER, computed_at INTEGER, raw_signals_available INTEGER DEFAULT 1, summary_json TEXT)` | `db.rs` + `migration.rs` |

## Schema Migration Cascade (v17 → v18)

Per entry #3539, all seven touchpoints must be updated:

| # | File | Change |
|---|------|--------|
| 1 | `migration.rs` | `CURRENT_SCHEMA_VERSION = 18`; add `if current_version < 18` block |
| 2 | `db.rs` | Add `cycle_review_index` DDL to `create_tables_if_needed()` |
| 3 | `db.rs` | Update schema_version INSERT from 17 → 18 |
| 4 | `sqlite_parity.rs` | Update `test_schema_version_is_N` and `test_schema_column_count` |
| 5 | `server.rs` | Update `assert_eq!(version, N)` assertions to 18 |
| 6 | Previous migration test | Rename `test_current_schema_version_is_17` → `test_current_schema_version_is_at_least_17` with `>= 17` |
| 7 | Migration test files | Check for any column-count assertions referencing the old count across ALL migration test files |

Gate enforcement: `grep -r 'schema_version.*== 17' crates/` must return zero matches before
marking migration complete (AC cascade grep check from entry #3539).

## Serde Audit Result (SR-01)

All types nested in `RetrospectiveReport` were audited against the source in
`crates/unimatrix-observe/src/types.rs`:

| Type | Serde status |
|------|-------------|
| `RetrospectiveReport` | `#[derive(Serialize, Deserialize)]` |
| `MetricVector` | `#[derive(Serialize, Deserialize)]` |
| `UniversalMetrics` | `#[derive(Serialize, Deserialize)]` |
| `PhaseMetrics` | `#[derive(Serialize, Deserialize)]` |
| `HotspotFinding` | `#[derive(Serialize, Deserialize)]` |
| `HotspotCategory` | `#[derive(Serialize, Deserialize)]` |
| `Severity` | `#[derive(Serialize, Deserialize)]` |
| `EvidenceRecord` | `#[derive(Serialize, Deserialize)]` |
| `BaselineComparison` | `#[derive(Serialize, Deserialize)]` |
| `BaselineStatus` | `#[derive(Serialize, Deserialize)]` |
| `EntryAnalysis` | `#[derive(Serialize, Deserialize)]` |
| `HotspotNarrative` | `#[derive(Serialize, Deserialize)]` |
| `EvidenceCluster` | `#[derive(Serialize, Deserialize)]` |
| `Recommendation` | `#[derive(Serialize, Deserialize)]` |
| `SessionSummary` | `#[derive(Serialize, Deserialize)]` |
| `FeatureKnowledgeReuse` | `#[derive(Serialize, Deserialize)]` |
| `AttributionMetadata` | `#[derive(Serialize, Deserialize)]` |
| `PhaseNarrative` | `#[derive(Serialize, Deserialize)]` |
| `PhaseCategoryComparison` | `#[derive(Serialize, Deserialize)]` |
| `PhaseStats` | `#[derive(Serialize, Deserialize)]` |
| `ToolDistribution` | `#[derive(Serialize, Deserialize)]` |
| `GateResult` | `#[derive(Serialize, Deserialize)]` |
| `EntryRef` | `#[derive(Serialize, Deserialize)]` |

**Conclusion**: no DTO shim is required. Direct serialization of `RetrospectiveReport` is
safe. `serde_json::to_string(&report)` and `serde_json::from_str::<RetrospectiveReport>(...)`
are the correct call sites. See ADR-003.

`existing round-trip tests in `report.rs` (test_backward_compat_deserialization,
test_entries_analysis_roundtrip`) confirm the type survives JSON round-trips including
forward/backward compat with `#[serde(default)]` fields.

## pending_cycle_reviews K-window Default (SR-04)

The K-window cutoff passed to `pending_cycle_reviews()` must be a pinned default rather than
deferred to delivery. Default: **90 days** (7_776_000 seconds). Rationale: this is a
conservative value that covers active feature work without pulling in ancient cycles. It must
be reconciled with GH #409's final retention constant at merge time. The constant is named
`PENDING_REVIEWS_K_WINDOW_DAYS` and lives in `services/status.rs` (not inlined at the call
site). When #409 merges, delivery aligns this constant with #409's purge window.

SQL for `pending_cycle_reviews(k_window_cutoff: i64)`:

```sql
SELECT DISTINCT ql.feature_cycle
FROM query_log ql
WHERE ql.feature_cycle IS NOT NULL
  AND ql.feature_cycle != ''
  AND ql.queried_at >= ?
  AND ql.feature_cycle NOT IN (
      SELECT feature_cycle FROM cycle_review_index
  )
ORDER BY ql.feature_cycle
```

The `k_window_cutoff` parameter is a unix timestamp: `now() - K_WINDOW_SECS`. This excludes
pre-K-window cycles and NULL/empty `feature_cycle` rows in one query.

## Open Questions

None blocking delivery. Advisory items only:

- **OQ-01**: When GH #409 merges, reconcile `PENDING_REVIEWS_K_WINDOW_DAYS` with #409's
  actual retention window constant. If #409 exposes a pub const, import it; otherwise keep
  the local constant and document the coupling in a comment.

- **OQ-02**: `summary_json` has no enforced byte ceiling at write time. The scope accepts
  this for v1 (SR-02 deemed low probability). A monitoring hook (log warn when JSON > 512KB)
  is optional but recommended if large cycles are observed in production.
