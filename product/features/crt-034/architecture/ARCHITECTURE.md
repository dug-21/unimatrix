# crt-034: Recurring co_access → GRAPH_EDGES Promotion Tick — Architecture

## System Overview

Unimatrix's PPR-based retrieval reads a `TypedRelationGraph` rebuilt each tick from
`GRAPH_EDGES`. Since schema v13, `CoAccess` edges were seeded by a one-shot bootstrap
migration; no recurring mechanism promotes new co-access pairs. crt-034 closes this gap
with a new background tick step that keeps the PPR graph synchronized with ongoing
co-access signal.

The fix is pure behavior change: no schema migration, no new table, no changes to PPR,
search scoring, or co-access write paths.

## Component Breakdown

### 1. `services/co_access_promotion_tick.rs` (new module)

Responsibility: recurring promotion of qualifying `co_access` pairs into `GRAPH_EDGES`
as `CoAccess`-typed edges, including weight refresh on existing edges that have drifted.

Public surface: one async function.

```rust
pub(crate) async fn run_co_access_promotion_tick(
    store: &Store,
    config: &InferenceConfig,
)
```

Internal phases:
- Phase 1: fetch global `MAX(count)` via subquery-embedded batch fetch (one SQL round-trip,
  see ADR-001)
- Phase 2: for each pair in the capped batch, attempt `INSERT OR IGNORE`; if no-op, check
  weight delta and `UPDATE` if `|new - existing| > CO_ACCESS_WEIGHT_UPDATE_DELTA`
- Phase 3: emit `tracing::info!` with inserted and updated counts

The function is infallible (`async fn ... -> ()`). Write errors are logged at `warn!` and
the tick proceeds. No rayon pool — this is pure SQL, no ML inference.

### 2. `unimatrix-store` — new public constants

Two new `pub const` values added alongside `EDGE_SOURCE_NLI` in `read.rs` and a
public constant added to `migration.rs` (or a new `constants.rs` sub-module, see ADR-002):

| Constant | Value | Purpose |
|----------|-------|---------|
| `EDGE_SOURCE_CO_ACCESS` | `"co_access"` | Identifies co_access-origin edges in GRAPH_EDGES |
| `CO_ACCESS_GRAPH_MIN_COUNT` | `3i64` | Promotion threshold; matches migration bootstrap threshold |
| `CO_ACCESS_WEIGHT_UPDATE_DELTA` | `0.1f32` | Churn-suppression guard for weight updates |

`CO_ACCESS_WEIGHT_UPDATE_DELTA` is a module-private constant in
`co_access_promotion_tick.rs` — not an operator-tunable config field (see ADR-003).

### 3. `infra/config.rs` — `InferenceConfig` extension

One new field added to `InferenceConfig`:

```rust
/// Maximum co_access pairs to promote per background tick.
///
/// Controls how many qualifying pairs (count >= CO_ACCESS_GRAPH_MIN_COUNT) are
/// processed per tick. Highest-count pairs are selected first (ORDER BY count DESC).
/// Default: 200. Valid range: [1, 10000].
#[serde(default = "default_max_co_access_promotion_per_tick")]
pub max_co_access_promotion_per_tick: usize,
```

Supporting additions (see ADR-004):
- Private serde default fn: `fn default_max_co_access_promotion_per_tick() -> usize { 200 }`
- `validate()`: range check `[1, 10000]` with `ConfigError::NliFieldOutOfRange`
- `Default` impl: field added to `InferenceConfig { ... }` block
- `merge_configs()`: project-overrides-global pattern matching `max_graph_inference_per_tick`

### 4. `background.rs` — tick insertion

`run_co_access_promotion_tick` is called unconditionally (no `nli_enabled` guard) between
orphaned-edge compaction (step 2) and `TypedGraphState::rebuild()` (step 3). See ADR-005
for the anchor comment convention.

`run_single_tick` receives no new parameters — `store` and `inference_config` are already
threaded through.

## Component Interactions

```
background.rs / run_single_tick
  │
  ├─ (step 1) maintenance_tick()         ← calls cleanup_stale_co_access()
  │
  ├─ (step 2) GRAPH_EDGES orphaned-edge compaction
  │
  ├─ (step 2b) run_co_access_promotion_tick(store, inference_config)   ← NEW
  │     │
  │     ├─ SQL: SELECT ..., (SELECT MAX(count) ...) AS max_count
  │     │       FROM co_access WHERE count >= ? ORDER BY count DESC LIMIT ?
  │     │
  │     └─ per pair:
  │           INSERT OR IGNORE INTO graph_edges ...
  │           if no-op: SELECT weight + UPDATE if delta > 0.1
  │
  └─ (step 3) TypedGraphState::rebuild()  ← sees freshly promoted edges
```

## Technology Decisions

- **SQL strategy**: single-query batch fetch with subquery MAX normalization (ADR-001)
- **Constants location**: `EDGE_SOURCE_CO_ACCESS` and `CO_ACCESS_GRAPH_MIN_COUNT` in
  `unimatrix-store/src/read.rs` alongside `EDGE_SOURCE_NLI`; re-exported via `lib.rs`
  (ADR-002)
- **Weight delta**: module-private constant, not config field (ADR-003)
- **InferenceConfig field**: serde default fn + validate() range + merge_configs (ADR-004)
- **Tick insertion point anchor**: named comment in `background.rs` (ADR-005)

## Integration Points

### Reads from

| Source | Access path | Purpose |
|--------|-------------|---------|
| `co_access` table | `store.write_pool_server()` (read-consistent with write sequence) | Fetch qualifying pairs + global MAX |
| `GRAPH_EDGES` table | `store.write_pool_server()` | Check existing weight before UPDATE |
| `InferenceConfig.max_co_access_promotion_per_tick` | via `config` parameter | LIMIT cap |
| `CO_ACCESS_GRAPH_MIN_COUNT` | constant | WHERE threshold |

### Writes to

| Destination | Write path | Semantics |
|-------------|------------|----------|
| `GRAPH_EDGES` (new edges) | `sqlx::query(...).execute(store.write_pool_server())` | `INSERT OR IGNORE` |
| `GRAPH_EDGES` (existing edges) | `sqlx::query(...).execute(store.write_pool_server())` | `UPDATE SET weight = ?` |

### Why `write_pool_server()` and not the analytics drain

`AnalyticsWrite::GraphEdge` is INSERT OR IGNORE only — it has no UPDATE variant. The
promotion tick requires conditional UPDATE semantics for weight refresh. Per the W1-2
NLI confirmed-edge write contract (entry #3821), co_access promotion shares this
constraint and must use the direct write pool path.

### Downstream consumers

`TypedGraphState::rebuild()` in the same tick cycle reads `GRAPH_EDGES` after promotion.
Freshly promoted `CoAccess` edges are immediately available to PPR in the same tick.

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `run_co_access_promotion_tick` | `async fn(store: &Store, config: &InferenceConfig)` | `services/co_access_promotion_tick.rs` |
| `EDGE_SOURCE_CO_ACCESS` | `pub const &str = "co_access"` | `unimatrix-store/src/read.rs` |
| `CO_ACCESS_GRAPH_MIN_COUNT` | `pub const i64 = 3` | `unimatrix-store/src/read.rs` |
| `CO_ACCESS_WEIGHT_UPDATE_DELTA` | `const f32 = 0.1` (module-private) | `services/co_access_promotion_tick.rs` |
| `InferenceConfig::max_co_access_promotion_per_tick` | `usize`, default `200`, range `[1, 10000]` | `infra/config.rs` |
| Promotion SQL batch query | See ADR-001 for exact shape | `services/co_access_promotion_tick.rs` |
| `services/mod.rs` registration | `pub(crate) mod co_access_promotion_tick;` | `services/mod.rs` |

## Known Limitation: Edge Directionality

The bootstrap migration wrote one edge per co_access pair: `source_id = entry_id_a`
(lower numeric ID), `target_id = entry_id_b` (higher numeric ID). PPR traverses
`Direction::Outgoing`, so from the lower-ID entry it can reach the higher-ID entry but
not the reverse.

v1 replicates this behavior exactly for consistency with bootstrapped edges. Writing
reverse edges is correct but is a follow-up issue. The `UNIQUE(source_id, target_id,
relation_type)` constraint means a future reverse-edge pass can safely write
`(entry_id_b, entry_id_a, 'CoAccess')` without colliding with v1 edges.

## SR-05: First-Run Signal-Loss Detectability

SR-05 identifies a high-severity risk: if GH #409 ships before crt-034 and prunes
co_access rows, qualifying pairs are permanently lost without any error or warning.

Mitigation: the promotion tick MUST log at `warn!` if the qualifying-pair query returns
zero rows AND it is one of the first N ticks (e.g., current_tick < 5). This surfaces
silent signal loss. The exact first-tick detection mechanism is described in ADR-005.

An alternative considered (but rejected) was a COUNTERS marker tracking whether any
promotion has run. This was rejected because it adds a one-shot flavor to a recurring
tick and requires a schema read per tick (see ADR-005 for full reasoning).

## Open Questions

1. **GH #409 sequencing confirmation**: Has GH #409 been merged? If yes, assess whether
   qualifying pairs still exist in `co_access`. The risk (SR-05) is silent signal loss —
   zero qualifying rows on first run would indicate the race was lost.

2. **Reverse-edge follow-up issue number**: The known-limitation follow-up for writing
   both directions should be filed as a GH issue. Architecture should be referenced in
   that issue to ensure `(entry_id_b, entry_id_a, 'CoAccess')` is recognized as the
   correct shape.
