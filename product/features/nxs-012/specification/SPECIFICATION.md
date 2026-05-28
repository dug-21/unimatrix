# Specification: nxs-012 — Export/Import Complete Persistent State Coverage

GH Issue: #631

## Objective

Extend the export/import CLI pipeline from 8 tables to 11 by adding `graph_edges`, `observations`, and `cycle_events`. This closes the semantic data loss gap exposed during workstation rebuilds: after an export/import cycle, runtime graph edges, phase affinity observations, and cycle history are currently lost permanently. The export `format_version` bumps from 1 to 2; import accepts both versions for backward compatibility.

## Functional Requirements

### FR-01: Export graph_edges table
Export all rows from `graph_edges` with 9 columns (excluding AUTOINCREMENT `id`): `source_id`, `target_id`, `relation_type`, `weight`, `created_at`, `created_by`, `source`, `bootstrap_only`, `metadata`. Each row is emitted as a JSONL line with `_table: "graph_edges"`. Rows are ordered by `(source_id, target_id, relation_type)` for deterministic output. No rows are filtered — all edges including `bootstrap_only=1` are exported.

**Verification**: Parse exported JSONL, confirm 9 data fields per `graph_edges` line, confirm no `id` field, confirm ordering matches `ORDER BY source_id, target_id, relation_type`.

### FR-02: Export observations table
Export all rows from `observations` with all 10 columns (including `id`): `id`, `session_id`, `ts_millis`, `hook`, `tool`, `input`, `response_size`, `response_snippet`, `topic_signal`, `phase`. Rows are ordered by `id` for deterministic output.

**Verification**: Parse exported JSONL, confirm 10 fields per `observations` line, confirm `id` field is present, confirm ordering matches `ORDER BY id`.

### FR-03: Export cycle_events table
Export all rows from `cycle_events` with 9 columns (excluding `goal_embedding` BLOB): `id`, `cycle_id`, `seq`, `event_type`, `phase`, `outcome`, `next_phase`, `timestamp`, `goal`. The `goal_embedding` column is excluded from the SELECT or emitted as JSON null. Rows are ordered by `id` for deterministic output.

**Verification**: Parse exported JSONL, confirm 9 data fields per `cycle_events` line, confirm `goal_embedding` is absent or null, confirm ordering matches `ORDER BY id`.

### FR-04: Bump export format_version to 2
The JSONL header line emits `format_version: 2` instead of `1`.

**Verification**: Parse header line, assert `format_version == 2`.

### FR-05: Import accepts format_version 1 (legacy)
When importing a `format_version: 1` file, the pipeline succeeds. The 3 new `_table` types are simply absent from the file, so zero rows are inserted for `graph_edges`, `observations`, and `cycle_events`.

**Verification**: Create a v1 export file with only the original 8 table types. Import succeeds without error. Import summary reports 0 for the 3 new table counts.

### FR-06: Import accepts format_version 2
When importing a `format_version: 2` file, the pipeline ingests all 11 table types. The `ingest_rows` match arms route new `ExportRow` variants to their respective inserter functions.

**Verification**: Create a v2 export file with all 11 table types populated. Import succeeds. Import summary reports non-zero counts for all 11 tables.

### FR-07: Import rejects format_version 3+
When importing a file with `format_version >= 3`, the pipeline returns an error with a message indicating the unsupported version and the supported range (1-2).

**Verification**: Create a file with `format_version: 3` header. Import returns error containing the version number and guidance.

### FR-08: Import rejects format_version 0
When importing a file with `format_version: 0`, the pipeline returns an error.

**Verification**: Create a file with `format_version: 0` header. Import returns error.

### FR-09: graph_edges import uses plain INSERT
Import inserts `graph_edges` rows with plain `INSERT INTO` (not `INSERT OR IGNORE`, not `INSERT OR REPLACE`). If duplicate `(source_id, target_id, relation_type)` tuples appear in the export file, the INSERT fails with a UNIQUE constraint violation, surfacing data corruption.

**Verification**: Craft a v2 file with two `graph_edges` rows sharing the same `(source_id, target_id, relation_type)`. Import fails with an error mentioning the UNIQUE constraint.

### FR-10: observations.id preserved through import
The `observations` inserter uses explicit `INSERT INTO observations (id, ...)` with the exported `id` value, preserving row identity across export/import cycles.

**Verification**: Export observations with known IDs. Import into fresh DB. Query `SELECT id FROM observations` and confirm IDs match the export file.

### FR-11: cycle_events.id preserved through import
The `cycle_events` inserter uses explicit `INSERT INTO cycle_events (id, ...)` with the exported `id` value, preserving row identity.

**Verification**: Export cycle_events with known IDs. Import into fresh DB. Query `SELECT id FROM cycle_events` and confirm IDs match the export file.

### FR-12: Extend drop_all_data for --force import
`drop_all_data` adds DELETE statements for `graph_edges`, `observations`, and `cycle_events`. Additionally, `observation_phase_metrics` and `observation_metrics` must be deleted before `observations` to respect the FK cascade chain: `observation_phase_metrics` -> `observation_metrics` -> (no FK to observations, but observation_metrics is a derived table that should be cleared). `graph_edges` has no FK dependents. `cycle_events` has no FK dependents.

The deletion order for new tables within `drop_all_data`:
1. `observation_phase_metrics` (FK child of `observation_metrics`)
2. `observation_metrics` (derived from observations; cleared to prevent stale aggregates)
3. `graph_edges` (no FK dependents)
4. `observations` (no FK dependents after derived tables cleared)
5. `cycle_events` (no FK dependents)

These DELETEs are added to the existing `drop_all_data` statement. None of these tables have append-only triggers.

**Verification**: Populate all 3 new tables plus `observation_metrics` and `observation_phase_metrics`. Run import with `--force`. Confirm all 5 tables are empty before ingestion begins.

### FR-13: Import summary reports new table counts
`print_summary` outputs counts for `graph_edges`, `observations`, and `cycle_events` alongside the existing 8 table counts.

**Verification**: Import a v2 file. Check stderr output includes lines for all 3 new tables with correct counts.

### FR-14: New tables appear after existing 8 in export output
In `do_export`, the 3 new export functions are called after `export_audit_log` (the last of the existing 8). Order: `export_graph_edges`, `export_observations`, `export_cycle_events`.

**Verification**: Export a populated DB. Verify that all `graph_edges` lines appear after all `audit_log` lines, all `observations` lines appear after all `graph_edges` lines, and all `cycle_events` lines appear after all `observations` lines.

### FR-15: Empty tables produce no spurious rows
When a table contains zero rows, its export function emits zero JSONL lines for that `_table` type.

**Verification**: Export a DB where `graph_edges`, `observations`, and `cycle_events` are empty. Confirm zero lines with those `_table` values in the output.

### FR-16: format.rs row structs for new tables
Add `GraphEdgeRow`, `ObservationRow`, and `CycleEventRow` deserialization structs to `format.rs`. Add 3 corresponding variants to the `ExportRow` tagged enum: `GraphEdge(GraphEdgeRow)`, `Observation(ObservationRow)`, `CycleEvent(CycleEventRow)`.

`GraphEdgeRow` fields: `source_id: i64`, `target_id: i64`, `relation_type: String`, `weight: f64`, `created_at: i64`, `created_by: String`, `source: String`, `bootstrap_only: i64`, `metadata: Option<String>`.

`ObservationRow` fields: `id: i64`, `session_id: String`, `ts_millis: i64`, `hook: String`, `tool: Option<String>`, `input: Option<String>`, `response_size: Option<i64>`, `response_snippet: Option<String>`, `topic_signal: Option<String>`, `phase: Option<String>`.

`CycleEventRow` fields: `id: i64`, `cycle_id: String`, `seq: i64`, `event_type: String`, `phase: Option<String>`, `outcome: Option<String>`, `next_phase: Option<String>`, `timestamp: i64`, `goal: Option<String>`.

**Verification**: Deserialize known JSON strings into each row struct. Confirm all fields parse correctly including nullable ones mapping `null` to `None`.

### FR-17: Record provenance for new tables
The `record_provenance` audit log entry detail string includes counts for `graph_edges`, `observations`, and `cycle_events` alongside existing counts.

**Verification**: After import, query `audit_log` for the import provenance event. Confirm the detail string mentions counts for all 3 new tables.

### FR-18: --skip-quarantined CLI flag on export subcommand
The `export` subcommand accepts an optional `--skip-quarantined` flag (boolean, default `false`). When present, the export pipeline omits quarantined entries (`status = 3`) and all rows in entry-referencing tables that reference them. When absent, export behavior is unchanged — all entries are exported regardless of status.

**Verification**: Run `unimatrix-server export --help` and confirm `--skip-quarantined` appears as an option. Run export without the flag and confirm quarantined entries are present in the output file.

### FR-19: Skip-set construction inside DEFERRED transaction
When `--skip-quarantined` is active, the export pipeline queries `SELECT id FROM entries WHERE status = 3` inside the existing `BEGIN DEFERRED` snapshot transaction, before any per-table export pass begins. The result is collected into a `HashSet<i64>` (the skip set). This query shares the same snapshot as all table exports, preventing TOCTOU races with concurrent quarantine operations.

**Verification**: Populate a database with 5 entries: 3 active (status=1), 2 quarantined (status=3). Export with `--skip-quarantined`. Confirm the skip set contains exactly the 2 quarantined entry IDs (verified indirectly by checking the export file omits those entries and their dependents).

### FR-20: Export-side filtering for entries
When `--skip-quarantined` is active, the `export_entries` function checks each entry row's `id` against the skip set. Entries whose `id` is in the skip set are not written to the export file. Entries not in the skip set are exported normally.

**Verification**: Export a database with 5 entries (3 active, 2 quarantined) using `--skip-quarantined`. Parse the JSONL output and confirm only 3 `entries` rows are present. Confirm the 2 quarantined entry IDs do not appear.

### FR-21: Export-side filtering for entry_tags
When `--skip-quarantined` is active, the `export_entry_tags` function checks each row's `entry_id` against the skip set. Rows whose `entry_id` is in the skip set are not written to the export file. Rows not in the skip set are exported normally.

**Verification**: Export a database with entry_tags referencing both quarantined and non-quarantined entries using `--skip-quarantined`. Parse JSONL output. Confirm entry_tags rows referencing quarantined entries are absent; rows referencing non-quarantined entries are present.

### FR-22: Export-side filtering for feature_entries
When `--skip-quarantined` is active, the `export_feature_entries` function checks each row's `entry_id` against the skip set. Rows whose `entry_id` is in the skip set are not written to the export file. Rows not in the skip set are exported normally.

**Verification**: Export a database with feature_entries referencing both quarantined and non-quarantined entries using `--skip-quarantined`. Parse JSONL output. Confirm feature_entries rows referencing quarantined entries are absent; rows referencing non-quarantined entries are present.

### FR-23: Export-side filtering for co_access
When `--skip-quarantined` is active, the `export_co_access` function checks each row against the skip set. If either `entry_a` or `entry_b` is in the skip set, the row is not written to the export file. Both sides must be checked because co-access is a symmetric relationship — a quarantined entry on either side renders the pair invalid.

**Verification**: Export a database with co_access rows where: (a) neither side is quarantined, (b) `entry_a` is quarantined, (c) `entry_b` is quarantined, (d) both are quarantined. Use `--skip-quarantined`. Parse JSONL output. Confirm only case (a) rows are present.

### FR-24: Export-side filtering for graph_edges
When `--skip-quarantined` is active, the `export_graph_edges` function checks each row against the skip set. If either `source_id` or `target_id` is in the skip set, the row is not written to the export file. Both sides must be checked because a directed edge referencing a quarantined entry on either endpoint is semantically invalid.

**Verification**: Export a database with graph_edges rows where: (a) neither endpoint is quarantined, (b) `source_id` is quarantined, (c) `target_id` is quarantined, (d) both are quarantined. Use `--skip-quarantined`. Parse JSONL output. Confirm only case (a) rows are present.

### FR-25: observations and cycle_events are NOT filtered
When `--skip-quarantined` is active, the `export_observations` and `export_cycle_events` functions are unaffected. These tables contain no entry ID references and are exported in full regardless of the `--skip-quarantined` flag.

**Verification**: Export a database with observations and cycle_events rows using `--skip-quarantined`. Confirm all observations and cycle_events rows are present in the output — row counts match the database.

### FR-26: --confirm safeguard for --skip-quarantined
When `--skip-quarantined` is provided without `--confirm`, the export aborts immediately with a clear error message explaining that `--skip-quarantined` produces a non-exact snapshot and `--confirm` is required to proceed. The `--confirm` flag is a CLI flag (not an interactive stdin prompt) for automation/CI compatibility.

**Verification**: Run `unimatrix-server export --skip-quarantined` without `--confirm`. Confirm the command exits with a non-zero status and an error message mentioning `--confirm`. Run with both `--skip-quarantined --confirm` and confirm the export proceeds normally.

### FR-27: Export summary reports skip counts
When `--skip-quarantined` is active, the export summary reports: (a) the count of skipped entries, and (b) the per-table count of skipped dependent rows for `entry_tags`, `feature_entries`, `co_access`, and `graph_edges`. These counts appear in addition to the standard per-table export row counts.

**Verification**: Export a database with quarantined entries and dependent rows using `--skip-quarantined --confirm`. Capture stderr output. Confirm the summary includes lines reporting the skipped entry count and per-table skipped dependent row counts.

### FR-28: Default export path unchanged
When the `--skip-quarantined` flag is not provided, the export pipeline produces identical output to the pre-feature behavior. No skip set is constructed, no status checks are performed, and no row filtering occurs. All entries (including quarantined) and all dependent rows are exported normally.

**Verification**: Export a database containing quarantined entries without `--skip-quarantined`. Confirm all entries (including status=3) and all dependent rows are present in the output file. Compare row counts to the database to verify no filtering occurred.

### FR-29: Hash integrity preserved with --skip-quarantined
The export file produced with `--skip-quarantined --confirm` has a valid content hash in its header. The hash is computed over the filtered output (after quarantined entries and dependents are excluded). Import of this file with hash validation enabled succeeds without requiring `--skip-hash-validation`.

**Verification**: Export with `--skip-quarantined --confirm`. Import the resulting file into a fresh database without `--skip-hash-validation`. Confirm import succeeds with hash validation passing.

## Non-Functional Requirements

### NFR-01: Export performance
Adding 3 new table exports must not significantly degrade export time. Target: total export time remains under 10 seconds for a database with 50K observations, 10K graph edges, and 5K cycle events (the upper bound of typical knowledge bases per SCOPE.md).

### NFR-02: Import transaction duration
All 11 table inserts execute within a single `BEGIN IMMEDIATE` transaction on a single `SqliteConnection`. The extended transaction duration from 3 additional tables is acceptable. Target: import of 50K observations completes within 30 seconds total.

### NFR-03: File size impact
New production code additions stay within acceptable bounds per the 500-line file limit rule:
- `export.rs`: ~90 lines added (3 export functions). Already exceeds 500 lines due to tests; production code addition is acceptable.
- `format.rs`: ~50 lines added (3 structs + 3 enum variants).
- `inserters.rs`: ~60 lines added (3 inserter functions). Stays well under 500 lines.
- `import/mod.rs`: ~30 lines added (ImportCounts fields, match arms, drop_all_data, print_summary).

### NFR-04: Backward compatibility
Existing 8-table export/import behavior is unchanged. The only observable difference for existing tables is the header `format_version` changing from 1 to 2.

### NFR-05: Snapshot isolation
All new export queries execute within the existing `BEGIN DEFERRED` transaction, guaranteeing a consistent snapshot across all 11 tables.

### NFR-06: Skip set memory overhead
The `HashSet<i64>` skip set for `--skip-quarantined` contains at most one entry per quarantined entry in the database. For a typical knowledge base with up to 10K entries, even if 50% are quarantined, the set consumes under 100KB (5000 x 8 bytes + hash overhead). No memory concern. The set is allocated once at the start of the export transaction and shared across all table exporters.

### NFR-07: Skip filtering performance
Skip set lookup is O(1) per row. The `--skip-quarantined` flag adds negligible overhead to export time — one `SELECT` query to build the set, then a single `HashSet::contains` call per row in the 5 entry-referencing table exporters. No measurable impact on the NFR-01 export time target.

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | `export` emits `graph_edges` rows with 9 columns (excluding `id`) as JSONL lines with `_table: "graph_edges"` | Unit test: export populated graph_edges, parse output, verify field count and names |
| AC-02 | `export` emits `observations` rows with all 10 columns (including `id`) as JSONL lines with `_table: "observations"` | Unit test: export populated observations, parse output, verify field count and names |
| AC-03 | `export` emits `cycle_events` rows with 9 columns (excluding `goal_embedding`) as JSONL lines with `_table: "cycle_events"` | Unit test: export populated cycle_events, parse output, verify `goal_embedding` absent or null |
| AC-04 | Export header `format_version` is 2 | Unit test: parse header, assert `format_version == 2` |
| AC-05 | `import` accepts `format_version` 1 (legacy) without error | Integration test: import v1 file, assert success |
| AC-06 | `import` accepts `format_version` 2 and ingests all 11 table types | Integration test: import v2 file with all tables, verify counts |
| AC-07 | `import` rejects `format_version` 3+ with a clear error message | Unit test: craft v3 header, assert error |
| AC-08 | `graph_edges` rows exported in `ORDER BY source_id, target_id, relation_type` | Unit test: insert edges in non-sorted order, export, verify output ordering |
| AC-09 | `observations` rows exported in `ORDER BY id` | Unit test: insert observations with known IDs, export, verify ordering |
| AC-10 | `cycle_events` rows exported in `ORDER BY id` | Unit test: insert cycle_events with known IDs, export, verify ordering |
| AC-11 | `graph_edges.weight` uses `Number::from_f64` with NaN fallback | Unit test: insert edge with NaN weight, export, verify JSON number (not NaN literal) |
| AC-12 | `graph_edges.metadata` nullable TEXT emitted as JSON null when SQL NULL | Unit test: insert edge with NULL metadata, export, verify JSON null |
| AC-13 | `--force` import DELETEs from `graph_edges`, `observations`, `cycle_events` (and derived metric tables) before importing | Integration test: populate tables, import with --force, verify tables empty before ingest |
| AC-14 | New tables appear after existing 8 tables in export output | Unit test: export all tables, verify line ordering by `_table` type |
| AC-15 | Round-trip test: export, import into fresh DB, re-export, compare (excluding `exported_at` and `goal_embedding`) | Integration test: full round-trip with diff |
| AC-16 | `observations.id` preserved through export/import | Integration test: insert with known IDs, export, import, query IDs |
| AC-17 | `cycle_events.id` preserved through export/import | Integration test: insert with known IDs, export, import, query IDs |
| AC-18 | `graph_edges` import uses plain INSERT (not INSERT OR IGNORE) | Integration test: import duplicate edges, assert UNIQUE constraint error |
| AC-19 | `cycle_events.goal_embedding` not present in export output | Unit test: insert cycle_event with goal_embedding, export, verify field absent or null |
| AC-20 | Import summary reports counts for all 3 new tables | Integration test: import v2 file, capture stderr, verify 3 new table count lines |
| AC-21 | All new export functions produce empty output for empty tables | Unit test: export empty DB, verify zero lines for new table types |
| AC-22 | `--skip-quarantined` CLI flag accepted by the **export** subcommand (default: off) | FR-18, FR-28. Unit test: parse CLI args with and without `--skip-quarantined`, confirm flag presence and default value |
| AC-23 | With `--skip-quarantined`, entries with `status = 3` are not emitted to the export file | FR-19, FR-20. Integration test: export DB with quarantined entries, parse JSONL, verify quarantined entries absent |
| AC-24 | With `--skip-quarantined`, `entry_tags` rows referencing skipped entry IDs are not emitted | FR-21. Integration test: verify `entry_tags` rows for quarantined entries absent from export file |
| AC-25 | With `--skip-quarantined`, `feature_entries` rows referencing skipped entry IDs are not emitted | FR-22. Integration test: verify `feature_entries` rows for quarantined entries absent from export file |
| AC-26 | With `--skip-quarantined`, `co_access` rows where either `entry_a` or `entry_b` references a skipped entry ID are not emitted | FR-23. Integration test: verify `co_access` rows with quarantined endpoints absent from export file |
| AC-27 | With `--skip-quarantined`, `graph_edges` rows where `source_id` or `target_id` references a skipped entry ID are not emitted | FR-24. Integration test: verify `graph_edges` rows with quarantined endpoints absent from export file |
| AC-28 | Export summary reports the count of skipped entries and skipped dependent rows when `--skip-quarantined` is active | FR-27. Integration test: capture stderr, verify skip counts present |
| AC-29 | Without `--skip-quarantined`, all entries (including quarantined) are exported as before — no behavioral change to default path | FR-28. Integration test: export with quarantined entries without flag, verify all present in output |
| AC-30 | `--skip-quarantined` requires `--confirm` flag to proceed — export aborts with clear message if confirmation missing | FR-26. Unit test: run export with `--skip-quarantined` but no `--confirm`, verify non-zero exit and error message |
| AC-31 | Export file produced with `--skip-quarantined` has valid hash integrity — import with hash validation succeeds without `--skip-hash-validation` | FR-29. Integration test: export with `--skip-quarantined --confirm`, import without `--skip-hash-validation`, verify success |

## Domain Models

### Entities

**graph_edges**: A directed edge in the typed knowledge graph connecting two entries. Carries a relation type (e.g., Supersedes, Contradicts, Supports, Advances), a weight, and provenance metadata. The `id` column is a synthetic AUTOINCREMENT primary key not referenced by any other table. The natural key is `(source_id, target_id, relation_type)` enforced by a UNIQUE constraint. `bootstrap_only` marks edges derivable from migrations vs. runtime-inferred edges.

**observations**: Tool-call telemetry recorded per hook invocation. Each row captures a single hook event with session context, tool name, response statistics, and phase information. Bounded by retention GC (`gc_observations`). The `id` column serves as a watermark and ordering key for the extraction tick.

**cycle_events**: Lifecycle events for feature cycles — `cycle_start`, `phase_transition`, `cycle_stop`. Records goals, outcomes, and phase transitions. The `goal_embedding` BLOB is a bincode-encoded `Vec<f32>` tied to the active ONNX model version; it is excluded from export because it creates fragile model-version coupling. The `id` column preserves event sequencing across export/import.

**observation_metrics** (derived, not exported): Aggregate metrics computed from raw `observations` rows per feature cycle. Cleared during `--force` import to prevent stale aggregates from orphaned data.

**observation_phase_metrics** (derived, not exported): Per-phase breakdown of observation metrics. Has `FOREIGN KEY (feature_cycle) REFERENCES observation_metrics(feature_cycle) ON DELETE CASCADE`. Must be cleared before `observation_metrics` during `--force` import.

### Key Terms

- **format_version**: Integer in the JSONL header that signals which table types are present. Version 1 = original 8 tables. Version 2 = 8 + graph_edges + observations + cycle_events. Importers reject unknown versions.
- **bootstrap_only**: Flag on `graph_edges` indicating edges re-derivable by schema migration (e.g., Supersedes from `entries.supersedes`). Exported as-is; no filtering applied.
- **goal_embedding**: BLOB column in `cycle_events` containing a model-version-specific embedding vector. Excluded from export. After import, goal-cluster affinity scoring in `context_briefing` gracefully degrades (NULL embedding falls back to pure semantic path) until the first cycle completion triggers lazy reconstruction.
- **NaN fallback**: `Number::from_f64()` returns `None` for NaN/Infinity. `graph_edges.weight` uses `Number::from_f64` with 1.0 fallback (ADR-003) because weight 0 would nullify edge significance; `entries.confidence` uses 0 fallback because 0 is a safe "lowest confidence" default.
- **round-trip**: Export from DB A, import into fresh DB B, re-export from DB B. The two export files must be identical (excluding `exported_at` timestamp and `goal_embedding` which is not exported).
- **skip set**: A `HashSet<i64>` of entry IDs constructed by querying `SELECT id FROM entries WHERE status = 3` inside the `BEGIN DEFERRED` snapshot transaction when `--skip-quarantined` is active on export. Built once before any per-table export pass begins. Each table exporter for entry-referencing tables (`entries`, `entry_tags`, `feature_entries`, `co_access`, `graph_edges`) checks its entry ID references against this set to determine whether to skip writing the row.
- **quarantined (status=3)**: An entry status indicating the entry has been identified as unwanted or problematic. Quarantined entries are excluded from search results and briefing rankings at runtime. The `--skip-quarantined` export flag extends this exclusion to the export pipeline, producing a clean snapshot that omits quarantined entries and their dependents — import remains a simple full-restore.

## User Workflows

### CLI Export
```
unimatrix-server export [--project-dir <path>] [--output <file>] [--skip-quarantined] [--confirm]
```
Produces a JSONL file with `format_version: 2` header followed by rows for all 11 tables. New table rows appear after the existing 8 tables. All reads occur within a `BEGIN DEFERRED` snapshot transaction.

When `--skip-quarantined` is provided with `--confirm`, the export pipeline first queries quarantined entry IDs (`status = 3`) inside the snapshot transaction to build a `HashSet<i64>` skip set. Each entry-referencing table exporter (`entries`, `entry_tags`, `feature_entries`, `co_access`, `graph_edges`) checks its entry ID references against the skip set and omits matching rows. `observations` and `cycle_events` are unaffected (no entry references). The export summary reports skip counts alongside standard row counts. The resulting file is a clean snapshot with valid hash integrity. If `--skip-quarantined` is provided without `--confirm`, the export aborts with an error message.

### CLI Import
```
unimatrix-server import <file> [--project-dir <path>] [--force] [--skip-hash-validation]
```
Reads a JSONL file with `format_version` 1 or 2. For v1 files, only the original 8 table types are present and imported. For v2 files, all 11 table types are ingested. `--force` clears all importable tables (including derived metric tables) before ingestion. After DB commit, embedding reconstruction runs. Import is a simple full-restore — no entry filtering occurs at import time.

### Post-Import Degradation Window
After importing a v2 file, `cycle_events.goal_embedding` is NULL for all rows. `context_briefing` gracefully degrades: goal-cluster affinity scoring produces neutral (no boost) results. The degradation resolves on the first cycle completion post-import, which triggers lazy goal embedding reconstruction. No manual intervention required.

## Constraints

1. **Schema v27 stable** — All 3 target tables exist in the current DDL. No schema migration changes required for this feature.

2. **format.rs tagged enum backward incompatibility** — Adding `ExportRow` variants means old binaries cannot deserialize new `_table` values. This is why `format_version` must bump to 2: old import code rejects `format_version != 1` with a clear error rather than failing on unknown table deserialization.

3. **Transaction isolation preserved** — Export: `BEGIN DEFERRED` (read snapshot). Import: `BEGIN IMMEDIATE` (write lock on single connection). New tables participate in the same transactions as existing tables.

4. **AUTOINCREMENT id handling divergence** — `graph_edges.id`: omitted from export (synthetic, unreferenced, fresh IDs assigned on import). `observations.id` and `cycle_events.id`: preserved through export/import (serve as watermarks and ordering keys).

5. **BLOB exclusion** — `cycle_events.goal_embedding` is bincode-encoded and ONNX-model-version-specific. Must not appear in export output to avoid silent model incompatibility after import.

6. **FK cascade ordering in drop_all_data** (SR-06) — `observation_phase_metrics` has `FOREIGN KEY ... ON DELETE CASCADE` to `observation_metrics`. Even though neither derived table is part of the export contract, `drop_all_data` must clear them to prevent stale derived data after `--force` import. Deletion order: `observation_phase_metrics` before `observation_metrics` before `observations`.

7. **f64 NaN safety** (SR-01) — `graph_edges.weight` is REAL (f64). `Number::from_f64()` returns `None` for NaN/Infinity. The export function must use `unwrap_or(Number::from(0))` fallback, matching the existing `entries.confidence` pattern.

8. **NULL goal_embedding graceful degradation** (SR-03) — After import, `context_briefing` must function correctly when all `cycle_events.goal_embedding` values are NULL. Goal-cluster affinity scoring falls back to the pure semantic path. This is an existing graceful degradation path, not new code.

9. **audit_log trigger** — The append-only trigger on `audit_log` prevents DELETE. `drop_all_data` already excludes `audit_log`. None of the 3 new tables have such triggers.

10. **Plain INSERT for graph_edges** (SR-05) — Import uses `INSERT INTO` (not `INSERT OR IGNORE` or `INSERT OR REPLACE`) to surface duplicate `(source_id, target_id, relation_type)` as UNIQUE constraint violations, detecting data corruption in the export file.

11. **Entry ID stability** (SR-07) — `graph_edges.source_id` and `target_id` reference entry IDs without FK constraints. The nan-002 import pattern preserves entry IDs through explicit `INSERT INTO entries (id, ...)`, so edges remain valid post-import.

12. **Skip-set query inside snapshot transaction** (SR-02) — The `SELECT id FROM entries WHERE status = 3` query that builds the skip set must execute inside the same `BEGIN DEFERRED` transaction as all table exports. This prevents TOCTOU races where an entry is quarantined between the skip-set query and the table export pass. The skip set is immutable once built.

13. **Consistent skip-set checking across 5 exporters** (SR-08) — Every entry-referencing table exporter (`entries`, `entry_tags`, `feature_entries`, `co_access`, `graph_edges`) must check the same `HashSet<i64>` skip set. A missed check in any single exporter would produce orphaned rows in the export file that reference non-existent entries. `observations` and `cycle_events` have no entry references and must NOT be filtered.

14. **--confirm is a CLI flag, not interactive** (SR-09) — The `--confirm` safeguard for `--skip-quarantined` must be a CLI flag for automation/CI compatibility. No interactive stdin prompts. This matches the nan-002 ADR-003 precedent (stderr warning, no interactive prompt).

## Dependencies

### Crates
- `sqlx` — all database queries (export reads, import writes)
- `serde` / `serde_json` — JSONL serialization and `ExportRow` tagged enum deserialization
- `unimatrix-store` — `SqlxStore`, `PoolConfig`, `compute_content_hash`, `AuditEvent`

### Existing Components
- `crates/unimatrix-server/src/export.rs` — `do_export`, `write_header`, `write_row`, `nullable_int`, `nullable_text` helpers
- `crates/unimatrix-server/src/format.rs` — `ExportHeader`, `ExportRow` enum, per-table row structs
- `crates/unimatrix-server/src/import/mod.rs` — `ImportCounts`, `ingest_rows`, `drop_all_data`, `print_summary`, `record_provenance`
- `crates/unimatrix-server/src/import/inserters.rs` — per-table `insert_*` functions

### External Services
None. Export/import is a local CLI operation with no network dependencies.

## NOT in Scope

- **goal_embedding BLOB export** — Excluded by design. Model-version-specific bincode creates fragile coupling. Lazy reconstruction post-import is the intended recovery path.
- **bootstrap_only filtering** — All edges exported regardless of `bootstrap_only` flag. Migration only re-derives edges when `schema_version < CURRENT_SCHEMA_VERSION`, which is false after same-version export/import.
- **Observations filtering/compaction** — No date-range or size-limit filtering. Retention GC already bounds the exported set.
- **observation_metrics / observation_phase_metrics export** — Derived/computed aggregates recomputed from raw observations by the metrics pipeline. Adding them creates redundant data.
- **shadow_evaluations / query_log / sessions export** — Remain operational/ephemeral tables excluded from the export contract per nan-001/nan-002 design decisions.
- **Merge/append import mode** — Import is full restore, not merge. Same constraint as nan-002.
- **Streaming/pagination** — All tables use `fetch_all`. Streaming is a future optimization.
- **Blending goal_embedding with topic signals** — Explicitly excluded by SCOPE.md Non-Goals.
- **Post-import goal_embedding reconstruction trigger** — Lazy reconstruction on first cycle completion is the design; no explicit post-import step.
