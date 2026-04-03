# crt-041 Architecture: Graph Enrichment — S1, S2, S8 Edge Sources

## System Overview

crt-041 adds bulk edge enrichment to the Unimatrix graph layer through three SQL-only
background tick sources. The Personalized PageRank expander (Group 4) requires a dense
GRAPH_EDGES table to surface entries outside the HNSW k=20 candidate set. ASS-038
confirmed that 6/10 UC ground-truth entries are only reachable via PPR — and PPR
needs cross-category edges to traverse. The current graph has ~1,086 active→active
edges. S1, S2, and S8 together are expected to yield ~5,652 new edges at current
corpus size, surpassing the Group 4 viability threshold of 3,000.

All three sources write to the existing `graph_edges` table (schema v19). No new
tables, no schema migration. The `counters` table holds the S8 watermark. All writes
are INSERT OR IGNORE against the UNIQUE(source_id, target_id, relation_type) constraint
— idempotent by construction.

## Component Breakdown

### 1. `graph_enrichment_tick.rs` (new)
**Location:** `crates/unimatrix-server/src/services/graph_enrichment_tick.rs`
**Responsibility:** Three public(crate) async tick functions (S1, S2, S8) and their
private SQL helpers. Module-level constants for S8 watermark key. Follows the
`co_access_promotion_tick.rs` design pattern: infallible, direct write_pool_server(),
no rayon, tracing::info! summary on completion, tracing::warn! on errors.

Functions:
- `pub(crate) async fn run_s1_tick(store: &Store, config: &InferenceConfig) -> ()`
- `pub(crate) async fn run_s2_tick(store: &Store, config: &InferenceConfig) -> ()`
- `pub(crate) async fn run_s8_tick(store: &Store, config: &InferenceConfig) -> ()`
- `const S8_WATERMARK_KEY: &str = "s8_audit_log_watermark"` (module constant)

Test extraction: `graph_enrichment_tick_tests.rs` sibling file if module exceeds 500
lines (500-line workspace rule, AC-24).

### 2. `nli_detection.rs` (modified — crt-040 prerequisite)
**Location:** `crates/unimatrix-server/src/services/nli_detection.rs`
**Responsibility:** Provides the generalized `write_graph_edge` function that S1, S2,
S8 call sites use. ADR-001 from crt-040 mandates this exists as a `pub(crate) async fn`
sibling to `write_nli_edge`, with `write_nli_edge` delegating to it.

If crt-040 did NOT ship `write_graph_edge`, crt-041 delivery must add it as the
first implementation step (SR-04 gate, ADR-001 here).

### 3. `unimatrix-store/src/read.rs` (modified)
**Location:** `crates/unimatrix-store/src/read.rs`
**Responsibility:** Three new named constants following the established `EDGE_SOURCE_NLI`
and `EDGE_SOURCE_CO_ACCESS` pattern (col-029 ADR-001, crt-034 ADR-002):
- `pub const EDGE_SOURCE_S1: &str = "S1";`
- `pub const EDGE_SOURCE_S2: &str = "S2";`
- `pub const EDGE_SOURCE_S8: &str = "S8";`

`GraphCohesionMetrics` already contains `isolated_entry_count` and
`cross_category_edge_count` as of col-029. No new fields needed — the eval gate
metrics are already computed by `compute_graph_cohesion_metrics()`.

### 4. `unimatrix-store/src/lib.rs` (modified)
**Location:** `crates/unimatrix-store/src/lib.rs`
**Responsibility:** Re-export the three new constants from the crate root, following
the existing pattern for `EDGE_SOURCE_NLI` and `EDGE_SOURCE_CO_ACCESS`.

### 5. `infra/config.rs` (modified)
**Location:** `crates/unimatrix-server/src/infra/config.rs`
**Responsibility:** Five new `InferenceConfig` fields with serde default functions,
`Default::default()` struct literal entries, `validate()` range checks, and
`merge_configs()` entries. Dual-site maintenance invariant: both `#[serde(default)]`
backing function AND `impl Default` struct literal must encode identical default
values (ADR-005, SR-07).

New fields:
- `s2_vocabulary: Vec<String>` — domain terms for S2 (default: empty, operator opt-in)
- `max_s1_edges_per_tick: usize` — S1 per-tick cap (default: 200, range: [1, 10000])
- `max_s2_edges_per_tick: usize` — S2 per-tick cap (default: 200, range: [1, 10000])
- `s8_batch_interval_ticks: u32` — S8 run frequency (default: 10, range: [1, 1000])
- `max_s8_pairs_per_batch: usize` — S8 per-batch pair cap (default: 500, range: [1, 10000])

### 6. `background.rs` (modified)
**Location:** `crates/unimatrix-server/src/background.rs`
**Responsibility:** Import and call the three new tick functions in `run_single_tick`
after `run_graph_inference_tick`. Update the tick-ordering invariant comment. S8 gated
by `current_tick % config.s8_batch_interval_ticks == 0`.

## Component Interactions

```
background.rs::run_single_tick
    │
    ├─ (existing) run_co_access_promotion_tick(store, config, tick)
    ├─ (existing) TypedGraphState::rebuild
    ├─ (existing) PhaseFreqTable::rebuild
    ├─ (existing) contradiction_scan (tick_multiple gate)
    ├─ (existing) extraction_tick
    ├─ (existing) run_graph_inference_tick(store, config, ...)   ← structural_graph_tick
    │
    ├─ NEW: run_s1_tick(store, config)                           ← always
    ├─ NEW: run_s2_tick(store, config)                           ← always (no-op if s2_vocabulary empty)
    └─ NEW: run_s8_tick(store, config)                           ← gated: tick % s8_batch_interval_ticks == 0
```

Data flow per tick:
1. S1 reads `entry_tags` and `entries`, writes to `graph_edges` with source='S1'
2. S2 reads `entries` (content+title), writes to `graph_edges` with source='S2'
3. S8 reads `counters` (watermark), reads `audit_log` (search results), validates
   against `entries` (quarantine filter), writes to `graph_edges` with source='S8',
   updates `counters` (watermark)

All reads use `store.write_pool_server()` directly (same pool as writes) — consistent
with the `co_access_promotion_tick` pattern and the constraint that no spawn_blocking
is used.

## Technology Decisions

See individual ADRs:
- ADR-001: Module structure and tick placement
- ADR-002: S2 safe SQL construction (parameterized binding, SR-01)
- ADR-003: S8 watermark strategy
- ADR-004: GraphCohesionMetrics extension scope
- ADR-005: InferenceConfig dual-maintenance guard (SR-07)

**No new dependencies.** All implementation uses sqlx, serde, and tracing — already
in the workspace manifest.

## Integration Points

### crt-040 Prerequisite Gate

**Hard dependency:** `write_graph_edge(store, source_id, target_id, relation_type, weight, created_at, source, metadata) -> bool` must exist in `nli_detection.rs` as `pub(crate) async fn` before any S1/S2/S8 call site is written.

Verification step (delivery agent must run this before writing call sites):

```
grep -n "pub(crate) async fn write_graph_edge" \
    crates/unimatrix-server/src/services/nli_detection.rs
```

If the function is absent, the first delivery wave adds it (with `write_nli_edge`
delegating to it), then proceeds to S1/S2/S8 implementation.

### `unimatrix-store` Constants

S1/S2/S8 call sites import `EDGE_SOURCE_S1`, `EDGE_SOURCE_S2`, `EDGE_SOURCE_S8`
from `unimatrix_store` (re-exported from `lib.rs`). These must be defined before
`graph_enrichment_tick.rs` is compiled.

### Counters Table (S8 Watermark)

S8 uses the existing `counters` read/write helpers in `unimatrix_store::counters`:
- Read: `counters::get(pool, S8_WATERMARK_KEY)` → `Option<i64>`; `None` = start from 0
- Write: `counters::set(pool, S8_WATERMARK_KEY, new_watermark)` → Result

The watermark is updated AFTER all edge writes for the batch (AC-11). If the watermark
write fails, the same batch is re-processed on the next S8 run — INSERT OR IGNORE
handles the duplicate safely.

### Dual-Endpoint Quarantine Guard

Entry #3981 documents a production bug: missing JOIN on the second endpoint of
co_access promotion silently promoted quarantined entries as edge targets. S1, S2,
and S8 SQL must JOIN `entries` on BOTH source_id and target_id with
`status != Quarantined (3)`. This is a hard constraint documented in AC-03, AC-08,
AC-14 and enforced by integration tests.

### GraphCohesionMetrics (Eval Gate)

`GraphCohesionMetrics` in `unimatrix-store::read` already contains:
- `cross_category_edge_count`: non-bootstrap edges where e1.category != e2.category
- `isolated_entry_count`: active entries with 0 non-bootstrap edges (derived as
  `active_entry_count - connected_entry_count` in Rust, per ADR-002 col-029)

No new fields are needed. The eval gate reads these fields via `compute_graph_cohesion_metrics()`
after delivery; their values increase (cross_category) and decrease (isolated) as
S1/S2/S8 edges populate the graph.

## Integration Surface

| Integration Point | Type / Signature | Location |
|-------------------|-----------------|----------|
| `run_s1_tick` | `pub(crate) async fn(store: &Store, config: &InferenceConfig)` | `services/graph_enrichment_tick.rs` |
| `run_s2_tick` | `pub(crate) async fn(store: &Store, config: &InferenceConfig)` | `services/graph_enrichment_tick.rs` |
| `run_s8_tick` | `pub(crate) async fn(store: &Store, config: &InferenceConfig)` | `services/graph_enrichment_tick.rs` |
| `write_graph_edge` | `pub(crate) async fn(store: &Store, source_id: u64, target_id: u64, relation_type: &str, weight: f64, created_at: u64, source: &str, metadata: Option<&str>) -> bool` | `services/nli_detection.rs` (crt-040 prereq) |
| `EDGE_SOURCE_S1` | `pub const &str = "S1"` | `unimatrix_store::read` + re-exported from `unimatrix_store` |
| `EDGE_SOURCE_S2` | `pub const &str = "S2"` | `unimatrix_store::read` + re-exported from `unimatrix_store` |
| `EDGE_SOURCE_S8` | `pub const &str = "S8"` | `unimatrix_store::read` + re-exported from `unimatrix_store` |
| `InferenceConfig::s2_vocabulary` | `Vec<String>`, default `vec![]` | `infra/config.rs` |
| `InferenceConfig::max_s1_edges_per_tick` | `usize`, default `200` | `infra/config.rs` |
| `InferenceConfig::max_s2_edges_per_tick` | `usize`, default `200` | `infra/config.rs` |
| `InferenceConfig::s8_batch_interval_ticks` | `u32`, default `10` | `infra/config.rs` |
| `InferenceConfig::max_s8_pairs_per_batch` | `usize`, default `500` | `infra/config.rs` |
| `counters::get` | `async fn(pool: &SqlitePool, name: &str) -> Result<Option<i64>>` | `unimatrix_store::counters` |
| `counters::set` | `async fn(pool: &SqlitePool, name: &str, value: i64) -> Result<()>` | `unimatrix_store::counters` |
| `S8_WATERMARK_KEY` | `const &str = "s8_audit_log_watermark"` | `services/graph_enrichment_tick.rs` (module-private) |
| `GraphCohesionMetrics::cross_category_edge_count` | `u64` (already exists, no change) | `unimatrix_store::read` |
| `GraphCohesionMetrics::isolated_entry_count` | `u64` (already exists, no change) | `unimatrix_store::read` |

## Tick Ordering (After crt-041)

```
compaction → co_access_promotion → graph-rebuild → PhaseFreqTable::rebuild
  → contradiction_scan (if embed adapter ready && tick_multiple)
  → extraction_tick
  → structural_graph_tick / run_graph_inference_tick (always)
  → run_s1_tick (always)
  → run_s2_tick (always; no-op when s2_vocabulary is empty)
  → run_s8_tick (gated: current_tick % s8_batch_interval_ticks == 0)
```

The graph-rebuild step runs BEFORE S1/S2/S8 write their edges. New edges from this
tick are visible to PPR at the NEXT tick's rebuild. This is the same one-tick delay
already accepted by co_access_promotion_tick (SR-09, SCOPE-RISK-ASSESSMENT).

The eval gate must be run after at least one full tick completes following delivery,
not immediately after server start.

## Open Questions

None. All design decisions are resolved in SCOPE.md §Design Decisions and reflected
in the five ADRs. The only conditional item is the crt-040 prerequisite gate — the
delivery agent verifies the function exists before writing call sites and adds it
if absent.
