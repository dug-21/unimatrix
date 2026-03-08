# nxs-009: Observation Metrics Normalization

## Problem Statement

The `observation_metrics` table stores `MetricVector` structs as opaque bincode blobs. This is the last remaining bincode blob in the schema (explicitly excluded from nxs-008 via ADR #354, tracked as GH #103). Every consumer must deserialize through Rust to access any metric value, preventing direct SQL analytics, cross-feature metric comparison queries, and external tooling integration.

The core challenge is **variable-shape data**: `MetricVector` contains a fixed `UniversalMetrics` struct (21 numeric fields) plus a `BTreeMap<String, PhaseMetrics>` where phase names vary per feature (e.g., "3a", "3b", "3c", "design", "implementation"). Phase names are not enumerated — they emerge from session observation data.

### Current State

**Schema** (v8):
```sql
CREATE TABLE observation_metrics (
    feature_cycle TEXT PRIMARY KEY,
    data BLOB NOT NULL
);
```

**MetricVector** (defined in `unimatrix-observe/src/types.rs`):
```rust
pub struct MetricVector {
    pub computed_at: u64,
    pub universal: UniversalMetrics,     // 21 fixed fields
    pub phases: BTreeMap<String, PhaseMetrics>,  // variable-shape
}

pub struct UniversalMetrics {
    pub total_tool_calls: u64,
    pub total_duration_secs: u64,
    pub session_count: u64,
    pub search_miss_rate: f64,
    pub edit_bloat_total_kb: f64,
    pub edit_bloat_ratio: f64,
    pub permission_friction_events: u64,
    pub bash_for_search_count: u64,
    pub cold_restart_events: u64,
    pub coordinator_respawn_count: u64,
    pub parallel_call_rate: f64,
    pub context_load_before_first_write_kb: f64,
    pub total_context_loaded_kb: f64,
    pub post_completion_work_pct: f64,
    pub follow_up_issues_created: u64,
    pub knowledge_entries_stored: u64,
    pub sleep_workaround_count: u64,
    pub agent_hotspot_count: u64,
    pub friction_hotspot_count: u64,
    pub session_hotspot_count: u64,
    pub scope_hotspot_count: u64,
}

pub struct PhaseMetrics {
    pub duration_secs: u64,
    pub tool_call_count: u64,
}
```

**Serialization** (bincode, `unimatrix-observe/src/types.rs`):
- `serialize_metric_vector()` / `deserialize_metric_vector()` using `bincode::config::standard()`
- All fields use `#[serde(default)]` for forward compatibility

### Consumers

1. **`context_retrospective` MCP tool** (`unimatrix-server/src/mcp/tools.rs`):
   - Writes: serializes MetricVector after computation, stores via `store.store_metrics()`
   - Reads: retrieves cached blob via `store.get_metrics()`, deserializes for cached-report path
   - Reads all: `store.list_all_metrics()` to load historical vectors, deserializes each for baseline comparison

2. **Baseline computation** (`unimatrix-observe/src/baseline.rs`):
   - Receives `&[MetricVector]` (already deserialized by caller)
   - Iterates all 21 universal metrics via extractor functions
   - Iterates phase metrics for cross-feature phase duration/tool-call baselines

3. **`context_status` MCP tool** (`unimatrix-server/src/services/status.rs`):
   - Calls `list_all_metrics()` only to count retrospected features (`.len()`)
   - Does NOT inspect metric values

### Store API

- `store.store_metrics(feature_cycle: &str, data: &[u8]) -> Result<()>` — INSERT OR REPLACE
- `store.get_metrics(feature_cycle: &str) -> Result<Option<Vec<u8>>>` — single lookup
- `store.list_all_metrics() -> Result<Vec<(String, Vec<u8>)>>` — full table scan

## Goals

1. **Decompose UniversalMetrics into SQL columns** on the `observation_metrics` table — 21 fixed numeric columns directly queryable via SQL
2. **Normalize PhaseMetrics into a separate table** — `observation_phase_metrics(feature_cycle, phase_name, duration_secs, tool_call_count)` to handle the variable-shape dimension
3. **Add `computed_at` as a SQL column** on `observation_metrics`
4. **Migrate existing data** from schema v8 (bincode blob) to schema v9 (SQL columns) with automatic migration
5. **Remove bincode serialization** for MetricVector from the write/read path — replace with direct SQL column read/write
6. **Update Store API** to accept/return `MetricVector` directly instead of `&[u8]` / `Vec<u8>`
7. **Enable SQL-native analytics** — e.g., `SELECT feature_cycle, total_tool_calls, session_count FROM observation_metrics WHERE total_tool_calls > 100`
8. **Update all consumers** in unimatrix-server to use the new typed API

## Non-Goals

- **No new MCP tools** — existing `context_retrospective` and `context_status` continue to work identically
- **No changes to MetricVector struct definition** — the Rust type stays the same, only its storage representation changes
- **No changes to observation ingestion** — the `observations` table (structured events) is unaffected
- **No changes to baseline computation logic** — `compute_baselines()` and `compare_to_baseline()` continue to receive `&[MetricVector]`
- **No external analytics tooling** — this enables future SQL analytics but does not build any
- **No changes to the `observations` table** — raw hook events stay as-is

## Design Options for Variable-Shape PhaseMetrics

### Option A: Separate Junction Table (Recommended)

```sql
-- Fixed-shape universal metrics as columns
CREATE TABLE observation_metrics (
    feature_cycle TEXT PRIMARY KEY,
    computed_at INTEGER NOT NULL,
    total_tool_calls INTEGER NOT NULL DEFAULT 0,
    total_duration_secs INTEGER NOT NULL DEFAULT 0,
    session_count INTEGER NOT NULL DEFAULT 0,
    search_miss_rate REAL NOT NULL DEFAULT 0.0,
    -- ... (21 universal metric columns)
);

-- Variable-shape phase metrics as rows
CREATE TABLE observation_phase_metrics (
    feature_cycle TEXT NOT NULL,
    phase_name TEXT NOT NULL,
    duration_secs INTEGER NOT NULL DEFAULT 0,
    tool_call_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (feature_cycle, phase_name),
    FOREIGN KEY (feature_cycle) REFERENCES observation_metrics(feature_cycle) ON DELETE CASCADE
);
```

**Pros**: Full SQL queryability. Phase metrics are naturally relational. Follows the `entry_tags` junction table pattern from nxs-008. Standard SQL aggregation across phases (`GROUP BY phase_name`).

**Cons**: Two-table write/read (minor — same pattern used everywhere else in the schema).

### Option B: JSON Column for Phases

```sql
CREATE TABLE observation_metrics (
    feature_cycle TEXT PRIMARY KEY,
    computed_at INTEGER NOT NULL,
    -- 21 universal columns...
    phases TEXT NOT NULL DEFAULT '{}'  -- JSON object
);
```

**Pros**: Single table. Simple migration.

**Cons**: Phase metrics not directly queryable without SQLite JSON functions. Inconsistent with the rest of the normalized schema. Mixes normalized (universal) and denormalized (phases) in one table.

### Option C: EAV (Entity-Attribute-Value)

```sql
CREATE TABLE observation_metrics_kv (
    feature_cycle TEXT NOT NULL,
    metric_name TEXT NOT NULL,
    metric_value REAL NOT NULL,
    PRIMARY KEY (feature_cycle, metric_name)
);
```

**Pros**: Maximum flexibility for adding new metrics.

**Cons**: No type safety. Terrible query ergonomics for fixed-shape data. Requires PIVOT-style queries for any cross-metric analysis. Over-generalizes the problem.

### Recommendation

**Option A** — matches the nxs-008 precedent (entry_tags junction table), provides full SQL queryability, and handles the variable-shape dimension cleanly.

## Migration Strategy

Following the nxs-008 migration pattern:

1. **Schema v8 to v9** migration in `migration.rs`
2. Read all existing `observation_metrics` rows
3. Deserialize each bincode blob via `deserialize_metric_vector()`
4. Write universal metrics as columns to new `observation_metrics` table
5. Write phase metrics as rows to new `observation_phase_metrics` table
6. Drop the `data BLOB` column (via table rebuild since SQLite cannot drop columns pre-3.35.0)
7. Update `CURRENT_SCHEMA_VERSION` to 9

## Scope of Changes

### unimatrix-store
- `db.rs`: Update `create_tables()` with new schema for `observation_metrics` (columns) and new `observation_phase_metrics` table
- `write_ext.rs`: Replace `store_metrics(&str, &[u8])` with `store_metrics(&str, &MetricVector)` — writes columns + phase rows
- `read.rs`: Replace `get_metrics(&str) -> Option<Vec<u8>>` with `get_metrics(&str) -> Option<MetricVector>` — reads columns + phase rows; replace `list_all_metrics() -> Vec<(String, Vec<u8>)>` with `list_all_metrics() -> Vec<(String, MetricVector)>`
- `migration.rs`: Add v8-to-v9 migration
- New dependency: `unimatrix-observe` types (MetricVector, UniversalMetrics, PhaseMetrics) — or move these types to `unimatrix-core` to avoid circular dependency

### unimatrix-observe
- `types.rs`: Remove `serialize_metric_vector()` / `deserialize_metric_vector()` (or deprecate, keep for migration compat)
- Keep `MetricVector`, `UniversalMetrics`, `PhaseMetrics` struct definitions unchanged

### unimatrix-server
- `mcp/tools.rs`: Remove bincode serialize/deserialize calls in `context_retrospective` — pass `MetricVector` directly to/from store
- `services/status.rs`: Update `list_all_metrics()` call site (now returns typed data, but still only counts)

### Dependency Consideration

Currently `unimatrix-store` does not depend on `unimatrix-observe`. The new `store_metrics(&str, &MetricVector)` API would require the store to know about `MetricVector`. Options:
1. **Move MetricVector to unimatrix-core** (preferred) — core is already a shared dependency
2. Add unimatrix-observe as a dependency of unimatrix-store (creates tighter coupling)
3. Keep the `&[u8]` API but add typed convenience methods at the server layer

## Open Questions

1. **Type location**: Should `MetricVector`, `UniversalMetrics`, `PhaseMetrics` move to `unimatrix-core`? This follows the precedent of `ObservationRecord` which was moved to core in col-013. The alternative is keeping the `&[u8]` boundary and only normalizing at the SQL level.

2. **Backward compatibility**: The migration deserializer needs to handle the bincode format. Should `deserialize_metric_vector()` be kept in observe as a `pub(crate)` or `#[cfg(feature = "migration")]` function, or should the migration code in unimatrix-store carry its own copy?

3. **Phase name cardinality**: How many distinct phase names exist in practice? If the set is small and stable (e.g., "3a", "3b", "3c", "design", "implementation"), the junction table approach is clearly correct. If phase names could proliferate unboundedly, we might want to reconsider.

4. **Index requirements**: Which columns on `observation_metrics` need indexes for anticipated query patterns? The table is likely small (one row per retrospected feature), so indexes may be unnecessary.

5. **Foreign key cascading**: Should `ON DELETE CASCADE` on `observation_phase_metrics` be used? If a feature's metrics row is deleted, should its phase metrics auto-delete?
