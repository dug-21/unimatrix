# nxs-009: Observation Metrics Normalization — Specification

## Functional Requirements

### FR-01: Schema v9 Table Definition

The `observation_metrics` table SHALL have 23 columns: `feature_cycle` (TEXT PRIMARY KEY), `computed_at` (INTEGER), and 21 universal metric columns matching the fields of `UniversalMetrics`. The `data BLOB` column SHALL be removed.

### FR-02: Phase Metrics Junction Table

A new `observation_phase_metrics` table SHALL exist with columns: `feature_cycle` (TEXT), `phase_name` (TEXT), `duration_secs` (INTEGER), `tool_call_count` (INTEGER), composite PRIMARY KEY on `(feature_cycle, phase_name)`, and a FOREIGN KEY referencing `observation_metrics(feature_cycle)` with ON DELETE CASCADE.

### FR-03: Typed Store API — Write

`Store::store_metrics(feature_cycle: &str, mv: &MetricVector) -> Result<()>` SHALL:
- Write all universal metric fields as SQL columns to `observation_metrics`
- Delete existing phase rows for the feature_cycle from `observation_phase_metrics`
- Insert one row per phase into `observation_phase_metrics`
- Execute all operations within a single SQLite transaction
- Use INSERT OR REPLACE semantics for the parent row

### FR-04: Typed Store API — Read Single

`Store::get_metrics(feature_cycle: &str) -> Result<Option<MetricVector>>` SHALL:
- Return `None` if no row exists for the feature_cycle
- Read all universal metric columns from `observation_metrics`
- Read all phase rows from `observation_phase_metrics` for the feature_cycle
- Construct and return a complete `MetricVector`

### FR-05: Typed Store API — Read All

`Store::list_all_metrics() -> Result<Vec<(String, MetricVector)>>` SHALL:
- Return all stored MetricVectors ordered by feature_cycle
- Use at most 2 SQL queries (one for universal metrics, one for all phase metrics)
- Merge phase metrics into their corresponding MetricVectors in a single pass

### FR-06: Migration v8 to v9

When opening a database with schema_version = 8:
- Read all `(feature_cycle, data)` rows from the old `observation_metrics` table
- Deserialize each bincode blob using a self-contained v8 deserializer
- On deserialization failure: insert a default `MetricVector` (computed_at = 0, all metrics = 0, no phases) to preserve the feature_cycle key
- Drop the old `observation_metrics` table
- Create the new `observation_metrics` table (23 columns)
- Create the `observation_phase_metrics` table
- Insert migrated data into both tables
- Update schema_version to 9
- Execute within a single transaction (rollback on any failure)

### FR-07: Type Definitions in unimatrix-store

`MetricVector`, `UniversalMetrics`, and `PhaseMetrics` SHALL be defined in `unimatrix-store/src/metrics.rs` with:
- `#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]` on MetricVector
- `#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]` on UniversalMetrics and PhaseMetrics
- `#[serde(default)]` on all fields for forward-compatible deserialization
- `Default` implementation for MetricVector (computed_at = 0, empty phases)
- `BTreeMap<String, PhaseMetrics>` for the phases field

### FR-08: Backward-Compatible Re-exports

`unimatrix-observe/src/types.rs` SHALL re-export `MetricVector`, `UniversalMetrics`, `PhaseMetrics` from `unimatrix-store` with a comment referencing nxs-009.

`unimatrix-core/src/lib.rs` SHALL re-export the same types from `unimatrix-store`.

### FR-09: Remove Bincode Serialization Helpers

`serialize_metric_vector()` and `deserialize_metric_vector()` in `unimatrix-observe/src/types.rs` SHALL be removed. All callers SHALL be updated to use the typed Store API.

### FR-10: Server Integration — context_retrospective

The `context_retrospective` tool SHALL:
- Pass `&MetricVector` directly to `store.store_metrics()` (no bincode serialization)
- Receive `MetricVector` directly from `store.get_metrics()` (no bincode deserialization)
- Receive `Vec<(String, MetricVector)>` from `store.list_all_metrics()` (no per-item deserialization)
- Produce identical `RetrospectiveReport` output as before

### FR-11: Server Integration — context_status

The `context_status` tool SHALL continue to count retrospected features using `list_all_metrics().len()`. The typed return value is a transparent change.

## Non-Functional Requirements

### NFR-01: Migration Safety

The v8→v9 migration SHALL create a backup of the database file at `{path}.v8-backup` before starting, following the nxs-008 precedent.

### NFR-02: Transaction Atomicity

All multi-table write operations SHALL be wrapped in SQLite transactions. A failure in any step SHALL roll back the entire operation.

### NFR-03: No Performance Regression

`store_metrics()`, `get_metrics()`, and `list_all_metrics()` SHALL have comparable or better performance than the bincode blob approach. The elimination of serialization/deserialization is expected to offset the cost of multiple SQL operations.

### NFR-04: Zero Downtime

Schema migration SHALL run automatically on database open (existing pattern). No manual intervention required.

## Acceptance Criteria

### AC-01: Schema Verification
Given a fresh database opened after nxs-009,
When inspecting the schema,
Then `observation_metrics` has 23 columns (no `data BLOB`) and `observation_phase_metrics` exists with 4 columns and a foreign key.

### AC-02: Store Roundtrip
Given a MetricVector with 21 universal metrics, 3 phase entries, and a non-zero computed_at,
When stored via `store_metrics()` and retrieved via `get_metrics()`,
Then the retrieved MetricVector equals the original.

### AC-03: Store Replace
Given an existing MetricVector stored for feature "col-001" with phases ["3a", "3b"],
When a new MetricVector with phases ["3a", "3c"] is stored for the same feature,
Then `get_metrics("col-001")` returns the new vector with phases ["3a", "3c"] only.

### AC-04: List All with Phases
Given 3 stored MetricVectors each with different phase configurations,
When `list_all_metrics()` is called,
Then all 3 vectors are returned with correct phase data, ordered by feature_cycle.

### AC-05: Migration from v8
Given a database at schema v8 with 2 observation_metrics rows (bincode blobs),
When the database is opened (triggering migration),
Then both rows are present in the new columnar schema with correct universal and phase metric values.

### AC-06: Migration Corrupted Blob
Given a database at schema v8 with 1 valid and 1 corrupted observation_metrics blob,
When migration runs,
Then the valid row is migrated correctly and the corrupted row is preserved as a default MetricVector.

### AC-07: Delete Cascade
Given a stored MetricVector with 3 phase entries,
When the parent row is deleted from `observation_metrics`,
Then all corresponding rows in `observation_phase_metrics` are also deleted.

### AC-08: Server Retrospective Unchanged
Given observation data for a feature,
When `context_retrospective` is called,
Then the output format is identical to pre-nxs-009 behavior (same JSON fields, same values).

### AC-09: Server Status Unchanged
Given stored metrics for N features,
When `context_status` is called,
Then the retrospected feature count equals N.

### AC-10: Bincode Removal
After nxs-009, `serialize_metric_vector()` and `deserialize_metric_vector()` SHALL NOT exist in the public API of `unimatrix-observe`.

### AC-11: Re-export Compatibility
Code importing `unimatrix_observe::MetricVector` SHALL continue to compile without changes.

### AC-12: Empty Phases Roundtrip
Given a MetricVector with no phase entries (empty BTreeMap),
When stored and retrieved,
Then the retrieved MetricVector has an empty phases map.

### AC-13: SQL Analytics Enabled
Given stored metrics for multiple features,
When executing `SELECT feature_cycle, total_tool_calls FROM observation_metrics WHERE session_count > 5`,
Then results are returned directly without Rust-side deserialization.

## Domain Model

```
MetricVector
├── computed_at: u64
├── universal: UniversalMetrics (21 fields)
│   ├── total_tool_calls: u64
│   ├── total_duration_secs: u64
│   ├── session_count: u64
│   ├── search_miss_rate: f64
│   ├── edit_bloat_total_kb: f64
│   ├── edit_bloat_ratio: f64
│   ├── permission_friction_events: u64
│   ├── bash_for_search_count: u64
│   ├── cold_restart_events: u64
│   ├── coordinator_respawn_count: u64
│   ├── parallel_call_rate: f64
│   ├── context_load_before_first_write_kb: f64
│   ├── total_context_loaded_kb: f64
│   ├── post_completion_work_pct: f64
│   ├── follow_up_issues_created: u64
│   ├── knowledge_entries_stored: u64
│   ├── sleep_workaround_count: u64
│   ├── agent_hotspot_count: u64
│   ├── friction_hotspot_count: u64
│   ├── session_hotspot_count: u64
│   └── scope_hotspot_count: u64
└── phases: BTreeMap<String, PhaseMetrics>
    └── PhaseMetrics
        ├── duration_secs: u64
        └── tool_call_count: u64
```

## Constraints

- C-01: No changes to `MetricVector` field names or types — struct is binary-compatible
- C-02: No new MCP tools
- C-03: No changes to the `observations` table (raw hook events)
- C-04: Schema version must be exactly 9 (next sequential after v8)
- C-05: Foreign keys must remain enabled (`PRAGMA foreign_keys = ON`)
- C-06: All SQL column names must match Rust field names for maintainability
