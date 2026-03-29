# Specification: crt-033 — CYCLE_REVIEW_INDEX Memoization for context_cycle_review

## Objective

`context_cycle_review` currently recomputes the full retrospective pipeline on every invocation, producing potentially inconsistent results as raw signals age out between calls. This feature introduces `CYCLE_REVIEW_INDEX`, a durable memoization table keyed by `feature_cycle`, so that a computed review is stored on first call and returned verbatim on subsequent calls. The stored record also serves as the prerequisite gate for GH #409's retention purge pass, which must not delete raw signals for a cycle until a review record exists.

---

## Ubiquitous Language

| Term | Definition |
|------|-----------|
| **Cycle** | A bounded unit of work identified by a `feature_cycle` string (e.g., `col-022`). Maps to `cycle_events.cycle_id`. |
| **Cycle review** | The full retrospective analysis of a cycle: metrics, hotspots, baseline comparison, narratives, session summaries, phase narrative. Produced by `context_cycle_review`. |
| **CycleReviewRecord** | The Rust struct stored in and retrieved from `CYCLE_REVIEW_INDEX`. Contains `feature_cycle`, `schema_version`, `computed_at`, `raw_signals_available`, and `summary_json`. |
| **CYCLE_REVIEW_INDEX** | The SQLite table (`cycle_review_index`) that holds one `CycleReviewRecord` per cycle. PK is `feature_cycle`. |
| **SUMMARY_SCHEMA_VERSION** | A `u32` constant in `cycle_review_index.rs` (initial value: `1`). Unified version covering both the detection-rules version and the `summary_json` serialization format. Bumped when either changes. Not imported from `unimatrix-observe`. |
| **Memoization hit** | A call to `context_cycle_review` where a stored `CYCLE_REVIEW_INDEX` row already exists and `force` is absent or `false`. The stored record is returned without any observation load or computation. |
| **Memoization miss** | A call where no stored row exists (or `force=true`). Full computation proceeds and the result is written to `CYCLE_REVIEW_INDEX`. |
| **Version advisory** | A text message appended to the response when the stored `schema_version` differs from `SUMMARY_SCHEMA_VERSION`: `"computed with schema_version N, current is M — use force=true to recompute."` |
| **Purged signals** | Raw signals (observations, co_access rows, query_log rows) that have been deleted by the retention pass. A cycle with purged signals has `raw_signals_available = 0` in its review record. |
| **Pending cycle review** | A cycle that has `cycle_events` rows or `query_log` rows within the K-window but no `CYCLE_REVIEW_INDEX` row. Reported in `context_status` as `pending_cycle_reviews`. |
| **K-window** | The time horizon used by #409's retention policy. Cycles with `query_log` rows whose `ts` falls within this window are candidates for both pending-review tracking and purge gating. Default fallback: 90 days (see NFR-05). |
| **force=true** | Parameter on `context_cycle_review` that forces recomputation even if a stored record exists. |
| **Raw signals** | Rows in `observations`, `query_log`, and `co_access` that carry per-cycle signal data. Their existence is a precondition for full computation. |
| **evidence_limit** | An existing `RetrospectiveParams` field that limits hotspot evidence items at render time. Never applied at storage time. |
| **write_pool_server** | The synchronous write pool used for structural writes in `unimatrix-store`. Used for `cycle_review_index` writes (not the analytics fire-and-forget queue). |
| **Pre-cycle_events cycle** | A cycle that was retrospected before `cycle_events` tracking existed. Its `observation_metrics` row exists but it has no `cycle_events` or `query_log` rows. Excluded from `pending_cycle_reviews`. |

---

## Domain Models

### CycleReviewRecord (Rust struct in `cycle_review_index.rs`)

```
feature_cycle:        String        — Primary key. Matches cycle_events.cycle_id.
schema_version:       u32           — SUMMARY_SCHEMA_VERSION at compute time.
computed_at:          u64           — Unix timestamp seconds.
raw_signals_available: bool         — true when computed from live signals; false when
                                      force=true was attempted after signals were purged.
summary_json:         String        — Full RetrospectiveReport serialized as JSON.
                                      Full hotspot evidence; no evidence_limit truncation.
```

### CYCLE_REVIEW_INDEX (SQLite table `cycle_review_index`)

```sql
CREATE TABLE IF NOT EXISTS cycle_review_index (
    feature_cycle         TEXT    PRIMARY KEY,
    schema_version        INTEGER NOT NULL,
    computed_at           INTEGER NOT NULL,
    raw_signals_available INTEGER NOT NULL DEFAULT 1,
    summary_json          TEXT    NOT NULL
)
```

- PK: `feature_cycle` (TEXT). One row per cycle.
- `raw_signals_available`: stored as `INTEGER` (1/0); mapped to `bool` in Rust.
- `summary_json`: full-depth JSON; no FK to any other table.
- No FK enforcement (consistent with all other tables; SQLite FK enforcement is off).
- Written via `INSERT OR REPLACE` to support `force=true` overwrite.

### SUMMARY_SCHEMA_VERSION

- Constant: `pub const SUMMARY_SCHEMA_VERSION: u32 = 1;`
- Location: `crates/unimatrix-store/src/cycle_review_index.rs`
- Semantics: unified version for both detection-rules logic and `summary_json` structure.
- Bump policy: increment when ANY of the following changes — detection rule logic in `unimatrix-observe`, `RetrospectiveReport` struct fields, or serialization format.
- No cross-crate coupling: the value is a plain integer; it does not import from `unimatrix-observe`.

### RetrospectiveParams (modified)

Current fields: `feature_cycle`, `agent_id`, `evidence_limit`, `format`.
New field: `pub force: Option<bool>` — fifth field, added last. Absent or `None` is equivalent to `false`.

---

## Functional Requirements

### FR-01: Memoization check (handler step 2.5)

After the three-path observation load and before full computation, the handler MUST query `get_cycle_review(&feature_cycle)` when `force` is absent or `false`. If a row exists, the handler returns the stored record immediately. No observation processing, hotspot detection, or report assembly is performed on a memoization hit.

### FR-02: Version advisory on schema mismatch

When FR-01 produces a hit and the stored `schema_version` differs from `SUMMARY_SCHEMA_VERSION`, the response MUST include the advisory: `"computed with schema_version N, current is M — use force=true to recompute."` The handler MUST NOT silently recompute on a version mismatch. The stored record is returned as-is regardless of the mismatch.

### FR-03: Store computed record (handler step 8a)

After full report assembly (after PhaseStats/goal step, before audit/format dispatch), the handler MUST serialize the full `RetrospectiveReport` to JSON and call `store_cycle_review()`. The written record MUST have `raw_signals_available = true` and `schema_version = SUMMARY_SCHEMA_VERSION`.

### FR-04: force=true with live signals

When `force=true` and attributed observations are non-empty, the handler MUST bypass FR-01 (skip the stored-record lookup), proceed to full computation, and overwrite the existing `CYCLE_REVIEW_INDEX` row via `INSERT OR REPLACE`. The freshly computed report is returned.

### FR-05: force=true with purged signals and stored record

When `force=true` and attributed observations are empty (three-path load yielded nothing), the handler MUST query `get_cycle_review(&feature_cycle)`. If a stored record exists, it MUST be returned with `raw_signals_available = false` and an explanatory note appended to the response: `"Raw signals have been purged; returning stored record from <computed_at>."` The `raw_signals_available` flag in the stored record itself is NOT updated in this case — the flag is an attribute of the stored record, not of the retrieval.

**SR-07 contract**: The handler MUST use the stored record's `raw_signals_available` field, NOT the three-path observation result alone, to distinguish "signals purged" from "cycle never had signals." Specifically: if `get_cycle_review()` returns `Some(record)` with `raw_signals_available = false`, the cycle previously had signals that are now purged. If `get_cycle_review()` returns `Some(record)` with `raw_signals_available = true`, the signals may still exist but the force=true recompute found no attributed observations (possible concurrent purge). In both cases the stored record is returned with the note. See Open Question OQ-01 for the ambiguous-empty case when no stored record exists.

### FR-06: force=true with purged signals and no stored record

When `force=true`, attributed observations are empty, and `get_cycle_review()` returns `None`, the handler MUST return `ERROR_NO_OBSERVATION_DATA`. This is unchanged behavior.

### FR-07: force=false with no stored record and no attributed observations

When `force` is absent or `false`, no stored record exists, and the three-path observation load yields nothing, the handler MUST proceed to the existing `get_metrics()` MetricVector path. If a `MetricVector` cache hit exists, return a cached report (existing behavior). If not, return `ERROR_NO_OBSERVATION_DATA` (existing behavior). CYCLE_REVIEW_INDEX has no role in this path.

### FR-08: evidence_limit truncation at render time only

When returning a report from `CYCLE_REVIEW_INDEX` (FR-01 or FR-05 paths), `evidence_limit` truncation MUST be applied to the deserialized `RetrospectiveReport` before format dispatch — not before storage. Stored `summary_json` MUST contain full-depth hotspot evidence.

### FR-09: pending_cycle_reviews in StatusReport

`StatusReport` MUST include a new field `pending_cycle_reviews: Vec<String>`. The value MUST be computed by `services/status.rs` Phase 7b as the set of cycle IDs that: (a) have at least one `cycle_events` row with a `cycle_start` event type, AND (b) have no row in `cycle_review_index`. Pre-cycle_events cycles (those without any `cycle_events` rows) are excluded even if they have `observation_metrics` rows.

The query is scoped to the K-window (see NFR-05) and is always computed — no opt-in parameter required.

### FR-10: pending_cycle_reviews empty case

When all cycles with `cycle_events` rows in the K-window also have `cycle_review_index` rows, `pending_cycle_reviews` MUST be an empty `Vec<String>`.

### FR-11: StatusReport formatters

The summary formatter MUST render `pending_cycle_reviews` as a list when non-empty, labeled `"Pending cycle reviews"`. The JSON formatter MUST include `pending_cycle_reviews` as an array field in the output. Both must handle the empty-vec case gracefully (no output for summary, empty array for JSON).

### FR-12: SUMMARY_SCHEMA_VERSION const placement

`SUMMARY_SCHEMA_VERSION` MUST be defined in `crates/unimatrix-store/src/cycle_review_index.rs` as a public constant. It MUST NOT be defined in `unimatrix-observe`, `tools.rs`, or anywhere else. The handler imports it from `cycle_review_index.rs`.

### FR-13: RetrospectiveReport serde completeness

`RetrospectiveReport` and all types it transitively contains MUST implement both `Serialize` and `Deserialize`. This MUST be verified at compile time. If any field type lacks a serde impl, a dedicated serializable DTO (`CycleReviewDto`) MUST be introduced in `unimatrix-store` or `unimatrix-server`; no partial serialization is permitted. The existing `#[derive(Serialize, Deserialize)]` on `RetrospectiveReport` (observed in `unimatrix-observe/src/types.rs`) satisfies this requirement if all transitive types are also covered. The architect must audit all field types before committing to direct serialization (SR-01).

### FR-14: Synchronous write path

`store_cycle_review()` MUST use `write_pool_server()`, not the analytics fire-and-forget queue. The stored row MUST exist before the handler returns, so that #409 can gate on its presence.

### FR-15: New store module

All `CYCLE_REVIEW_INDEX` database operations MUST be implemented in a new module `crates/unimatrix-store/src/cycle_review_index.rs`. Operations MUST NOT be added to `db.rs`, `write.rs`, or `read.rs`. The module exports `CycleReviewRecord`, `SUMMARY_SCHEMA_VERSION`, and the three async functions described in the Store-layer API section.

---

## Store-layer API

Defined in `crates/unimatrix-store/src/cycle_review_index.rs`. Implemented on `SqlxStore`.

```
get_cycle_review(feature_cycle: &str) -> Result<Option<CycleReviewRecord>>
  - SELECT from cycle_review_index WHERE feature_cycle = ?
  - Returns None if no row exists.

store_cycle_review(record: &CycleReviewRecord) -> Result<()>
  - INSERT OR REPLACE INTO cycle_review_index
  - Uses write_pool_server().
  - Supports both first-write and force=true overwrite.

pending_cycle_reviews(k_window_cutoff_secs: i64) -> Result<Vec<String>>
  - Returns cycle_ids from cycle_events that have a cycle_start row
    AND whose timestamp >= k_window_cutoff_secs
    AND which have no matching row in cycle_review_index.
  - Ordered by cycle_id.
  - Pre-cycle_events cycles (no cycle_events rows) are excluded by definition.
```

Reference SQL for `pending_cycle_reviews`:

```sql
SELECT DISTINCT ce.cycle_id
FROM cycle_events ce
WHERE ce.event_type = 'cycle_start'
  AND ce.timestamp >= ?1
  AND ce.cycle_id NOT IN (SELECT feature_cycle FROM cycle_review_index)
ORDER BY ce.cycle_id
```

Note: The `cycle_start` event_type filter ensures only cycles that went through `context_cycle start` are included, excluding any cycles that exist only in `observation_metrics` (pre-cycle_events era).

---

## Non-Functional Requirements

### NFR-01: Latency — memoization hit path

On a memoization hit (FR-01), `context_cycle_review` MUST complete the store read and response serialization within 100ms for cycles with `summary_json` up to 1MB. The full computation path (memoization miss) has no additional latency requirement beyond the pre-existing behavior.

### NFR-02: Latency — first-call write

The synchronous `store_cycle_review()` write (FR-14) adds latency to the first-call path. This is accepted. The write MUST complete within 500ms for `summary_json` up to 1MB. The architect SHOULD verify write-pool contention behavior under concurrent first-call scenarios (SR-06).

### NFR-03: summary_json size ceiling

The stored `summary_json` blob MUST NOT exceed 4MB. The store layer MUST return an error (not panic) if the serialized JSON exceeds this ceiling. The handler MUST propagate this error as a tool error, not a server crash. The SCOPE estimates "well under 1MB" for the largest observed cycles; 4MB provides a 4x safety margin.

### NFR-04: Backward compatibility

The v17→v18 migration MUST NOT alter or drop any existing table or column. All pre-existing data MUST be accessible unchanged after migration. Existing `context_cycle_review` callers that omit `force` continue to work (Option<bool> with absent = false).

### NFR-05: K-window default

The K-window cutoff for `pending_cycle_reviews` MUST default to 90 days before the current time when GH #409 has not yet merged a shared constant. This default MUST be defined as a named constant (e.g., `DEFAULT_PENDING_REVIEW_WINDOW_DAYS: u64 = 90`) in `services/status.rs` or a shared config location — not inlined as a magic number. When #409 merges, this constant MUST be reconciled with the #409 retention policy constant. The delivery agent is responsible for this reconciliation at merge time (SR-04).

### NFR-06: Idempotency

Running the v17→v18 migration twice on the same database MUST succeed without error or data loss. `CREATE TABLE IF NOT EXISTS` satisfies this for the new table.

### NFR-07: serde_json for summary_json

`summary_json` MUST be serialized with `serde_json`. Bincode MUST NOT be used for this column. This is consistent with `domain_metrics_json` (schema v14) and `keywords` (TEXT JSON array).

### NFR-08: File size limit

`tools.rs` is already large. The handler additions for steps 2.5 and 8a MUST be extracted into helper functions where possible to stay within the 500-line-per-file guideline established in `rust-workspace.md`. The new `cycle_review_index.rs` module handles all store ops; the handler delegates to it.

---

## Acceptance Criteria

Each criterion carries a verification method. The AC-IDs from SCOPE.md are preserved; additional criteria addressing scope risks are appended.

### Schema & Migration

**AC-01** — Schema version is 18 in fresh databases.
Verification: `test_current_schema_version_is_18` unit test asserts `CURRENT_SCHEMA_VERSION == 18`. `test_fresh_db_creates_schema_v18` asserts `schema_version` counter = 18 after opening a new database. [SR-05]

**AC-02** — v17 databases migrate to v18 without data loss; `cycle_review_index` table is created.
Verification: `test_v17_to_v18_migration_creates_table` integration test in `tests/migration_v17_to_v18.rs` builds a v17-shaped database, opens it with `SqlxStore`, queries `SELECT name FROM sqlite_master WHERE name='cycle_review_index'`, and asserts the table exists. All pre-existing rows in other tables are readable. [SR-05]

**AC-02b** — All five schema cascade touchpoints are updated.
Verification: Code review gate. The five touchpoints are:
1. `migration.rs`: `CURRENT_SCHEMA_VERSION` constant updated to `18`.
2. `migration.rs`: `if current_version < 18 { ... }` block added in `run_main_migrations()` with `CREATE TABLE IF NOT EXISTS cycle_review_index`.
3. `db.rs`: `create_tables_if_needed()` DDL includes `cycle_review_index` table.
4. `tests/sqlite_parity.rs` or `tests/sqlite_parity_specialized.rs`: table-count or named-table assertion updated to include `cycle_review_index`.
5. Any column-count structural tests in `db.rs` that enumerate tables or assert total table count are updated.
Failure to update all five touchpoints is a gate-blocking defect per entry #3539. [SR-05]

**AC-13** — Migration integration test confirms `cycle_review_index` table exists after migrating a v17 database to v18.
Verification: See AC-02. The `tests/migration_v17_to_v18.rs` file covers this. Pattern follows `tests/migration_v16_to_v17.rs`.

### Core Memoization

**AC-03** — First call for a cycle computes the full report and writes a row with `raw_signals_available = 1`.
Verification: Integration test opens a store with seeded observations for a cycle, calls the handler once, queries `SELECT raw_signals_available FROM cycle_review_index WHERE feature_cycle = ?`, asserts value = 1.

**AC-04** — Second call with `force` absent or `false` returns the stored record without re-running computation.
Verification: Unit/integration test calls handler twice for the same cycle; asserts the second call does NOT execute the observation-load path (mock or spy on store reads) and returns the identical `feature_cycle` and `metrics`.

**AC-04b** — When stored `schema_version` differs from `SUMMARY_SCHEMA_VERSION`, response includes advisory and does NOT silently recompute.
Verification: Integration test inserts a `cycle_review_index` row with `schema_version = 0` (mismatched), calls handler with `force=false`, asserts response text contains the advisory string `"use force=true to recompute"` and that no observation-load DB reads occurred.

**AC-05** — `force=true` with live signals recomputes and overwrites the stored row.
Verification: Integration test writes an initial row, then calls handler with `force=true` and slightly different signals; asserts `computed_at` in the stored row is updated (greater than initial) and returned report reflects fresh computation.

**AC-06** — `force=true` with purged signals but existing stored record returns stored record with note.
Verification: Integration test inserts a `cycle_review_index` row directly (no live observations), calls handler with `force=true`; asserts response contains the explanatory note and the returned `feature_cycle` matches. `raw_signals_available` in the response is `false`. [SR-07]

**AC-07** — `force=true` with purged signals and no stored record returns `ERROR_NO_OBSERVATION_DATA`.
Verification: Integration test with empty observations and no `cycle_review_index` row, calls handler with `force=true`; asserts MCP error code matches `ERROR_NO_OBSERVATION_DATA`.

**AC-08** — Stored `summary_json` preserves full hotspot evidence; `evidence_limit` truncation is applied only at render time.
Verification: Integration test stores a cycle review with 10 hotspots each having 5 evidence items. Calls handler with `evidence_limit=2`. Reads raw `summary_json` from `cycle_review_index` table, deserializes, and asserts each hotspot has 5 evidence items. Asserts returned MCP response hotspots each have 2 evidence items.

**AC-11** — `cycle_review_index.schema_version` is populated with `SUMMARY_SCHEMA_VERSION` at write time.
Verification: AC-03 test also asserts `schema_version = 1` in the written row.

**AC-12** — `force: Option<bool>` field accepted in `RetrospectiveParams` JSON; absent is equivalent to `false`.
Verification: Unit test deserializes JSON `{"feature_cycle": "x"}` into `RetrospectiveParams` and asserts `force.is_none()`. Also deserializes `{"feature_cycle": "x", "force": true}` and asserts `force == Some(true)`.

**AC-17** — `SUMMARY_SCHEMA_VERSION` const is defined in `cycle_review_index.rs`.
Verification: `grep -r 'SUMMARY_SCHEMA_VERSION' crates/` in CI asserts the single definition is in `cycle_review_index.rs`. No occurrences in `tools.rs` as a numeric literal or in `unimatrix-observe`.

### StatusReport

**AC-09** — `context_status` response includes `pending_cycle_reviews` listing K-window cycles with `cycle_events` rows but no `cycle_review_index` row.
Verification: Integration test seeds two cycles in `cycle_events` (both within K-window); writes a `cycle_review_index` row for one. Calls `context_status`. Asserts `pending_cycle_reviews` contains exactly the un-reviewed cycle ID.

**AC-10** — `pending_cycle_reviews` returns empty list when all K-window cycles have review rows.
Verification: Same setup as AC-09 but write a review row for both cycles. Assert `pending_cycle_reviews` is empty.

### Testing

**AC-14** — Unit test for memoization hit path (stored record returned without DB read of observations).
Verification: Test in handler unit test suite with a mock or patched store. First: insert `cycle_review_index` row. Second: call handler with `force=false`. Assert observation tables are not queried (no calls to `get_observations` or equivalent).

**AC-15** — Unit test for `force=true` with purged signals (stored record returned with note).
Verification: Test seeds `cycle_review_index` row but no observations. Calls handler with `force=true`. Assert: (a) return is `Ok`, not `ERROR_NO_OBSERVATION_DATA`; (b) response text contains explanatory note; (c) `raw_signals_available` is reported as false. [SR-07]

### Serde

**AC-16** — `RetrospectiveReport` (or dedicated DTO) is fully `Serialize + Deserialize` — compile-time verification.
Verification: The `cargo build --workspace` step in CI is the compile-time gate. If `RetrospectiveReport` has a non-serializable field, the `serde_json::to_string(&report)?` call in step 8a will fail to compile. Additionally: a unit test MUST perform a round-trip assertion — `serde_json::from_str::<RetrospectiveReport>(&serde_json::to_string(&sample_report).unwrap()).unwrap()` — on a fully-populated `RetrospectiveReport` instance. This catches runtime deserialization gaps in `#[serde(default)]` fields not caught by compilation. [SR-01]

---

## User Workflows

### Workflow 1: Agent calls context_cycle_review for the first time for a cycle

1. Agent sends `context_cycle_review` with `feature_cycle = "col-022"` (no `force`).
2. Handler loads observations via three-path logic.
3. If observations found: handler checks `CYCLE_REVIEW_INDEX` — no row exists.
4. Handler runs full computation pipeline (steps 3–8 of existing flow).
5. Handler serializes `RetrospectiveReport` to JSON and writes `CycleReviewRecord` with `raw_signals_available = 1`.
6. Handler applies `evidence_limit` at render time and returns report.

### Workflow 2: Agent calls context_cycle_review again for the same cycle

1. Agent sends `context_cycle_review` with `feature_cycle = "col-022"` (no `force`).
2. Handler loads observations (three-path) — signals may or may not still exist.
3. Handler checks `CYCLE_REVIEW_INDEX` — row exists; `schema_version` matches.
4. Handler deserializes `summary_json`, applies `evidence_limit` truncation, returns immediately.
5. No computation, no hotspot detection, no baseline comparison.

### Workflow 3: Agent forces recomputation

1. Agent sends `context_cycle_review` with `feature_cycle = "col-022"` and `force = true`.
2. Handler loads observations — signals exist.
3. Handler skips CYCLE_REVIEW_INDEX read (force=true).
4. Handler runs full computation pipeline.
5. Handler overwrites existing `cycle_review_index` row via `INSERT OR REPLACE`.
6. Returns fresh report.

### Workflow 4: Agent queries status to see pending reviews

1. Agent sends `context_status`.
2. `services/status.rs` Phase 7b runs `pending_cycle_reviews(k_window_cutoff)`.
3. Query returns cycles with `cycle_events` rows (cycle_start) but no `cycle_review_index` rows, within K-window.
4. `StatusReport.pending_cycle_reviews` is populated.
5. Summary formatter renders list under `"Pending cycle reviews"`.

### Workflow 5: GH #409 retention pass gates on review existence

1. #409 retention pass iterates cycles eligible for purge.
2. For each cycle: checks `SELECT 1 FROM cycle_review_index WHERE feature_cycle = ?`.
3. If row exists: proceeds with purge (writes `raw_signals_available = 0` is NOT done by #409 — that is set when `force=true` is attempted post-purge and a stored record is returned).
4. If row absent: skips purge for this cycle. The cycle appears in `pending_cycle_reviews` until a review is computed.

---

## Constraints

### C-01: Schema cascade — all five touchpoints required

The v17→v18 migration MUST update all five locations (see AC-02b). Historical gate failures have resulted from missing touchpoints (entry #3539). The architect MUST reference the cascade checklist and delivery MUST verify all five before opening a PR.

### C-02: Synchronous write — not analytics queue

`store_cycle_review()` uses `write_pool_server()`. The analytics fire-and-forget queue MUST NOT be used. Rationale: the stored row must exist before the handler returns so #409 can gate on it.

### C-03: evidence_limit — render time only

`evidence_limit` truncation is applied only when building the MCP response. It MUST NOT be applied before calling `serde_json::to_string` in step 8a.

### C-04: SUMMARY_SCHEMA_VERSION — no cross-crate coupling

`SUMMARY_SCHEMA_VERSION` lives in `cycle_review_index.rs` only. It is a plain `u32` literal. No import from `unimatrix-observe` or any other crate.

### C-05: Stale-version advisory — no silent recompute

When a stored record's `schema_version` != `SUMMARY_SCHEMA_VERSION`, the handler MUST return the stored record plus advisory. Silent recompute is prohibited.

### C-06: pending_cycle_reviews — K-window scope and pre-cycle_events exclusion

The query MUST be bounded by the K-window cutoff (default: 90 days, see NFR-05). Pre-cycle_events cycles (those without `cycle_events` rows) are excluded. The `event_type = 'cycle_start'` filter achieves this exclusion.

### C-07: pending_cycle_reviews — always computed

No `opt_in` or `check_pending` parameter. The query is a set difference against two small K-window-bounded tables and runs unconditionally as Phase 7b of `compute_report()`.

### C-08: serde_json — no bincode

Consistent with `domain_metrics_json` (schema v14, ADR-006) and `keywords` (TEXT JSON array in schema v12). Bincode MUST NOT be used for `summary_json`.

### C-09: No FK enforcement

Consistent with all existing reference patterns. No `FOREIGN KEY` clause on `cycle_review_index`. SQLite FK enforcement remains off.

### C-10: tools.rs file size

The handler additions MUST be kept minimal. Helper functions for the memoization check, version advisory, and purged-signals response MUST be extracted to keep `tools.rs` within the 500-line guideline.

### C-11: K-window default is a named constant

`DEFAULT_PENDING_REVIEW_WINDOW_DAYS: u64 = 90` (or equivalent) MUST be a named constant, not a magic number. Must be reconciled with #409 at merge time.

### C-12: #409 dependency direction

crt-033 provides the gate. It does NOT implement the purge pass. The #409 author is responsible for adding the pre-purge check.

---

## Dependencies

### Rust Crates

- `sqlx 0.8` with `sqlite`, `runtime-tokio`, `macros` features — existing dependency in `unimatrix-store`.
- `serde_json` — existing dependency; used for `summary_json` serialization.
- `serde` with `Serialize` + `Deserialize` — existing dependency in `unimatrix-observe` for `RetrospectiveReport`.

### Internal Crate Dependencies

- `unimatrix-store`: new module `cycle_review_index.rs`; schema v17→v18 migration; `CycleReviewRecord` struct.
- `unimatrix-server`: handler `tools.rs` (steps 2.5, 8a, force paths); `mcp/response/status.rs` (`StatusReport`, `StatusReportJson`, `From<&StatusReport>`); `services/status.rs` (Phase 7b).
- `unimatrix-observe`: `RetrospectiveReport` must be fully serde-capable (audit required per SR-01; no code changes if already satisfying; DTO introduced in `unimatrix-server` or `unimatrix-store` if not).

### External Dependencies

- **GH #409** (intelligence-driven retention): crt-033 is a prerequisite for #409. #409 must not be merged before crt-033 provides `cycle_review_index`. The #409 author must read `cycle_review_index.rs` to understand the gate contract.

---

## NOT in Scope

- **GH #409 retention/purge pass implementation** — crt-033 only provides the gate (CYCLE_REVIEW_INDEX row existence). The actual DELETE logic for observations, co_access rows, and query_log rows is #409's responsibility.
- **Migrating pre-existing `observation_metrics` rows** into `CYCLE_REVIEW_INDEX` — existing `MetricVector` cache rows are not backfilled.
- **Schema version auto-upgrade for stored `summary_json`** — if `SUMMARY_SCHEMA_VERSION` bumps after rows are written, old rows are returned as-is with the version advisory. No re-compute on mismatch.
- **Hotspot detection rule changes** — no rule logic or scoring changes.
- **`observation_metrics` table changes** — the existing `MetricVector` cache (`get_metrics`/`store_metrics`) is unchanged. CYCLE_REVIEW_INDEX stores the richer full report; both coexist.
- **Schema version `CURRENT_DETECTION_RULES_VERSION` in `unimatrix-observe`** — SCOPE mentions this but the resolution is that `SUMMARY_SCHEMA_VERSION` in `cycle_review_index.rs` serves as the unified version; no new constant is added to `unimatrix-observe`.
- **`query_log.feature_cycle` column** — SCOPE references `query_log.feature_cycle` for the pending_cycle_reviews query, but the current `query_log` schema (as observed) does not have this column. The specification adopts `cycle_events.cycle_id` with `event_type = 'cycle_start'` as the source of truth for pending reviews (see FR-09). See Open Question OQ-02.
- **context_cycle tool changes** — no changes to `CycleParams` or the `context_cycle` handler.
- **context_status `maintain=true` path** — pending_cycle_reviews is always computed; it does not require `maintain=true`.

---

## Open Questions

**OQ-01 (SR-07 — ambiguous empty attributed observations, no stored record):**
When `force=true` and the three-path observation load returns empty, and `get_cycle_review()` returns `None`, FR-06 prescribes `ERROR_NO_OBSERVATION_DATA`. However, the empty result could mean "signals were purged without a review ever being written" (a pathological case) versus "this cycle never had signals." The handler cannot distinguish these without additional metadata. This specification accepts the `ERROR_NO_OBSERVATION_DATA` response for both sub-cases when no stored record exists. The architect should confirm this is acceptable or add a discriminator (e.g., check `cycle_events` for any row for this cycle_id).

**OQ-02 (query_log.feature_cycle column):**
SCOPE references `query_log.feature_cycle` for the `pending_cycle_reviews` query. Inspection of the current `query_log` schema (`query_log.rs` and migration tests) shows no `feature_cycle` column — the table has: `query_id`, `session_id`, `query_text`, `ts`, `result_count`, `result_entry_ids`, `similarity_scores`, `retrieval_mode`, `source`, `phase`. The specification substitutes `cycle_events` (with `event_type = 'cycle_start'`) as the primary source for pending cycle identification. The architect must confirm whether `query_log.feature_cycle` was an aspirational column that does not yet exist, or whether it exists via `sessions.feature_cycle` join. If a `query_log.feature_cycle` column is intended, it requires a separate schema migration not covered by crt-033.

**OQ-03 (concurrent first-call contention):**
If two requests for the same cycle arrive concurrently and both find no stored record, both will run full computation and both will attempt `INSERT OR REPLACE`. `INSERT OR REPLACE` is safe (last writer wins), but the wasted computation is accepted. The architect should document this as a known race with acceptable duplicate-compute outcome, not a data corruption risk.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 20 entries. Most relevant: entry #3001 (ADR-004 phase narrative on RetrospectiveReport), entry #3619 (lesson: write_pool_server vs analytics queue decision from col-029), entry #723 (lesson: spec/architecture inconsistency from crt-013 design). Applied: write_pool_server constraint confirmed by entry #3619; spec/arch consistency discipline noted from entry #723.
