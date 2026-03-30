# crt-034: Recurring co_access → GRAPH_EDGES Promotion Tick — Implementation Brief

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/crt-034/SCOPE.md |
| Architecture | product/features/crt-034/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-034/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-034/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-034/ALIGNMENT-REPORT.md |

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| co_access_promotion_tick | pseudocode/co_access_promotion_tick.md | test-plan/co_access_promotion_tick.md |
| config_extension | pseudocode/config_extension.md | test-plan/config_extension.md |
| store_constants | pseudocode/store_constants.md | test-plan/store_constants.md |
| background_tick_insertion | pseudocode/background_tick_insertion.md | test-plan/background_tick_insertion.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Goal

Add a recurring background tick step (`run_co_access_promotion_tick`) that promotes
qualifying `co_access` pairs (count >= 3) into `GRAPH_EDGES` as `CoAccess`-typed edges
and refreshes the weight of already-promoted edges when the normalized weight has drifted
beyond a configurable delta. This closes the gap where new co-access signal accumulated
since schema v13 is permanently invisible to Personalized PageRank (PPR), whose
`TypedRelationGraph` is rebuilt each tick from a `GRAPH_EDGES` table frozen at bootstrap.
With `w_coac` zeroed in crt-032, `GRAPH_EDGES` is now the sole carrier of co-access
signal; without this fix, PPR operates on a static snapshot forever.

---

## Resolved Decisions Table

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| SQL strategy for batch fetch + global MAX normalization | Single-query batch fetch with scalar subquery (`(SELECT MAX(count) FROM co_access WHERE count >= ?1) AS max_count`) embedded in the candidate SELECT; eliminates the separate round-trip for MAX. Per-pair write remains two-step INSERT OR IGNORE + conditional UPDATE; UPSERT rejected due to semantic mismatch with delta guard. | ADR-001 (Unimatrix #3823) | product/features/crt-034/architecture/ADR-001-sql-strategy.md |
| Location of public constants `EDGE_SOURCE_CO_ACCESS` and `CO_ACCESS_GRAPH_MIN_COUNT` | Co-located with `EDGE_SOURCE_NLI` in `unimatrix-store/src/read.rs` at line ~1630, re-exported from `lib.rs`. New `constants.rs` sub-module rejected as over-engineering for two constants. | ADR-002 (Unimatrix #3824) | product/features/crt-034/architecture/ADR-002-constants-location.md |
| Weight delta threshold (0.1) — config field vs. named constant | Module-private constant `CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1` in `co_access_promotion_tick.rs`. Type is `f64` (not `f32`) — sqlx fetches SQLite REAL as `f64`; comparing against `f32` introduces precision noise (`0.1f32` as `f64` = `0.100000001490116...`). Not added to `InferenceConfig`. | ADR-003 (Unimatrix #3825) | product/features/crt-034/architecture/ADR-003-weight-delta-constant.md |
| `max_co_access_promotion_per_tick` placed in `InferenceConfig` | Added to `InferenceConfig` following the exact `max_graph_inference_per_tick` pattern: serde default fn, validate() range [1, 10000], Default impl stanza, merge_configs stanza. Default 200 (vs NLI's 100) because co_access is pure SQL with no ML cost. Separate config section rejected; sharing NLI cap rejected (different cost centers). | ADR-004 (Unimatrix #3826) | product/features/crt-034/architecture/ADR-004-inference-config-field.md |
| Tick insertion point and SR-05 early-tick detectability | Insert with named anchor comment (ORDERING INVARIANT block, SR-06) between orphaned-edge compaction and `TypedGraphState::rebuild()`. SR-05: emit `warn!` when `qualifying_count == 0 AND current_tick < PROMOTION_EARLY_RUN_WARN_TICKS (5)`. Function signature gains `current_tick: u32`. COUNTERS marker approach rejected (one-shot semantics incompatible with recurring tick). | ADR-005 (Unimatrix #3827) | product/features/crt-034/architecture/ADR-005-tick-insertion-and-sr05.md |
| Edge directionality — v1 matches bootstrap (one direction only) | Write `source_id = entry_id_a`, `target_id = entry_id_b` only. Bidirectional edges deferred to follow-up; writing both directions now would produce PPR asymmetry between bootstrapped (one-direction) and newly promoted (two-direction) pairs. Follow-up protocol: write `(entry_id_b, entry_id_a, 'CoAccess')` — distinct under UNIQUE constraint, no collision risk. | ADR-006 (Unimatrix #3828) | product/features/crt-034/architecture/ADR-006-edge-directionality-v1-contract.md |

---

## Files to Create / Modify

| File | Action | Summary |
|------|--------|---------|
| `crates/unimatrix-server/src/services/co_access_promotion_tick.rs` | **Create** | New module implementing `run_co_access_promotion_tick`; <500 lines |
| `crates/unimatrix-server/src/services/mod.rs` | **Modify** | Add `pub(crate) mod co_access_promotion_tick;` |
| `crates/unimatrix-server/src/background.rs` | **Modify** | Insert `run_co_access_promotion_tick` call with anchor comment between orphaned-edge compaction (step 2) and `TypedGraphState::rebuild()` (step 3); add `PROMOTION_EARLY_RUN_WARN_TICKS` constant |
| `crates/unimatrix-server/src/infra/config.rs` | **Modify** | Add `max_co_access_promotion_per_tick: usize` field to `InferenceConfig` with serde default fn, validate() range check, Default impl stanza, merge_configs stanza |
| `crates/unimatrix-store/src/read.rs` | **Modify** | Add `pub const EDGE_SOURCE_CO_ACCESS: &str = "co_access"` and `pub const CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3` immediately below `EDGE_SOURCE_NLI` at line ~1630 |
| `crates/unimatrix-store/src/lib.rs` | **Modify** | Re-export `EDGE_SOURCE_CO_ACCESS` and `CO_ACCESS_GRAPH_MIN_COUNT` in existing `pub use read::{...}` block |

---

## Data Structures

### Tables (no schema change — read and write only)

**`co_access`** (source, read-only for this feature)
```
entry_id_a  INTEGER  -- lower entry ID (entry_id_a < entry_id_b by convention)
entry_id_b  INTEGER  -- higher entry ID
count       INTEGER  -- co-access event count
last_access INTEGER  -- epoch timestamp
```

**`graph_edges`** (sink, insert/update target)
```
id              INTEGER PRIMARY KEY AUTOINCREMENT
source_id       INTEGER NOT NULL   -- co_access.entry_id_a
target_id       INTEGER NOT NULL   -- co_access.entry_id_b
relation_type   TEXT NOT NULL      -- 'CoAccess'
weight          REAL NOT NULL      -- normalized [0.0, 1.0]: count / MAX(count) over all qualifying pairs
created_at      INTEGER NOT NULL   -- epoch timestamp
created_by      TEXT NOT NULL      -- 'tick' (for tick-promoted edges)
source          TEXT NOT NULL      -- EDGE_SOURCE_CO_ACCESS = "co_access"
bootstrap_only  INTEGER NOT NULL   -- 0 (always for tick-promoted edges)
metadata        TEXT DEFAULT NULL  -- unused
UNIQUE(source_id, target_id, relation_type)
```

### Config Field

```rust
// In InferenceConfig
pub max_co_access_promotion_per_tick: usize  // default 200, range [1, 10000]
```

### Row Type for Batch Fetch (Rust)

```rust
struct CoAccessBatchRow {
    entry_id_a: i64,
    entry_id_b: i64,
    count: i64,
    max_count: Option<i64>,  // NULL if table empty after WHERE filter → early return
}
```

---

## Function Signatures

### Primary tick function

```rust
// crates/unimatrix-server/src/services/co_access_promotion_tick.rs
pub(crate) async fn run_co_access_promotion_tick(
    store: &Store,
    config: &InferenceConfig,
    current_tick: u32,
)
```

Infallible (`-> ()`). Write errors logged at `warn!`, tick continues. Emits `tracing::info!`
with inserted/updated counts after the batch. Emits `tracing::warn!` when
`qualifying_count == 0 AND current_tick < PROMOTION_EARLY_RUN_WARN_TICKS`.

### Batch fetch SQL

```sql
SELECT
    entry_id_a,
    entry_id_b,
    count,
    (SELECT MAX(count) FROM co_access WHERE count >= ?1) AS max_count
FROM co_access
WHERE count >= ?1
ORDER BY count DESC
LIMIT ?2
```

Bind: `?1 = CO_ACCESS_GRAPH_MIN_COUNT (3i64)`, `?2 = config.max_co_access_promotion_per_tick`.

### Insert SQL (per pair)

```sql
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
VALUES (?1, ?2, 'CoAccess', ?3, strftime('%s','now'), 'tick', 'co_access', 0)
```

### Weight fetch SQL (on INSERT no-op)

```sql
SELECT weight FROM graph_edges
WHERE source_id = ?1 AND target_id = ?2 AND relation_type = 'CoAccess'
```

### Weight update SQL (on delta exceeded)

```sql
UPDATE graph_edges
SET weight = ?1
WHERE source_id = ?2 AND target_id = ?3 AND relation_type = 'CoAccess'
```

### Config additions

```rust
// In InferenceConfig struct
#[serde(default = "default_max_co_access_promotion_per_tick")]
pub max_co_access_promotion_per_tick: usize,

fn default_max_co_access_promotion_per_tick() -> usize { 200 }
```

### Constants

```rust
// unimatrix-store/src/read.rs (public)
pub const EDGE_SOURCE_CO_ACCESS: &str = "co_access";
pub const CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3;

// co_access_promotion_tick.rs (module-private)
/// Minimum weight change required to UPDATE an existing CoAccess edge.
/// Not operator-configurable: this is a calibrated noise floor, not a domain policy.
/// f64 to match SQLite REAL fetch type — avoids implicit cast precision noise from f32.
const CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1;

// background.rs
const PROMOTION_EARLY_RUN_WARN_TICKS: u32 = 5;
```

---

## Constraints

1. **No new schema migration.** `GRAPH_EDGES` schema is complete since v13. No new columns, no schema version bump.
2. **Direct write pool path only.** All reads and writes use `store.write_pool_server()`. `AnalyticsWrite::GraphEdge` analytics drain is INSERT OR IGNORE only — it cannot express conditional UPDATE semantics and must not be used.
3. **Infallible tick contract.** Function signature is `async fn ... -> ()`. Errors are absorbed with `warn!` logging; none propagate to the tick caller.
4. **No rayon pool.** Co_access promotion is pure SQL with no CPU-bound ML inference.
5. **No COUNTERS marker.** This tick is explicitly recurring; idempotency is structural via `INSERT OR IGNORE` and the delta guard.
6. **No GC in this feature.** Sub-threshold `CoAccess` edges are not removed by this tick; deferred to GH #409.
7. **File size limit.** `co_access_promotion_tick.rs` must stay under 500 lines.
8. **One-directional edges.** v1 writes `source_id = entry_id_a`, `target_id = entry_id_b` only (ADR-006). Reverse edges are a follow-up.
9. **Tick ordering invariant.** Promotion MUST run after `maintenance_tick` + orphaned-edge compaction and BEFORE `TypedGraphState::rebuild()` (ADR-005, SR-06 anchor comment required).
10. **GH #409 blocking dependency.** crt-034 must be merged and deployed before GH #409 ships. Confirm at delivery gate that #409 is not yet merged.

---

## Dependencies

### Internal Crates

| Crate | Usage |
|-------|-------|
| `unimatrix-store` | `co_access` read queries, `graph_edges` write queries, `write_pool_server()`, `CO_ACCESS_GRAPH_MIN_COUNT`, `EDGE_SOURCE_CO_ACCESS` |
| `unimatrix-server` | `InferenceConfig`, `background.rs` tick loop, `services/` module tree |

### Existing Components

| Component | Relationship |
|-----------|-------------|
| `maintenance_tick()` / `cleanup_stale_co_access()` | Must complete before promotion tick runs (tick ordering) |
| Orphaned-edge compaction in `run_single_tick` | Must complete before promotion tick runs |
| `TypedGraphState::rebuild()` | Must run after promotion tick; makes freshly promoted edges visible to PPR |
| `EDGE_SOURCE_NLI` | Pattern source for `EDGE_SOURCE_CO_ACCESS` constant (co-located in `read.rs`) |
| `max_graph_inference_per_tick` in `InferenceConfig` | Exact pattern mirrored by `max_co_access_promotion_per_tick` |
| `run_graph_inference_tick` / `nli_detection_tick.rs` | Structural template for the new module |

### External Dependencies

No new crate dependencies. Uses `sqlx` (already present) for parameterized queries, `tracing` for structured logs.

---

## NOT in Scope

- Changes to the v12→v13 migration SQL or the bootstrap co_access batch
- Changes to `AnalyticsWrite::GraphEdge` or the analytics drain (no new variant)
- Changes to `co_access` write paths or co-access staleness cleanup logic
- Changes to PPR, `TypedGraphState`, or downstream search scoring
- Changes to `w_coac` or the fusion scoring formula (crt-032 already zeroed `w_coac`)
- Removal or modification of `bootstrap_only = 1` edges (crt-023)
- GC of `CoAccess` edges whose source `co_access` pairs have dropped below threshold (deferred to GH #409)
- Bidirectional edge promotion — v1 is one-directional to match the bootstrap
- New MCP tool surface or API changes
- New schema migration or schema version bump

---

## Alignment Status

**Overall: PASS with 1 WARN**

| Check | Status |
|-------|--------|
| Vision alignment | PASS — directly repairs frozen co-access signal; PPR co-access carrier; Wave 1 correctness fix |
| Milestone fit | PASS — Wave 1 / Wave 1A correctness fix, no overreach into Wave 2/3 capabilities |
| Scope gaps | PASS — all SCOPE.md goals, non-goals, constraints, and ACs addressed |
| Architecture consistency | PASS — components, integration surface, tick ordering diagram, write pool justification all consistent |
| Risk completeness | PASS — all 6 scope risks traced; 13 implementation risks with full scenario coverage |
| Scope additions | WARN |

### WARN-01: SR-05 Early-Tick Warn! Mechanism Added Beyond Scope

The architecture and risk strategy define a runtime `warn!` log emitted when
`qualifying_count == 0 AND current_tick < PROMOTION_EARLY_RUN_WARN_TICKS (5)`. This
mechanism was recommended in SCOPE-RISK-ASSESSMENT.md SR-05 (High severity) but was not
explicitly authorized in SCOPE.md. SCOPE.md describes the #409 race as managed at the GH
milestone level; SPEC FR-08 says "no warning" unconditionally for the empty-table no-op
case.

**Resolution**: Architecture and risk strategy accepted. FR-08 wording to be clarified at
delivery gate to read "no warning (except the SR-05 early-tick detection log)." No
implementation change required — implement SR-05 warn as designed in ADR-005.

The function signature gains `current_tick: u32` to support this mechanism (minor
deviation from the two-parameter signature in the architecture's component overview —
the integration surface table in the same document lists the three-parameter form as the
resolved signature).

---

## Known Limitations

**CoAccess edge directionality (v1).** The bootstrap and this tick write one edge per pair:
`source_id = entry_id_a (min-id)`, `target_id = entry_id_b`. PPR's `Direction::Outgoing`
means seeding the lower-ID entry reaches the higher-ID entry, but seeding `entry_id_b`
reaches nothing via CoAccess — half the traversal paths are missing. The follow-up issue
must: (1) write `(entry_id_b, entry_id_a, 'CoAccess')` for new pairs and (2) back-fill
ALL bootstrap-era one-direction pairs (identifiable via `source = 'co_access'`,
`created_by = 'bootstrap'`). Reverse CoAccess edges do not break cycle detection —
cycle detection uses a Supersedes-only temp graph (Pattern #2429). See ADR-006 (#3828).

**Near-threshold pair overhead.** Pairs at exactly count=3 are fetched and checked every
tick. INSERT OR IGNORE no-op + delta guard ensures zero writes in steady state, but the
SQL round-trips occur. Negligible at current table size (~0.34 MB).

**SR-05 early-tick false positives on restart.** `current_tick` resets to 0 on server
restart. Ticks 0–4 post-restart fire the SR-05 `warn!` if `qualifying_count == 0` — this
is a false positive when all pairs were already promoted before the restart. The warn is
still correct when it fires on a genuinely post-#409-pruned table.
