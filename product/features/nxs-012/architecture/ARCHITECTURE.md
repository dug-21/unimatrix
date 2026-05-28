# Architecture: nxs-012 Export/Import Complete Persistent State Coverage

## System Overview

nxs-012 extends the existing export/import CLI pipeline (nan-001 export, nan-002 import) from 8 tables to 11 tables by adding `graph_edges`, `observations`, and `cycle_events`. The pipeline lives entirely in `crates/unimatrix-server/src/` across four files: `export.rs`, `format.rs`, `import/mod.rs`, and `import/inserters.rs`. No new files are created. The format_version bumps from 1 to 2 to signal the presence of new table types.

This feature also adds `--skip-quarantined` and `--confirm` flags to the **export** subcommand, allowing users to produce clean snapshots that exclude quarantined entries and all rows that reference them. Import remains a simple full-restore.

## Component Breakdown

### C1: Format Types (`format.rs`)

**Responsibility**: Define the typed deserialization contract for all 11 table types.

**Changes**:
- Add `GraphEdgeRow` struct (9 fields -- no `id`)
- Add `ObservationRow` struct (10 fields -- includes `id`)
- Add `CycleEventRow` struct (9 fields -- excludes `goal_embedding`)
- Add 3 variants to `ExportRow` enum: `GraphEdge`, `Observation`, `CycleEvent`

**Key detail**: `CycleEventRow` does NOT include `goal_embedding`. The field is excluded from both export SELECT and format struct. If a future format version adds it, `goal_embedding` would be `Option<serde_json::Value>` with `#[serde(default)]` -- but that is out of scope.

### C2: Export Functions (`export.rs`)

**Responsibility**: Query each table and serialize rows as JSONL, with optional quarantine filtering.

**Changes**:
- Add `export_graph_edges(pool, writer, skip_ids)` -- 9 columns, ORDER BY source_id, target_id, relation_type
- Add `export_observations(pool, writer)` -- 10 columns, ORDER BY id
- Add `export_cycle_events(pool, writer)` -- 9 columns (excluding goal_embedding), ORDER BY id
- Add 3 calls in `do_export` after existing 8 table calls
- Change `write_header` to emit `format_version: 2`
- Add `skip_ids: &HashSet<i64>` parameter to `do_export` and to the 5 affected table exporters (`export_entries`, `export_entry_tags`, `export_co_access`, `export_feature_entries`, `export_graph_edges`)
- Build `skip_ids` by querying `SELECT id FROM entries WHERE status = 3` inside the `BEGIN DEFERRED` transaction, before calling `do_export`
- Add `--skip-quarantined` and `--confirm` CLI flags to the export subcommand
- Add `--confirm` validation: abort if `--skip-quarantined` without `--confirm` (ADR-009)

### C3: Import Inserters (`import/inserters.rs`)

**Responsibility**: Parameterized INSERT statements for each table type.

**Changes**:
- Add `insert_graph_edge(conn, &GraphEdgeRow)` -- plain INSERT (not INSERT OR IGNORE) to surface duplicates
- Add `insert_observation(conn, &ObservationRow)` -- INSERT with explicit id
- Add `insert_cycle_event(conn, &CycleEventRow)` -- INSERT with explicit id, goal_embedding set to NULL

### C4: Import Pipeline (`import/mod.rs`)

**Responsibility**: Orchestrate header validation, data ingestion, and cleanup.

**Changes**:
- `ImportCounts`: add `graph_edges: u64`, `observations: u64`, `cycle_events: u64`
- `ingest_rows`: add 3 match arms routing to inserters
- `drop_all_data`: add DELETEs for `observation_phase_metrics`, `observation_metrics`, `graph_edges`, `observations`, `cycle_events` (ADR-001: FK-safe ordering)
- `print_summary`: add 3 lines
- `parse_header` / format_version validation: accept 1 or 2, reject 0 and 3+ (ADR-002)

### C5: Export-Side Skip-Quarantined Filter (`export.rs`)

**Responsibility**: Optionally exclude quarantined entries and all rows referencing them during export.

**Changes**:
- `--skip-quarantined` CLI flag (bool, default false) threads through `run_export` -> `run_export_inner` -> async block -> `do_export`
- `--confirm` CLI flag (bool, default false) required when `--skip-quarantined` is active (ADR-009)
- Inside the `BEGIN DEFERRED` transaction, before any `export_*` call:
  ```sql
  SELECT id FROM entries WHERE status = 3
  ```
  Results collected into `HashSet<i64>` named `skip_ids`. When `--skip-quarantined` is false, `skip_ids` is an empty `HashSet` -- zero overhead on the default path.
- `do_export` receives `skip_ids: &HashSet<i64>` and passes it to the 5 affected table exporters
- Each affected exporter checks its entry ID column(s) against `skip_ids` before calling `write_row`

**Skip-Quarantined Filter Cascade**:

```
Table exporter          Check column(s)              Action when match
-----------             -------------------------    -------------------
export_entries          id (status=3 in skip_ids)    Skip write_row
export_entry_tags       entry_id                     Skip write_row
export_feature_entries  entry_id                     Skip write_row
export_co_access        entry_id_a OR entry_id_b     Skip write_row
export_graph_edges      source_id OR target_id       Skip write_row
export_observations     (none)                       Always write
export_cycle_events     (none)                       Always write
export_counters         (none)                       Always write
export_outcome_index    (none)                       Always write
export_agent_registry   (none)                       Always write
export_audit_log        (none)                       Always write
```

**Key details**:
- The skip-set query runs inside the same `BEGIN DEFERRED` snapshot transaction as all table reads, eliminating TOCTOU races (SR-02)
- When `--skip-quarantined` is false, `skip_ids` is empty. The `contains()` calls are O(1) no-ops on an empty set (AC-29)
- The export file produced with `--skip-quarantined` has valid hash integrity because the footer hash covers exactly the filtered rows (AC-31)
- The export header includes `skip_quarantined: true` when the flag is active, so downstream consumers can identify filtered exports

## Component Interactions

```
CLI (run_export / run_import)
    |
    v
run_export_inner(skip_quarantined, confirm)
    |
    +-- if skip_quarantined && !confirm -> abort (ADR-009)
    |
    +-- BEGIN DEFERRED
    +-- if skip_quarantined:
    |     SELECT id FROM entries WHERE status = 3
    |     -> skip_ids: HashSet<i64>
    +-- else: skip_ids = empty HashSet
    |
    +-- do_export(pool, writer, &skip_ids)
    |    +-- write_header [format_version=2, skip_quarantined?]
    |    +-- export_counters
    |    +-- export_entries(&skip_ids)         -- check id
    |    +-- export_entry_tags(&skip_ids)      -- check entry_id
    |    +-- export_co_access(&skip_ids)       -- check entry_id_a, entry_id_b
    |    +-- export_feature_entries(&skip_ids)  -- check entry_id
    |    +-- export_outcome_index
    |    +-- export_agent_registry
    |    +-- export_audit_log
    |    +-- export_graph_edges(&skip_ids)      -- check source_id, target_id
    |    +-- export_observations
    |    +-- export_cycle_events
    |    +-- write_footer
    +-- COMMIT
    +-- if skip_quarantined: print skip summary to stderr

run_import_async(...)
    |
    +-- parse_header
    +-- check_preflight
    +-- drop_all_data (if --force)
    +-- BEGIN IMMEDIATE
    +-- ingest_rows
    |    +-- match ExportRow variant -> insert_* (all rows)
    +-- validate_hashes
    +-- COMMIT
    +-- reconstruct_embeddings
    +-- record_provenance
    +-- print_summary
```

### Data Flow

1. **Export**: `BEGIN DEFERRED` -> (optionally) build skip_ids -> query each table -> filter rows against skip_ids -> serialize to JSONL -> `COMMIT`
2. **Import**: parse header -> validate format_version (1 or 2) -> `drop_all_data` (if --force) -> `BEGIN IMMEDIATE` -> deserialize JSONL lines -> route to inserter -> validate hashes -> `COMMIT` -> reconstruct embeddings
3. **Export with --skip-quarantined --confirm**: Same as (1), but skip_ids is populated from the entries table. Each affected exporter checks its entry ID columns against skip_ids and omits matching rows. The export file is a clean snapshot. Import processes it as a normal full-restore.

### Error Boundaries

- Export errors propagate as `Box<dyn Error>` through `do_export` -> `run_export_inner`
- `--confirm` validation fails fast before any DB access (ADR-009)
- Import errors trigger ROLLBACK before propagation (existing pattern in `run_import_async`)
- Inserter errors surface SQLite constraint violations (UNIQUE, NOT NULL) as-is -- no wrapping
- `graph_edges` uses plain INSERT specifically to detect duplicate (source_id, target_id, relation_type) tuples as data corruption

## Technology Decisions

| Decision | ADR | Summary |
|----------|-----|---------|
| FK-safe DELETE ordering in drop_all_data | ADR-001 | Delete observation_phase_metrics and observation_metrics before observations |
| Format version acceptance range | ADR-002 | Accept 1 (legacy) and 2 (new), reject 0 and 3+ |
| f64 NaN safety for graph_edges.weight | ADR-003 | Reuse Number::from_f64 with fallback to 1.0 (default weight) |
| goal_embedding exclusion strategy | ADR-004 | Exclude from SELECT, not present in CycleEventRow |
| graph_edges.id omission | ADR-005 | Synthetic AUTOINCREMENT, unreferenced -- omit from export |
| observations.id and cycle_events.id preservation | ADR-006 | Preserve through export/import like audit_log pattern |
| ~~Skip-quarantined cascade via HashSet during ingest~~ | ~~ADR-007~~ | ~~SUPERSEDED by ADR-008~~ |
| Export-side skip-quarantined filter via pre-query HashSet | ADR-008 | Build HashSet<i64> from `SELECT id FROM entries WHERE status = 3` inside DEFERRED transaction; pass to 5 affected table exporters |
| --confirm safeguard for --skip-quarantined | ADR-009 | Require --confirm alongside --skip-quarantined; no interactive prompts (consistent with nan-002 ADR-003) |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `GraphEdgeRow` | `struct { source_id: i64, target_id: i64, relation_type: String, weight: f64, created_at: i64, created_by: String, source: String, bootstrap_only: i64, metadata: Option<String> }` | `format.rs` (new) |
| `ObservationRow` | `struct { id: i64, session_id: String, ts_millis: i64, hook: String, tool: Option<String>, input: Option<String>, response_size: Option<i64>, response_snippet: Option<String>, topic_signal: Option<String>, phase: Option<String> }` | `format.rs` (new) |
| `CycleEventRow` | `struct { id: i64, cycle_id: String, seq: i64, event_type: String, phase: Option<String>, outcome: Option<String>, next_phase: Option<String>, timestamp: i64, goal: Option<String> }` | `format.rs` (new) |
| `ExportRow::GraphEdge(GraphEdgeRow)` | enum variant `#[serde(rename = "graph_edges")]` | `format.rs` (new) |
| `ExportRow::Observation(ObservationRow)` | enum variant `#[serde(rename = "observations")]` | `format.rs` (new) |
| `ExportRow::CycleEvent(CycleEventRow)` | enum variant `#[serde(rename = "cycle_events")]` | `format.rs` (new) |
| `export_graph_edges(pool: &SqlitePool, writer: &mut impl Write, skip_ids: &HashSet<i64>)` | `async fn -> Result<(), Box<dyn Error>>` | `export.rs` (new) |
| `export_observations(pool: &SqlitePool, writer: &mut impl Write)` | `async fn -> Result<(), Box<dyn Error>>` | `export.rs` (new) |
| `export_cycle_events(pool: &SqlitePool, writer: &mut impl Write)` | `async fn -> Result<(), Box<dyn Error>>` | `export.rs` (new) |
| `export_entries(pool, writer, skip_ids)` | signature gains `skip_ids: &HashSet<i64>` | `export.rs` (modified) |
| `export_entry_tags(pool, writer, skip_ids)` | signature gains `skip_ids: &HashSet<i64>` | `export.rs` (modified) |
| `export_co_access(pool, writer, skip_ids)` | signature gains `skip_ids: &HashSet<i64>` | `export.rs` (modified) |
| `export_feature_entries(pool, writer, skip_ids)` | signature gains `skip_ids: &HashSet<i64>` | `export.rs` (modified) |
| `do_export(pool: &SqlitePool, writer: &mut impl Write, skip_ids: &HashSet<i64>)` | signature gains `skip_ids: &HashSet<i64>` | `export.rs` (modified) |
| `run_export(project_dir, output, skip_quarantined, confirm)` | `pub fn(Option<&Path>, Option<&Path>, bool, bool) -> Result<(), Box<dyn Error>>` | `export.rs` (modified) |
| `run_export_with_base(project_dir, output, base_dir, skip_quarantined, confirm)` | `pub fn(Option<&Path>, Option<&Path>, &Path, bool, bool) -> Result<(), Box<dyn Error>>` | `export.rs` (modified) |
| `insert_graph_edge(conn: &mut SqliteConnection, r: &GraphEdgeRow)` | `async fn -> Result<(), Box<dyn Error>>` | `inserters.rs` (new) |
| `insert_observation(conn: &mut SqliteConnection, r: &ObservationRow)` | `async fn -> Result<(), Box<dyn Error>>` | `inserters.rs` (new) |
| `insert_cycle_event(conn: &mut SqliteConnection, r: &CycleEventRow)` | `async fn -> Result<(), Box<dyn Error>>` | `inserters.rs` (new) |
| `ImportCounts.graph_edges` | `u64` | `import/mod.rs` (new field) |
| `ImportCounts.observations` | `u64` | `import/mod.rs` (new field) |
| `ImportCounts.cycle_events` | `u64` | `import/mod.rs` (new field) |
| `ingest_rows(conn, lines)` | `async fn(&mut SqliteConnection, impl Iterator<Item=io::Result<String>>) -> Result<ImportCounts, Box<dyn Error>>` | `import/mod.rs` (unchanged signature) |
| `run_import(project_dir, input, skip_hash_validation, force)` | `pub fn(Option<&Path>, &Path, bool, bool) -> Result<(), Box<dyn Error>>` | `import/mod.rs` (unchanged signature) |

### Export Column Mappings

**graph_edges** (9 columns, ORDER BY source_id, target_id, relation_type):
| JSON key | SQL column | Type | Nullable | Notes |
|----------|-----------|------|----------|-------|
| `_table` | -- | `"graph_edges"` | no | discriminator |
| `source_id` | source_id | i64 | no | |
| `target_id` | target_id | i64 | no | |
| `relation_type` | relation_type | String | no | |
| `weight` | weight | f64 | no | `Number::from_f64` with NaN->1.0 fallback |
| `created_at` | created_at | i64 | no | |
| `created_by` | created_by | String | no | |
| `source` | source | String | no | |
| `bootstrap_only` | bootstrap_only | i64 | no | |
| `metadata` | metadata | String/null | yes | `nullable_text` |

**observations** (10 columns, ORDER BY id):
| JSON key | SQL column | Type | Nullable | Notes |
|----------|-----------|------|----------|-------|
| `_table` | -- | `"observations"` | no | discriminator |
| `id` | id | i64 | no | preserved through import |
| `session_id` | session_id | String | no | |
| `ts_millis` | ts_millis | i64 | no | |
| `hook` | hook | String | no | |
| `tool` | tool | String/null | yes | `nullable_text` |
| `input` | input | String/null | yes | `nullable_text` |
| `response_size` | response_size | i64/null | yes | `nullable_int` |
| `response_snippet` | response_snippet | String/null | yes | `nullable_text` |
| `topic_signal` | topic_signal | String/null | yes | `nullable_text` |
| `phase` | phase | String/null | yes | `nullable_text` |

**cycle_events** (9 columns, ORDER BY id):
| JSON key | SQL column | Type | Nullable | Notes |
|----------|-----------|------|----------|-------|
| `_table` | -- | `"cycle_events"` | no | discriminator |
| `id` | id | i64 | no | preserved through import |
| `cycle_id` | cycle_id | String | no | |
| `seq` | seq | i64 | no | |
| `event_type` | event_type | String | no | |
| `phase` | phase | String/null | yes | `nullable_text` |
| `outcome` | outcome | String/null | yes | `nullable_text` |
| `next_phase` | next_phase | String/null | yes | `nullable_text` |
| `timestamp` | timestamp | i64 | no | |
| `goal` | goal | String/null | yes | `nullable_text` |

### drop_all_data DELETE Order

```sql
-- Existing (unchanged)
DELETE FROM entry_tags;           -- FK -> entries (CASCADE)
DELETE FROM co_access;
DELETE FROM feature_entries;
DELETE FROM outcome_index;
DELETE FROM agent_registry;
DELETE FROM vector_map;

-- NEW: derived metric tables before observations (SR-07)
DELETE FROM observation_phase_metrics;  -- FK -> observation_metrics (CASCADE)
DELETE FROM observation_metrics;

-- NEW: the 3 new exported tables
DELETE FROM graph_edges;
DELETE FROM observations;
DELETE FROM cycle_events;

-- Existing (unchanged)
DELETE FROM entries;
DELETE FROM counters;
```

### Format Version Validation Logic

```
match header.format_version {
    1 => Ok(()),           // legacy: 8 tables only, new _table values absent
    2 => Ok(()),           // current: all 11 tables
    v => Err(format!("unsupported format_version: {v}. This binary supports format_version 1 and 2."))
}
```
