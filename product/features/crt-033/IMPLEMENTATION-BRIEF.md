# crt-033: CYCLE_REVIEW_INDEX — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-033/SCOPE.md |
| Architecture | product/features/crt-033/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-033/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-033/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-033/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| cycle_review_index store module | pseudocode/cycle_review_index.md | test-plan/cycle_review_index.md |
| migration v17→v18 | pseudocode/migration.md | test-plan/migration.md |
| tools.rs handler modifications | pseudocode/tools_handler.md | test-plan/tools_handler.md |
| status.rs response modifications | pseudocode/status_response.md | test-plan/status_response.md |
| services/status.rs Phase 7b | pseudocode/status_service.md | test-plan/status_service.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Introduce `CYCLE_REVIEW_INDEX`, a durable SQLite memoization table keyed by `feature_cycle`, so that `context_cycle_review` computes and stores a full `RetrospectiveReport` on first call and returns the stored record verbatim on subsequent calls. The stored record provides the prerequisite gate for GH #409's retention purge pass — raw signals must not be deleted for a cycle until a `cycle_review_index` row exists. `context_status` gains a `pending_cycle_reviews` field listing cycles with raw signals but no stored review, giving operators visibility into retrospective backlog.

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Write path for store_cycle_review | `write_pool_server()` (synchronous, not analytics queue) — row must exist before handler returns so #409 can gate on it; fire-and-forget has a 500ms window where #409 could incorrectly purge | ADR-001 (Unimatrix #3793) | architecture/ADR-001-synchronous-write.md |
| SUMMARY_SCHEMA_VERSION placement | Single `u32` const in `cycle_review_index.rs` only; unified coverage for both detection-rule staleness and JSON structure staleness; no cross-crate coupling; circular dep (observe→store) prevents putting it in observe | ADR-002 (Unimatrix #3794) | architecture/ADR-002-unified-summary-schema-version.md |
| RetrospectiveReport serialization | Direct serde derives — all 23 nested types audited and confirmed `Serialize + Deserialize`; no DTO shim needed; compile-time verification via write/read call sites; deserialization failure falls through to recomputation (not panic) | ADR-003 (Unimatrix #3795) | architecture/ADR-003-direct-serde-no-dto.md |
| pending_cycle_reviews K-window | Uses `query_log.feature_cycle` (not `cycle_events`) scoped to 90-day window; `PENDING_REVIEWS_K_WINDOW_SECS` named constant in `services/status.rs`; must reconcile with #409 at merge time; `read_pool()` for the aggregate query | ADR-004 (Unimatrix #3796) | architecture/ADR-004-pending-reviews-k-window.md |

---

## Files to Create / Modify

### New Files

| File | Summary |
|------|---------|
| `crates/unimatrix-store/src/cycle_review_index.rs` | New store module: `CycleReviewRecord` struct, `SUMMARY_SCHEMA_VERSION = 1`, `get_cycle_review`, `store_cycle_review` (INSERT OR REPLACE via write_pool_server), `pending_cycle_reviews` (K-window set-difference via read_pool) |
| `tests/migration_v17_to_v18.rs` | Migration integration test: build a v17-shaped DB, open with SqlxStore, assert `cycle_review_index` table exists with all five columns; pattern follows `tests/migration_v16_to_v17.rs` |

### Modified Files

| File | Summary |
|------|---------|
| `crates/unimatrix-store/src/migration.rs` | Bump `CURRENT_SCHEMA_VERSION` 17→18; add `if current_version < 18` block with `CREATE TABLE IF NOT EXISTS cycle_review_index` DDL |
| `crates/unimatrix-store/src/db.rs` | Add `cycle_review_index` DDL to `create_tables_if_needed()`; update schema_version INSERT from 17→18 |
| `crates/unimatrix-server/src/mcp/tools.rs` | Add `force: Option<bool>` to `RetrospectiveParams`; insert step 2.5 (memoization check) and step 8a (memoization store) into the `context_cycle_review` handler; extract `check_stored_review` and `build_cycle_review_record` helper functions; add SR-07 discriminator (COUNT on `cycle_events` to distinguish purged vs never-existed on force=true+empty path) |
| `crates/unimatrix-server/src/mcp/response/status.rs` | Add `pending_cycle_reviews: Vec<String>` to `StatusReport`, `StatusReport::default()`, `StatusReportJson`, and `From<&StatusReport>` conversion; summary formatter renders list when non-empty; JSON formatter includes field as array |
| `crates/unimatrix-server/src/services/status.rs` | Add Phase 7b in `compute_report()`: call `store.pending_cycle_reviews(k_window_cutoff)` and populate `report.pending_cycle_reviews`; define `PENDING_REVIEWS_K_WINDOW_SECS` named constant |
| `tests/sqlite_parity.rs` (or `sqlite_parity_specialized.rs`) | Update table-count and named-table assertions to include `cycle_review_index` |
| `crates/unimatrix-server/src/server.rs` | Update `assert_eq!(version, N)` schema version assertions to 18 |
| Previous migration test file | Rename `test_current_schema_version_is_17` → `test_current_schema_version_is_at_least_17` with `>= 17` assertion |

---

## Data Structures

### CycleReviewRecord (new — `cycle_review_index.rs`)

```rust
pub struct CycleReviewRecord {
    pub feature_cycle: String,
    pub schema_version: u32,
    pub computed_at: i64,        // unix timestamp seconds
    pub raw_signals_available: i32,  // 1 = live signals; 0 = signals purged (sqlx INTEGER mapping)
    pub summary_json: String,    // full RetrospectiveReport JSON, no evidence_limit truncation
}
```

Note: `raw_signals_available` is `i32` in Rust (matching SQLite INTEGER) per the architecture's `CycleReviewRecord` definition. The spec's domain model shows `bool`; delivery must confirm the sqlx column binding and use a consistent mapping. The RISK-TEST-STRATEGY flags this as an explicit edge case.

### SUMMARY_SCHEMA_VERSION (new constant — `cycle_review_index.rs`)

```rust
pub const SUMMARY_SCHEMA_VERSION: u32 = 1;
```

Bump policy: increment when any `RetrospectiveReport` field changes JSON round-trip fidelity, OR when any hotspot detection rule in `unimatrix-observe` changes. Never import from `unimatrix-observe`.

### SQLite Table DDL

```sql
CREATE TABLE IF NOT EXISTS cycle_review_index (
    feature_cycle         TEXT    PRIMARY KEY,
    schema_version        INTEGER NOT NULL,
    computed_at           INTEGER NOT NULL,
    raw_signals_available INTEGER NOT NULL DEFAULT 1,
    summary_json          TEXT    NOT NULL
)
```

### RetrospectiveParams (modified — `tools.rs`)

```rust
pub struct RetrospectiveParams {
    pub feature_cycle: String,
    pub agent_id: Option<String>,
    pub evidence_limit: Option<usize>,
    pub format: Option<String>,
    pub force: Option<bool>,   // NEW — fifth field; absent = false
}
```

### StatusReport (modified — `status.rs`)

New field appended after `category_lifecycle`:
```rust
pub pending_cycle_reviews: Vec<String>,
```

---

## Function Signatures

All implemented on `SqlxStore` in `cycle_review_index.rs`:

```rust
pub async fn get_cycle_review(
    &self,
    feature_cycle: &str,
) -> Result<Option<CycleReviewRecord>>

pub async fn store_cycle_review(
    &self,
    record: &CycleReviewRecord,
) -> Result<()>
// Uses write_pool_server(); INSERT OR REPLACE; enforces 4MB ceiling on summary_json

pub async fn pending_cycle_reviews(
    &self,
    k_window_cutoff: i64,  // unix timestamp seconds: now - PENDING_REVIEWS_K_WINDOW_SECS
) -> Result<Vec<String>>
// Uses read_pool(); returns DISTINCT feature_cycle from query_log WHERE feature_cycle NOT IN cycle_review_index
```

Handler helper functions (extracted from `tools.rs` to limit line growth, per NFR-08 / C-10):

```rust
fn check_stored_review(
    record: &CycleReviewRecord,
    current_version: u32,
) -> (RetrospectiveReport, Option<String>)
// Deserializes summary_json; returns advisory string when schema_version differs

fn build_cycle_review_record(
    feature_cycle: &str,
    report: &RetrospectiveReport,
) -> Result<CycleReviewRecord, serde_json::Error>
// Serializes report; enforces 4MB ceiling check is delegated to store_cycle_review
```

---

## Handler Control Flow (crt-033 additions in context)

### First-call path (no stored record, force absent or false)

1. Step 1: identity resolution
2. Step 2: validate params (existing; `force` needs no validation — it is bool)
3. Step 3: three-path observation load → `attributed: Vec<ObservationRecord>`
4. **Step 2.5 (NEW):** `get_cycle_review(feature_cycle)` — None → proceed
5. Step 4: if `attributed.is_empty()` → existing `get_metrics()` / ERROR_NO_OBSERVATION_DATA path (unchanged)
6. Steps 5–8: full computation pipeline (unchanged)
7. **Step 8a (NEW):** serialize report to JSON; call `store_cycle_review(record)` via write_pool_server; `raw_signals_available = 1`
8. Step 9: audit + format dispatch (unchanged); apply `evidence_limit` truncation here

### Memoization hit path (stored record exists, force absent or false)

1. Step 2.5: `get_cycle_review` returns `Some(record)`
2. If `record.schema_version != SUMMARY_SCHEMA_VERSION`: append advisory `"computed with schema_version N, current is M — use force=true to recompute"`
3. Deserialize `record.summary_json` → `RetrospectiveReport` (fallthrough to full pipeline on deserialization error — ADR-003)
4. Apply `evidence_limit` truncation; proceed to format dispatch; skip steps 4–8a entirely

### force=true with live signals

1. Step 2.5 skipped entirely (force=true bypasses the stored-record check)
2. Full pipeline executes
3. Step 8a: `INSERT OR REPLACE` overwrites prior record

### force=true with empty attributed observations (SR-07 discriminator)

1. Three-path load yields empty `attributed`
2. **Discriminator (NEW):** `SELECT COUNT(*) FROM cycle_events WHERE cycle_id = ?`
   - count > 0 → cycle had signals, they are purged:
     - `get_cycle_review` → `Some(record)`: return record with note `"Raw signals have been purged; returning stored record from <computed_at>"`
     - `get_cycle_review` → `None`: return `ERROR_NO_OBSERVATION_DATA`
   - count == 0 → cycle never had cycle_events signals: fall through to existing `observation_metrics` / `ERROR_NO_OBSERVATION_DATA` path

---

## Constraints

| Constraint | Detail |
|-----------|--------|
| Schema cascade | All 7 touchpoints required (architecture is authoritative — supersedes AC-02b's 5-touchpoint list). Gate check: `grep -r 'schema_version.*== 17' crates/` must return zero matches |
| Synchronous write | `store_cycle_review` uses `write_pool_server()` directly in async context — MUST NOT be called from `spawn_blocking` (entries #2266, #2249) |
| evidence_limit | Truncation at render time only (step 9 / format dispatch). MUST NOT be applied before `serde_json::to_string` in step 8a |
| SUMMARY_SCHEMA_VERSION location | Defined once in `cycle_review_index.rs` only. No definition in `tools.rs` or `unimatrix-observe` |
| Stale version advisory | Return stored record + advisory on schema_version mismatch. Silent recompute prohibited |
| pending_cycle_reviews query pool | Uses `read_pool()` only (entry #3619 lesson: write pool causes contention for status aggregates) |
| pending_cycle_reviews scope | K-window bounded (90-day default); `query_log.feature_cycle` as signal source; NULL/empty excluded in SQL |
| pending_cycle_reviews always-on | No opt-in parameter; computed unconditionally in Phase 7b of `compute_report()` |
| serde_json for summary_json | Bincode prohibited; consistent with `domain_metrics_json` (schema v14) |
| 4MB ceiling | `store_cycle_review` must return `Err` (not panic) when `summary_json` exceeds 4MB |
| tools.rs file size | Memoization steps must be extracted into helpers; 500-line-per-function guideline |
| No FK enforcement | `cycle_review_index` has no FOREIGN KEY clause; consistent with all other tables |
| raw_signals_available mapping | sqlx maps INTEGER to `i32`; confirm field type and binding are consistent before merging (RISK-TEST-STRATEGY edge case) |

---

## Dependencies

### Rust Crates (existing — no new additions)

- `sqlx 0.8` (`sqlite`, `runtime-tokio`, `macros`) — `unimatrix-store`
- `serde_json` — `unimatrix-server`, `unimatrix-store`
- `serde` with `Serialize + Deserialize` — `unimatrix-observe` (already on `RetrospectiveReport`)

### Internal Crate Dependencies

- `unimatrix-store`: new `cycle_review_index.rs` module; schema migration; `CycleReviewRecord`
- `unimatrix-server`: `tools.rs` handler, `mcp/response/status.rs`, `services/status.rs`
- `unimatrix-observe`: no code changes; serde audit confirms all 23 types are already `Serialize + Deserialize`

### External / Feature Dependencies

- **GH #409** (intelligence-driven retention): crt-033 is a prerequisite. #409 must not merge before crt-033 ships. When #409 merges, reconcile `PENDING_REVIEWS_K_WINDOW_SECS` with #409's retention window constant.
- **Upstream issue**: GH #451

---

## NOT in Scope

- GH #409 retention/purge pass implementation (DELETE logic for observations, co_access, query_log rows)
- Backfilling pre-existing `observation_metrics` rows into `CYCLE_REVIEW_INDEX`
- Schema version auto-upgrade for stored `summary_json` on `SUMMARY_SCHEMA_VERSION` mismatch
- Changes to hotspot detection rule logic or scoring
- Changes to the `observation_metrics` table or `get_metrics`/`store_metrics` API
- New constant `CURRENT_DETECTION_RULES_VERSION` in `unimatrix-observe`
- `query_log.feature_cycle` column addition (column does not exist; `cycle_events` is the source for `cycle_review_index` presence; `query_log.feature_cycle` is used for the pending query per ADR-004 — if it is consistently NULL, the pending list will be empty, which is documented as a known assumption)
- Changes to `CycleParams` or the `context_cycle` handler
- `context_status maintain=true` gating for `pending_cycle_reviews`

---

## Alignment Status

**Overall**: PASS with 1 WARN. No FAIL or VARIANCE.

| Finding | Status | Action Required |
|---------|--------|----------------|
| Vision alignment | PASS | None |
| Milestone fit (Cortical phase) | PASS | None |
| Architecture consistency (all 4 ADRs present and internally consistent) | PASS | None |
| Risk completeness (13 risks, 39 scenarios, all scope risks traced) | PASS | None |
| `query_log.feature_cycle` → `cycle_events` substitution for cycle presence | WARN | Semantics change: "Unimatrix was queried during cycle" → "a cycle_start event was recorded." A cycle with a `cycle_start` but zero Unimatrix queries will appear as pending. Documented in spec OQ-02 and risk strategy I-01. ADR-004 uses `query_log.feature_cycle` for the pending query (different from the `cycle_events`-based discriminator). Delivery must verify `query_log.feature_cycle` is populated; if consistently NULL, pending list will always be empty. |
| AC-02b lists 5 cascade touchpoints; architecture documents 7 | WARN | Architecture is authoritative for delivery. Use the 7-touchpoint cascade table from ARCHITECTURE.md, not the 5-touchpoint list in AC-02b. Touchpoints 5 (`server.rs` assertions) and 6 (previous migration test rename) are required by the architecture and will be verified at the gate. |
| `raw_signals_available` field type: spec shows `bool`, architecture shows `i32` | Advisory | Delivery must confirm sqlx INTEGER→Rust type binding and use it consistently. The round-trip test (AC-16) will surface any mapping error at runtime. |
| get_cycle_review read failure fallthrough (risk strategy says "graceful miss"; spec FR-01 is silent on read error) | Advisory | Delivery should confirm whether read failure → graceful miss (treat as cache miss, proceed to full pipeline) is intended, and add it to the handler. This aligns with the resilience pattern from ADR-003. |
