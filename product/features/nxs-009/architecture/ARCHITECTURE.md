# nxs-009: Observation Metrics Normalization — Architecture

## Overview

nxs-009 decomposes the last bincode blob in the schema — `observation_metrics.data` — into SQL columns for `UniversalMetrics` (21 fixed columns) and a new `observation_phase_metrics` junction table for the variable-shape `PhaseMetrics`. Schema version advances from v8 to v9. The `MetricVector` types move to `unimatrix-store` (where `EntryRecord` already lives), with re-exports from `unimatrix-observe` for backward compatibility.

## Open Question Resolutions

| # | Question | Decision | ADR |
|---|----------|----------|-----|
| 1 | Where do MetricVector types live? | `unimatrix-store` (same crate as EntryRecord), re-export from observe | [ADR-001](ADR-001-metric-types-in-store.md) |
| 2 | Migration deserializer location | Self-contained in `migration_compat.rs` (same module that holds v5 deserializers) | [ADR-002](ADR-002-migration-deserializer.md) |
| 3 | ON DELETE CASCADE for phase metrics | Yes, with foreign key on observation_phase_metrics | [ADR-003](ADR-003-phase-metrics-cascade.md) |

## Architectural Decisions

| ADR | Title | Mitigates |
|-----|-------|-----------|
| ADR-001 | MetricVector Types in unimatrix-store | SR-01, SR-05 |
| ADR-002 | Self-Contained Migration Deserializer | SR-02 |
| ADR-003 | Phase Metrics Foreign Key with CASCADE | SR-03 |

## Target Schema (v9)

### observation_metrics (23 columns, replaces bincode blob)

```sql
CREATE TABLE observation_metrics (
    feature_cycle                     TEXT    PRIMARY KEY,
    computed_at                       INTEGER NOT NULL DEFAULT 0,
    total_tool_calls                  INTEGER NOT NULL DEFAULT 0,
    total_duration_secs               INTEGER NOT NULL DEFAULT 0,
    session_count                     INTEGER NOT NULL DEFAULT 0,
    search_miss_rate                  REAL    NOT NULL DEFAULT 0.0,
    edit_bloat_total_kb               REAL    NOT NULL DEFAULT 0.0,
    edit_bloat_ratio                  REAL    NOT NULL DEFAULT 0.0,
    permission_friction_events        INTEGER NOT NULL DEFAULT 0,
    bash_for_search_count             INTEGER NOT NULL DEFAULT 0,
    cold_restart_events               INTEGER NOT NULL DEFAULT 0,
    coordinator_respawn_count         INTEGER NOT NULL DEFAULT 0,
    parallel_call_rate                REAL    NOT NULL DEFAULT 0.0,
    context_load_before_first_write_kb REAL   NOT NULL DEFAULT 0.0,
    total_context_loaded_kb           REAL    NOT NULL DEFAULT 0.0,
    post_completion_work_pct          REAL    NOT NULL DEFAULT 0.0,
    follow_up_issues_created          INTEGER NOT NULL DEFAULT 0,
    knowledge_entries_stored          INTEGER NOT NULL DEFAULT 0,
    sleep_workaround_count            INTEGER NOT NULL DEFAULT 0,
    agent_hotspot_count               INTEGER NOT NULL DEFAULT 0,
    friction_hotspot_count            INTEGER NOT NULL DEFAULT 0,
    session_hotspot_count             INTEGER NOT NULL DEFAULT 0,
    scope_hotspot_count               INTEGER NOT NULL DEFAULT 0
);
```

No additional indexes needed — the table is keyed by `feature_cycle` (PRIMARY KEY) and expected to remain small (one row per retrospected feature). Cross-feature analytics use full-table scans which are efficient at this scale.

### observation_phase_metrics (junction table, new)

```sql
CREATE TABLE observation_phase_metrics (
    feature_cycle   TEXT    NOT NULL,
    phase_name      TEXT    NOT NULL,
    duration_secs   INTEGER NOT NULL DEFAULT 0,
    tool_call_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (feature_cycle, phase_name),
    FOREIGN KEY (feature_cycle) REFERENCES observation_metrics(feature_cycle) ON DELETE CASCADE
);
```

No additional indexes needed — the composite PRIMARY KEY covers lookup by `feature_cycle` and the table is small.

## Component Architecture

### Type Location (ADR-001)

```
unimatrix-store/src/metrics.rs   (NEW)
├── MetricVector
├── UniversalMetrics
└── PhaseMetrics

unimatrix-observe/src/types.rs
├── pub use unimatrix_store::{MetricVector, UniversalMetrics, PhaseMetrics};  (NEW re-export)
├── (remove MetricVector, UniversalMetrics, PhaseMetrics definitions)
├── (remove serialize_metric_vector, deserialize_metric_vector)
└── (keep all other types: HotspotFinding, BaselineEntry, etc.)

unimatrix-core/src/lib.rs
└── pub use unimatrix_store::{MetricVector, UniversalMetrics, PhaseMetrics};  (NEW re-export)
```

This follows the `EntryRecord` pattern: defined in store, re-exported by core and downstream crates. The dependency graph remains unchanged — no new edges.

### Store API Changes

```rust
// OLD (bytes boundary)
pub fn store_metrics(&self, feature_cycle: &str, data: &[u8]) -> Result<()>
pub fn get_metrics(&self, feature_cycle: &str) -> Result<Option<Vec<u8>>>
pub fn list_all_metrics(&self) -> Result<Vec<(String, Vec<u8>)>>

// NEW (typed boundary)
pub fn store_metrics(&self, feature_cycle: &str, mv: &MetricVector) -> Result<()>
pub fn get_metrics(&self, feature_cycle: &str) -> Result<Option<MetricVector>>
pub fn list_all_metrics(&self) -> Result<Vec<(String, MetricVector)>>
```

### Write Path (store_metrics)

```
store_metrics(feature_cycle, &mv)
├── BEGIN IMMEDIATE
├── INSERT OR REPLACE INTO observation_metrics (feature_cycle, computed_at, ...)
│   VALUES (?1, ?2, ..., ?23)
├── DELETE FROM observation_phase_metrics WHERE feature_cycle = ?1
├── for (phase_name, phase) in &mv.phases:
│   └── INSERT INTO observation_phase_metrics (feature_cycle, phase_name, ...)
│       VALUES (?1, ?2, ?3, ?4)
└── COMMIT
```

The DELETE+INSERT pattern for phases handles INSERT OR REPLACE semantics cleanly. The CASCADE foreign key means a DELETE of the parent row also cleans up phases, but the explicit DELETE in the write path handles the replace case where the parent row survives.

### Read Path (get_metrics)

```
get_metrics(feature_cycle)
├── SELECT * FROM observation_metrics WHERE feature_cycle = ?1
│   → construct MetricVector with universal fields + computed_at
├── SELECT phase_name, duration_secs, tool_call_count
│   FROM observation_phase_metrics WHERE feature_cycle = ?1
│   → populate mv.phases BTreeMap
└── return Some(mv) or None
```

### Read Path (list_all_metrics)

```
list_all_metrics()
├── SELECT * FROM observation_metrics ORDER BY feature_cycle
│   → Vec of (feature_cycle, partial MetricVector)
├── SELECT feature_cycle, phase_name, duration_secs, tool_call_count
│   FROM observation_phase_metrics ORDER BY feature_cycle, phase_name
│   → group by feature_cycle, attach to corresponding MetricVector
└── return Vec<(String, MetricVector)>
```

The two-query approach is more efficient than N+1 (one query per feature for phases). The second query returns all phase metrics sorted by feature_cycle, enabling a single-pass merge.

### Migration Path (v8 → v9)

```
migrate_if_needed()
├── if current_version < 9:
│   ├── Read all rows from observation_metrics (feature_cycle, data BLOB)
│   ├── For each row:
│   │   ├── deserialize_metric_vector_v8(data) → MetricVector
│   │   │   (skip on error, log warning, insert default)
│   │   └── Collect (feature_cycle, MetricVector)
│   ├── DROP TABLE observation_metrics
│   ├── CREATE TABLE observation_metrics (23 columns)
│   ├── CREATE TABLE observation_phase_metrics (junction)
│   ├── For each (feature_cycle, mv):
│   │   ├── INSERT INTO observation_metrics (columns...)
│   │   └── INSERT INTO observation_phase_metrics (per phase)
│   └── UPDATE counters SET value = 9 WHERE name = 'schema_version'
```

The migration uses the self-contained deserializer in `migration_compat.rs` (ADR-002). Failed deserialization inserts a default MetricVector with `computed_at = 0` to preserve the feature_cycle key.

### Server Changes

The `context_retrospective` tool currently:
1. Serializes `MetricVector` to bincode bytes
2. Passes bytes to `store.store_metrics()`
3. Retrieves bytes from `store.get_metrics()`
4. Deserializes bytes back to `MetricVector`

After nxs-009:
1. Passes `&MetricVector` directly to `store.store_metrics()`
2. Retrieves `MetricVector` directly from `store.get_metrics()`
3. No bincode serialization/deserialization in the hot path

### Removed Code

- `unimatrix-observe/src/types.rs`: `serialize_metric_vector()`, `deserialize_metric_vector()` — removed (bincode serde no longer needed for metrics)
- `unimatrix-observe/src/types.rs`: `MetricVector`, `UniversalMetrics`, `PhaseMetrics` struct definitions — moved to store, re-exported
- Related tests in `types.rs` for bincode roundtrip — removed or adapted

## Wave Structure

Single wave — all changes are tightly coupled:

1. **Add types to store** (`metrics.rs`) — define MetricVector, UniversalMetrics, PhaseMetrics
2. **Update schema** (`db.rs`) — new table definitions
3. **Add migration** (`migration.rs`, `migration_compat.rs`) — v8→v9 with self-contained deserializer
4. **Update store API** (`read.rs`, `write_ext.rs`) — typed methods
5. **Update observe** (`types.rs`) — remove definitions, add re-exports, remove bincode helpers
6. **Update server** (`mcp/tools.rs`, `services/status.rs`) — remove bincode calls
7. **Update tests** — adapt integration tests in `sqlite_parity.rs` and server tests

## Integration Surfaces

| Surface | Change | Risk |
|---------|--------|------|
| `Store::store_metrics()` | Signature change: `&[u8]` → `&MetricVector` | All callers must update (server only) |
| `Store::get_metrics()` | Return type change: `Option<Vec<u8>>` → `Option<MetricVector>` | All callers must update (server only) |
| `Store::list_all_metrics()` | Return type change: `Vec<(String, Vec<u8>)>` → `Vec<(String, MetricVector)>` | All callers must update (server only) |
| `unimatrix_observe::MetricVector` | Still accessible via re-export | Zero-disruption for observe consumers |
| `unimatrix_observe::serialize_metric_vector` | Removed | Server must stop calling it |
| `unimatrix_observe::deserialize_metric_vector` | Removed | Server must stop calling it |
| `create_tables()` in `db.rs` | New table schema | Existing DBs migrated by v8→v9 |
