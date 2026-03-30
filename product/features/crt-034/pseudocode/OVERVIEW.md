# crt-034: Pseudocode Overview — Recurring co_access → GRAPH_EDGES Promotion Tick

## Components Involved

| Component | File(s) Affected | Action |
|-----------|-----------------|--------|
| `store_constants` | `crates/unimatrix-store/src/read.rs`, `lib.rs` | Add 2 public constants |
| `config_extension` | `crates/unimatrix-server/src/infra/config.rs` | Add 1 field + serde default fn + validate + Default + merge_configs |
| `co_access_promotion_tick` | `crates/unimatrix-server/src/services/co_access_promotion_tick.rs` (create), `services/mod.rs` (modify) | New tick module |
| `background_tick_insertion` | `crates/unimatrix-server/src/background.rs` | Wire call site + PROMOTION_EARLY_RUN_WARN_TICKS constant |

## Sequencing Constraints (Build Order)

```
Wave 1:  store_constants        (unimatrix-store — no intra-feature deps)
Wave 2:  config_extension       (unimatrix-server infra — depends on nothing in this feature)
Wave 3:  co_access_promotion_tick  (depends on Wave 1 constants + Wave 2 config field)
Wave 4:  background_tick_insertion (depends on Wave 3 fn signature)
```

Waves 1 and 2 can be implemented in parallel. Wave 3 requires Wave 1 constants to be importable and Wave 2 `InferenceConfig` field to be present. Wave 4 requires Wave 3's function signature to be fixed.

## Data Flow Between Components

```
co_access table (SQLite)
  │
  │  SELECT entry_id_a, entry_id_b, count,
  │         (SELECT MAX(count) FROM co_access WHERE count >= CO_ACCESS_GRAPH_MIN_COUNT) AS max_count
  │  FROM co_access WHERE count >= CO_ACCESS_GRAPH_MIN_COUNT
  │  ORDER BY count DESC LIMIT max_co_access_promotion_per_tick
  │
  ▼
co_access_promotion_tick.rs
  │  per-pair two-step:
  │    1. INSERT OR IGNORE INTO graph_edges (source_id, target_id, relation_type='CoAccess',
  │       weight=count/max_count, created_at, created_by='tick',
  │       source=EDGE_SOURCE_CO_ACCESS, bootstrap_only=0)
  │    2. if rows_affected==0: SELECT weight FROM graph_edges WHERE ...
  │       if |new_weight - existing_weight| > CO_ACCESS_WEIGHT_UPDATE_DELTA:
  │         UPDATE graph_edges SET weight=new_weight WHERE ...
  │
  ▼
graph_edges table (SQLite)
  │
  ▼
TypedGraphState::rebuild()  (reads all graph_edges, builds PPR graph — same tick cycle)
```

## Shared Types Introduced or Modified

### New public constants in `unimatrix-store`

```
pub const EDGE_SOURCE_CO_ACCESS: &str = "co_access";
    -- Location: crates/unimatrix-store/src/read.rs, immediately after EDGE_SOURCE_NLI (~line 1630)
    -- Re-exported: crates/unimatrix-store/src/lib.rs, in existing pub use read::{...} block

pub const CO_ACCESS_GRAPH_MIN_COUNT: i64 = 3;
    -- Location: same, immediately after EDGE_SOURCE_CO_ACCESS
    -- Re-exported: same pub use read::{...} block
    -- Relationship: migration.rs has a file-private CO_ACCESS_BOOTSTRAP_MIN_COUNT = 3;
       the migration constant is NOT removed (out of scope); both must stay equal to 3
```

### New module-private constant in `co_access_promotion_tick.rs`

```
const CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1;
    -- f64, NOT f32 — sqlx fetches SQLite REAL as f64; f32 introduces precision noise
    -- Module-private: not operator-configurable (ADR-003)
```

### New constant in `background.rs`

```
const PROMOTION_EARLY_RUN_WARN_TICKS: u32 = 5;
    -- Used by run_co_access_promotion_tick to detect SR-05 signal-loss scenario
    -- Defined in background.rs alongside other tick-level constants
```

### New field in `InferenceConfig` (config.rs)

```
pub max_co_access_promotion_per_tick: usize
    -- Default: 200 (via serde default fn)
    -- Valid range: [1, 10000]
    -- Mirrors max_graph_inference_per_tick pattern exactly
```

### New row struct (module-private in `co_access_promotion_tick.rs`)

```
struct CoAccessBatchRow {
    entry_id_a: i64,
    entry_id_b: i64,
    count: i64,
    max_count: Option<i64>,   -- NULL when table is empty after WHERE filter → early return
}
```

## Integration Surface (from ARCHITECTURE.md, verbatim)

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `run_co_access_promotion_tick` | `async fn(store: &Store, config: &InferenceConfig, current_tick: u32)` | `services/co_access_promotion_tick.rs` |
| `EDGE_SOURCE_CO_ACCESS` | `pub const &str = "co_access"` | `unimatrix-store/src/read.rs` |
| `CO_ACCESS_GRAPH_MIN_COUNT` | `pub const i64 = 3` | `unimatrix-store/src/read.rs` |
| `CO_ACCESS_WEIGHT_UPDATE_DELTA` | `const f64 = 0.1` (module-private) | `services/co_access_promotion_tick.rs` |
| `InferenceConfig::max_co_access_promotion_per_tick` | `usize`, default 200, range [1,10000] | `infra/config.rs` |
| `services/mod.rs` registration | `pub(crate) mod co_access_promotion_tick;` | `services/mod.rs` |

Note: The architecture's integration surface table lists the two-parameter form
`async fn(store: &Store, config: &InferenceConfig)`. The resolved three-parameter form
`async fn(store: &Store, config: &InferenceConfig, current_tick: u32)` is authoritative
per IMPLEMENTATION-BRIEF.md and ADR-005 — the `current_tick: u32` parameter is required
for the SR-05 early-tick warn detection. The integration surface table in ARCHITECTURE.md
is the two-parameter form and should be treated as superseded by the IMPLEMENTATION-BRIEF
resolution.

## Key Architectural Decisions (Summary)

- ADR-001 (#3823): Single batch SELECT with embedded scalar subquery for MAX — one SQL round-trip for fetch+normalize
- ADR-002 (#3824): Constants co-located with EDGE_SOURCE_NLI in read.rs — no new constants.rs sub-module
- ADR-003 (#3825): CO_ACCESS_WEIGHT_UPDATE_DELTA is f64 module-private constant — not f32, not config field
- ADR-004 (#3826): max_co_access_promotion_per_tick in InferenceConfig follows max_graph_inference_per_tick pattern exactly
- ADR-005 (#3827): Tick insertion between orphaned-edge compaction and TypedGraphState::rebuild; ORDERING INVARIANT anchor comment; SR-05 warn on qualifying_count==0 AND current_tick < 5
- ADR-006 (#3828): One-directional edges only in v1 — source_id=entry_id_a, target_id=entry_id_b
